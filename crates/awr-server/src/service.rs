//! Operator-bound multi-project HTTP read service. Every request authenticates
//! inside PostgreSQL; tenant/actor/client/grants are never taken from its JSON.
use awr_team_pg::{PgError, WorkstreamQuery, WorkstreamReadStore};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap, net::SocketAddr, path::Path as FilePath, sync::Arc, time::Duration,
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBinding {
    pub key: String,
    pub tenant_id: String,
    pub project_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    pub version: u32,
    pub listen: SocketAddr,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    pub projects: Vec<ProjectBinding>,
}

impl ServiceConfig {
    pub fn read(path: &FilePath) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|_| "could not read service configuration".to_string())?;
        if text.len() > 262144 {
            return Err("service configuration is too large".into());
        }
        let config: Self =
            toml::from_str(&text).map_err(|_| "invalid service configuration".to_string())?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.projects.is_empty() || self.projects.len() > 1024 {
            return Err("unsupported version or project count".into());
        }
        let mut keys = std::collections::BTreeSet::new();
        for p in &self.projects {
            if p.key.is_empty()
                || p.key.len() > 128
                || !p
                    .key
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
                || !keys.insert(&p.key)
                || [&p.tenant_id, &p.project_id]
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
            {
                return Err("invalid or duplicate project binding".into());
            }
        }
        if self.allowed_hosts.len() > 64
            || self.allowed_hosts.iter().any(|s| {
                s.is_empty()
                    || s.len() > 255
                    || s.chars().any(|c| c.is_whitespace())
                    || s.contains(['*', '/', '@'])
            })
        {
            return Err("allowed_hosts must be exact host authorities".into());
        }
        if !self.listen.ip().is_loopback() && self.allowed_hosts.is_empty() {
            return Err(
                "remote listeners require explicit allowed_hosts and operator TLS termination"
                    .into(),
            );
        }
        Ok(())
    }
}

struct StateData {
    store: WorkstreamReadStore,
    projects: BTreeMap<String, ProjectBinding>,
    hosts: Vec<String>,
    permits: tokio::sync::Semaphore,
}

pub fn router(
    config: ServiceConfig,
    actual: SocketAddr,
    store: WorkstreamReadStore,
) -> Result<Router, String> {
    config.validate()?;
    let mut hosts = config.allowed_hosts;
    if actual.ip().is_loopback() {
        hosts.push(actual.to_string());
        hosts.push(format!("localhost:{}", actual.port()));
    }
    let state = Arc::new(StateData {
        store,
        projects: config
            .projects
            .into_iter()
            .map(|p| (p.key.clone(), p))
            .collect(),
        hosts,
        permits: tokio::sync::Semaphore::new(64),
    });
    Ok(Router::new()
        .route("/v1/projects/{project}/query", post(query))
        .layer(DefaultBodyLimit::max(65536))
        .with_state(state))
}

fn response(status: StatusCode, value: Value) -> Response {
    (
        status,
        [
            ("content-type", "application/json"),
            ("cache-control", "no-store"),
        ],
        value.to_string(),
    )
        .into_response()
}

fn denied() -> Response {
    response(
        StatusCode::FORBIDDEN,
        json!({"code":"Forbidden","message":"access denied"}),
    )
}

async fn query(
    State(state): State<Arc<StateData>>,
    Path(key): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let host = headers.get("host").and_then(|h| h.to_str().ok());
    if headers.contains_key("origin")
        || !host.is_some_and(|h| {
            state
                .hosts
                .iter()
                .any(|allowed| h.eq_ignore_ascii_case(allowed))
        })
    {
        return denied();
    }
    let values: Vec<_> = headers.get_all("authorization").iter().collect();
    let token = if values.len() == 1 {
        values[0]
            .to_str()
            .ok()
            .and_then(|s| s.split_once(' '))
            .filter(|(kind, _)| kind.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| token)
    } else {
        None
    };
    let Some(token) = token else {
        return denied();
    };
    let Some(project) = state.projects.get(&key) else {
        return denied();
    };
    let request: WorkstreamQuery = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => {
            return response(
                StatusCode::BAD_REQUEST,
                json!({"code":"InvalidInput","message":"invalid workstream query"}),
            );
        }
    };
    let Ok(_permit) = state.permits.try_acquire() else {
        return response(StatusCode::SERVICE_UNAVAILABLE, json!({"code":"Busy"}));
    };
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        state
            .store
            .query(&project.tenant_id, &project.project_id, token, request),
    )
    .await;
    match result {
        Ok(Ok(value)) => response(StatusCode::OK, value),
        Ok(Err(PgError::Forbidden)) => denied(),
        Ok(Err(PgError::Workstream(e))) => match e.code() {
            "WorkstreamAccessDenied" | "WorkstreamUnavailable" => denied(),
            _ => response(
                StatusCode::CONFLICT,
                json!({"code":e.code(),"message":e.to_string()}),
            ),
        },
        Ok(Err(PgError::Unsupported(_))) => response(
            StatusCode::NOT_IMPLEMENTED,
            json!({"code":"Unsupported","message":"operation or protocol is unavailable"}),
        ),
        Ok(Err(PgError::Protocol(_))) => response(
            StatusCode::BAD_REQUEST,
            json!({"code":"InvalidInput","message":"query fields or bounds are invalid"}),
        ),
        Ok(Err(PgError::CursorExpired)) => response(
            StatusCode::CONFLICT,
            json!({"code":"CursorExpired","message":"refresh the scoped query"}),
        ),
        Ok(Err(PgError::ContextIncomplete)) => response(
            StatusCode::CONFLICT,
            json!({"code":"ContextIncomplete","message":"required context exceeds the requested budget"}),
        ),
        Ok(Err(PgError::ResponseTooLarge)) => response(
            StatusCode::CONFLICT,
            json!({"code":"ResponseTooLarge","message":"response exceeds service limit; use a smaller page or a narrower selector"}),
        ),
        // Never return SQL, driver errors, connection strings or source bodies.
        _ => response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"code":"Unavailable","message":"query could not be completed"}),
        ),
    }
}

pub async fn serve(path: &FilePath) -> Result<(), String> {
    let config = ServiceConfig::read(path)?;
    let url = std::env::var("AWR_TEAM_DATABASE_URL")
        .map_err(|_| "AWR_TEAM_DATABASE_URL is required".to_string())?;
    let client = awr_team_pg::connect(&url)
        .await
        .map_err(|_| "could not connect to Team database".to_string())?;
    awr_team_pg::check_schema(&client)
        .await
        .map_err(|_| "Team schema is incompatible; migrate explicitly as owner".to_string())?;
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|_| "could not bind Team listener".to_string())?;
    let actual = listener
        .local_addr()
        .map_err(|_| "could not inspect listener".to_string())?;
    let router = router(config, actual, WorkstreamReadStore::new(url))?;
    println!(
        "{}",
        json!({"service":"awr-team-workstream-read","listen":actual.to_string(),"protocol_version":1})
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|_| "Team HTTP service stopped with an error".into())
}
