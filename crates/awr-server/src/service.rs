//! Operator-bound multi-project HTTP/MCP service. Every request authenticates
//! inside PostgreSQL; tenant/actor/client/grants are never taken from its JSON.
mod mcp;

use awr_team_pg::{
    PgError, WorkstreamCommand, WorkstreamCommandStore, WorkstreamQuery, WorkstreamReadStore,
};
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
    commands: WorkstreamCommandStore,
    projects: BTreeMap<String, ProjectBinding>,
    hosts: Vec<String>,
    permits: Arc<tokio::sync::Semaphore>,
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
        commands: store.commands(),
        store,
        projects: config
            .projects
            .into_iter()
            .map(|p| (p.key.clone(), p))
            .collect(),
        hosts,
        permits: Arc::new(tokio::sync::Semaphore::new(64)),
    });
    let mut router = Router::new()
        .route("/v1/projects/{project}/query", post(query))
        .route("/v1/projects/{project}/command", post(command))
        .layer(DefaultBodyLimit::max(65536))
        .with_state(state.clone());
    for project in state.projects.values() {
        router = router.merge(mcp::router(state.clone(), project.clone()));
    }
    Ok(router)
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
    dispatch(state, key, headers, body, false).await
}

async fn command(
    State(state): State<Arc<StateData>>,
    Path(key): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    dispatch(state, key, headers, body, true).await
}

async fn dispatch(
    state: Arc<StateData>,
    key: String,
    headers: HeaderMap,
    body: Bytes,
    write: bool,
) -> Response {
    if !allowed_request(&state, &headers) {
        return denied();
    }
    let Some(token) = bearer(&headers) else {
        return denied();
    };
    let Some(project) = state.projects.get(&key) else {
        return denied();
    };
    enum Request {
        Query(WorkstreamQuery),
        Command(WorkstreamCommand),
    }
    let parsed = if write {
        serde_json::from_slice(&body).map(Request::Command)
    } else {
        serde_json::from_slice(&body).map(Request::Query)
    };
    let request = match parsed {
        Ok(r) => r,
        Err(_) => {
            return response(
                StatusCode::BAD_REQUEST,
                json!({"code":"InvalidInput","message":"invalid workstream request"}),
            );
        }
    };
    let Ok(_permit) = state.permits.try_acquire() else {
        return response(StatusCode::SERVICE_UNAVAILABLE, json!({"code":"Busy"}));
    };
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        match request {
            Request::Query(q) => {
                state
                    .store
                    .query(&project.tenant_id, &project.project_id, token, q)
                    .await
            }
            Request::Command(c) => {
                state
                    .commands
                    .execute(&project.tenant_id, &project.project_id, token, c)
                    .await
            }
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => response(StatusCode::OK, value),
        Ok(Err(error)) => error_response(error),
        Err(_) => unavailable(),
    }
}

fn allowed_request(state: &StateData, headers: &HeaderMap) -> bool {
    !headers.contains_key("origin")
        && headers.get_all("host").iter().count() == 1
        && headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| {
                state
                    .hosts
                    .iter()
                    .any(|allowed| h.eq_ignore_ascii_case(allowed))
            })
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all("authorization").iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    value
        .to_str()
        .ok()?
        .split_once(' ')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| token)
}

fn unavailable_value() -> Value {
    json!({"code":"Unavailable","message":"request outcome unavailable; inspect a command before retrying with its original identity"})
}

fn unavailable() -> Response {
    response(StatusCode::SERVICE_UNAVAILABLE, unavailable_value())
}

fn error_response(error: PgError) -> Response {
    let (status, value) = public_error(error);
    response(status, value)
}

// Shared by HTTP and MCP; never expose SQL, URLs, credentials or source bodies.
fn public_error(error: PgError) -> (StatusCode, Value) {
    match error {
        PgError::Forbidden => (
            StatusCode::FORBIDDEN,
            json!({"code":"Forbidden","message":"access denied"}),
        ),
        PgError::Workstream(e) => match e.code() {
            "WorkstreamAccessDenied" | "WorkstreamUnavailable" => public_error(PgError::Forbidden),
            _ => (
                StatusCode::CONFLICT,
                json!({"code":e.code(),"message":e.to_string()}),
            ),
        },
        PgError::Unsupported(_) => (
            StatusCode::NOT_IMPLEMENTED,
            json!({"code":"Unsupported","message":"operation or protocol is unavailable"}),
        ),
        PgError::Protocol(_) => (
            StatusCode::BAD_REQUEST,
            json!({"code":"InvalidInput","message":"query fields or bounds are invalid"}),
        ),
        PgError::CursorExpired => (
            StatusCode::CONFLICT,
            json!({"code":"CursorExpired","message":"refresh the scoped query"}),
        ),
        e @ (PgError::PreconditionsChanged
        | PgError::IdempotencyConflict
        | PgError::EpochChanged
        | PgError::ProjectNotAvailable
        | PgError::RecoveryBlocked
        | PgError::ClaimHeld
        | PgError::LeaseExpired
        | PgError::StaleFence
        | PgError::ScopeExceeded
        | PgError::WaitOpen) => {
            let code = match e {
                PgError::PreconditionsChanged => "PreconditionsChanged",
                PgError::IdempotencyConflict => "IdempotencyConflict",
                PgError::EpochChanged => "EpochChanged",
                PgError::ProjectNotAvailable => "ProjectNotAvailable",
                PgError::ClaimHeld => "ClaimHeld",
                PgError::LeaseExpired => "LeaseExpired",
                PgError::StaleFence => "StaleFence",
                PgError::ScopeExceeded => "ScopeExceeded",
                PgError::WaitOpen => "WaitOpen",
                _ => "RecoveryBlocked",
            };
            (
                StatusCode::CONFLICT,
                json!({"code":code,"message":e.to_string()}),
            )
        }
        PgError::ContextIncomplete => (
            StatusCode::CONFLICT,
            json!({"code":"ContextIncomplete","message":"required context exceeds the requested budget"}),
        ),
        PgError::ResponseTooLarge => (
            StatusCode::CONFLICT,
            json!({"code":"ResponseTooLarge","message":"response exceeds service limit; use a smaller page or a narrower selector; inspect a command before retrying"}),
        ),
        _ => (StatusCode::SERVICE_UNAVAILABLE, unavailable_value()),
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
        json!({"service":"awr-team-workstream","listen":actual.to_string(),"protocol_version":1})
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|_| "Team HTTP service stopped with an error".into())
}
