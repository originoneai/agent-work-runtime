#![cfg(feature = "pg-tests")]
#[path = "../../awr-team-pg/tests/common/mod.rs"]
mod common;
#[path = "../../awr-team-pg/tests/fixtures/workstream_access.rs"]
mod fixture;
use awr_server::service::{ProjectBinding, ServiceConfig};
use awr_team_pg::WorkstreamReadStore;
use fixture::*;
use serde_json::{Value, json};

struct Server {
    url: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn start(store: WorkstreamReadStore) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = ServiceConfig {
        version: 1,
        listen: address,
        allowed_hosts: vec![],
        projects: vec![
            ProjectBinding {
                key: "one".into(),
                tenant_id: TENANT.into(),
                project_id: PROJECT.into(),
            },
            ProjectBinding {
                key: "other".into(),
                tenant_id: "other-tenant".into(),
                project_id: PROJECT.into(),
            },
        ],
    };
    let router = awr_server::service::router(config, address, store).unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Server {
        url: format!("http://{address}/v1/projects"),
        task,
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}
async fn post(server: &Server, key: &str, token: &str, body: Value) -> reqwest::Response {
    http()
        .post(format!("{}/{key}/query", server.url))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn one_http_service_isolates_parallel_clients_and_projects_using_real_pg() {
    let (_guard, _, _, store) = setup().await;
    let server = start(store).await;
    let body = json!({"protocol_version":1,"op":"work.list"});
    let (a, b) = tokio::join!(
        post(&server, "one", A, body.clone()),
        post(&server, "one", B, body.clone())
    );
    assert_eq!(a.status(), 200);
    assert_eq!(b.status(), 200);
    let a: Value = a.json().await.unwrap();
    let b: Value = b.json().await.unwrap();
    assert_eq!(a["data"]["total"], 2);
    assert!(!a.to_string().contains("b-private"));
    assert_eq!(b["data"]["total"], 1);
    assert_eq!(b["data"]["items"][0]["work_id"], "b-private");
    assert_eq!(post(&server, "other", A, body.clone()).await.status(), 403);
    assert_eq!(post(&server, "not-configured", A, body).await.status(), 403);
    for work in ["b-private", "absent"] {
        let response = post(
            &server,
            "one",
            A,
            json!({"protocol_version":1,"op":"work.prepare","work_id":work}),
        )
        .await;
        assert_eq!(response.status(), 403);
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"code":"Forbidden","message":"access denied"})
        );
    }
    let result = post(
        &server,
        "one",
        A,
        json!({"protocol_version":1,"op":"work.prepare","work_id":"c"}),
    )
    .await;
    let result: Value = result.json().await.unwrap();
    assert_eq!(result["data"]["dependency_export_unavailable"], true);
    assert!(!result.to_string().contains("b-private"));
}

#[tokio::test]
async fn http_requires_live_auth_and_never_accepts_grants_or_identity_from_a_body() {
    let (_guard, admin, _, store) = setup().await;
    let server = start(store).await;
    let body = json!({"protocol_version":1,"op":"capabilities"});
    assert_eq!(
        http()
            .post(format!("{}/one/query", server.url))
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(post(&server, "one", NONE, body.clone()).await.status(), 403);
    let cap: Value = post(&server, "one", A, body.clone())
        .await
        .json()
        .await
        .unwrap();
    assert!(cap["commands"].as_array().unwrap().is_empty());
    assert_eq!(cap["execution_admission"], false);
    for field in ["tenant_id", "project_id", "actor_id", "client_id", "grants"] {
        let mut request = body.clone();
        request[field] = json!("forged");
        let response = post(&server, "one", A, request).await;
        assert_eq!(response.status(), 400);
        let text = response.text().await.unwrap();
        assert!(!text.contains(A));
        assert!(!text.contains("reader-tenant"));
    }
    admin
        .batch_execute(
            "UPDATE awr_team.credentials SET revoked_at=clock_timestamp() WHERE id='reader-a'",
        )
        .await
        .unwrap();
    assert_eq!(post(&server, "one", A, body.clone()).await.status(), 403);
    assert_eq!(post(&server, "one", B, body).await.status(), 200);
}

#[tokio::test]
async fn transport_bounds_hosts_origins_cursor_replay_and_unknown_operations() {
    let (_guard, _, _, store) = setup().await;
    let server = start(store).await;
    let body = json!({"protocol_version":1,"op":"work.list","limit":1});
    let response = post(&server, "one", A, body).await;
    assert_eq!(response.headers()["cache-control"], "no-store");
    let first: Value = response.json().await.unwrap();
    assert_eq!(post(&server,"one",B,json!({"protocol_version":1,"op":"work.list","limit":1,"cursor":first["data"]["next_cursor"]})).await.status(),409);
    let body = json!({"protocol_version":1,"op":"capabilities"});
    for (header, value) in [
        ("origin", "https://untrusted.invalid"),
        ("host", "untrusted.invalid"),
    ] {
        assert_eq!(
            http()
                .post(format!("{}/one/query", server.url))
                .bearer_auth(A)
                .header(header, value)
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    assert_eq!(
        post(
            &server,
            "one",
            A,
            json!({"protocol_version":2,"op":"capabilities"})
        )
        .await
        .status(),
        501
    );
    assert_eq!(
        post(
            &server,
            "one",
            A,
            json!({"protocol_version":1,"op":"claim.acquire"})
        )
        .await
        .status(),
        501
    );
    assert_eq!(
        http()
            .post(format!("{}/one/query", server.url))
            .bearer_auth(A)
            .body("x".repeat(65537))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
}

#[tokio::test]
async fn http_preserves_context_limits_and_labels_stale_recovery() {
    let (_guard, admin, _, store) = setup().await;
    let server = start(store).await;
    let response = post(
        &server,
        "one",
        A,
        json!({"protocol_version":1,"op":"work.prepare","work_id":"a","max_context_bytes":1}),
    )
    .await;
    assert_eq!(response.status(), 409);
    assert_eq!(
        response.json::<Value>().await.unwrap()["code"],
        "ContextIncomplete"
    );
    let recovery: Value = post(
        &server,
        "one",
        A,
        json!({"protocol_version":1,"op":"work.recovery","work_id":"a"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        recovery["data"]["items"][0]["contract_matches_current"],
        false
    );
    assert_eq!(recovery["data"]["automatic_resume"], false);
    admin.batch_execute("UPDATE awr_team.checkpoints SET next_action=repeat('x',1048576) WHERE session_id='session-a'").await.unwrap();
    let response = post(
        &server,
        "one",
        A,
        json!({"protocol_version":1,"op":"work.recovery","work_id":"a"}),
    )
    .await;
    assert_eq!(response.status(), 409);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], "ResponseTooLarge");
    assert!(body.get("data").is_none());
}
