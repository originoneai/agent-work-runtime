#![cfg(feature = "pg-tests")]
#[path = "../../awr-team-pg/tests/common/mod.rs"]
mod common;
#[path = "../../awr-team-pg/tests/fixtures/workstream_access.rs"]
mod fixture;
use awr_server::service::{ProjectBinding, ServiceConfig};
use awr_team_pg::WorkstreamReadStore;
use fixture::*;
use rmcp::{
    RoleClient, ServiceExt,
    model::CallToolRequestParams,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::time::Duration;

type Client = RunningService<RoleClient, ()>;

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
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}
async fn connect(server: &Server, alias: &str, token: &str) -> Result<Client, String> {
    let transport = StreamableHttpClientTransport::with_client(
        http(),
        StreamableHttpClientTransportConfig::with_uri(format!("{}/{alias}/mcp", server.url))
            .auth_header(token),
    );
    tokio::time::timeout(Duration::from_secs(10), ().serve(transport))
        .await
        .map_err(|_| "timeout".to_owned())?
        .map_err(|e| e.to_string())
}
async fn call(client: &Client, name: &str, args: Value, error: bool) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(name.to_owned())
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(result.is_error.unwrap_or(false), error, "{result:?}");
    result.structured_content.unwrap()
}
async fn prepared(client: &Client) -> Value {
    call(
        client,
        "awr_team_query",
        json!({"protocol_version":1,"op":"work.prepare","work_id":"a"}),
        false,
    )
    .await
}
fn rpc(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
}
fn raw(server: &Server, token: &str) -> reqwest::RequestBuilder {
    http()
        .post(format!("{}/one/mcp", server.url))
        .bearer_auth(token)
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-03-26")
}

#[tokio::test]
async fn sdk_clients_negotiate_tools_and_isolate_workstreams_on_one_service() {
    let (_guard, _, _, store) = setup().await;
    let server = start(store).await;
    let (a, b) = tokio::join!(connect(&server, "one", A), connect(&server, "one", B));
    let (a, b) = (a.unwrap(), b.unwrap());
    let info = a.peer_info().unwrap();
    assert_eq!(info.server_info.as_ref().unwrap().name, "awr-team-mcp");
    assert!(info.capabilities.tools.is_some());
    let tools = a.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 2);
    let caps = call(
        &a,
        "awr_team_query",
        json!({"protocol_version":1,"op":"capabilities"}),
        false,
    )
    .await;
    for (tool, ops, read) in [
        ("awr_team_query", "queries", true),
        ("awr_team_command", "commands", false),
    ] {
        let t = tools.iter().find(|t| t.name == tool).unwrap();
        assert_eq!(t.annotations.as_ref().unwrap().read_only_hint, Some(read));
        assert_eq!(t.input_schema["properties"]["op"]["enum"], caps[ops]);
    }
    assert_eq!(caps["execution_admission"], false);
    let args = json!({"protocol_version":1,"op":"work.list"});
    let (ar, br) = tokio::join!(
        call(&a, "awr_team_query", args.clone(), false),
        call(&b, "awr_team_query", args, false)
    );
    assert_eq!(ar["data"]["total"], 2);
    assert!(!ar.to_string().contains("private"));
    assert_eq!(br["data"]["items"][0]["work_id"], "b-private");
    for work in ["b-private", "does-not-exist"] {
        let r = call(
            &a,
            "awr_team_query",
            json!({"protocol_version":1,"op":"work.prepare","work_id":work}),
            true,
        )
        .await;
        assert_eq!(r, json!({"code":"Forbidden","message":"access denied"}));
    }
    let dep = call(
        &a,
        "awr_team_query",
        json!({"protocol_version":1,"op":"work.prepare","work_id":"c"}),
        false,
    )
    .await;
    assert_eq!(dep["data"]["context_complete"], false);
    assert_eq!(dep["data"]["dependency_export_unavailable"], true);
    assert!(!dep.to_string().contains("b-private"));
    assert!(connect(&server, "other", A).await.is_err());
    assert!(connect(&server, "one", NONE).await.is_err());
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_session_journal_reconnects_replays_and_shares_http_outcomes() {
    let (_guard, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let server = start(store).await;
    let a = connect(&server, "one", A).await.unwrap();
    let b = connect(&server, "one", B).await.unwrap();
    let prep = prepared(&a).await;
    let request = serde_json::to_value(command(
        &prep,
        "mcp-start",
        "session.start",
        json!({"conversation_id":"sdk-conversation"}),
    ))
    .unwrap();
    let started = call(&a, "awr_team_command", request.clone(), false).await;
    let id = started["receipt"]["data"]["session_id"].as_str().unwrap();
    a.cancel().await.unwrap(); // MCP connection lifetime is not durable session lifetime.
    let a = connect(&server, "one", A).await.unwrap();
    let inspected = call(
        &a,
        "awr_team_query",
        json!({"protocol_version":1,"op":"command.inspect","work_id":"a","request_id":"mcp-start"}),
        false,
    )
    .await;
    assert_eq!(inspected["data"]["receipt"], started["receipt"]);
    let replay = call(&a, "awr_team_command", request.clone(), false).await;
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["receipt"], started["receipt"]);
    let http_replay: Value = http()
        .post(format!("{}/one/command", server.url))
        .bearer_auth(A)
        .json(&request)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(http_replay, replay);
    let mut changed = request.clone();
    changed["args"]["conversation_id"] = json!("different-intent");
    assert_eq!(
        call(&a, "awr_team_command", changed, true).await["code"],
        "IdempotencyConflict"
    );
    assert_eq!(
        call(&b, "awr_team_command", request, true).await["code"],
        "Forbidden"
    );
    let prep = prepared(&a).await;
    let checkpoint=serde_json::to_value(command(&prep,"mcp-checkpoint","session.checkpoint",json!({"session_id":id,
        "expected_session_version":"1","context_hash":prep["data"]["context_hash"],"next_action":"Review the SDK integration","open_loops":["business acceptance pending"]}))).unwrap();
    let mut mismatch = checkpoint.clone();
    mismatch["args"]["context_hash"] = json!("0".repeat(64));
    assert_eq!(
        call(&a, "awr_team_command", mismatch, true).await["code"],
        "PreconditionsChanged"
    );
    call(&a, "awr_team_command", checkpoint, false).await;
    let session = call(
        &a,
        "awr_team_query",
        json!({"protocol_version":1,"op":"session.inspect","session_id":id}),
        false,
    )
    .await;
    assert_eq!(
        session["data"]["items"][0]["next_action"],
        "Review the SDK integration"
    );
    let end = serde_json::to_value(command(
        &prepared(&a).await,
        "mcp-end",
        "session.end",
        json!({"session_id":id,"expected_session_version":"2"}),
    ))
    .unwrap();
    call(&a, "awr_team_command", end, false).await;
    let unknown=call(&a,"awr_team_query",json!({"protocol_version":1,"op":"command.inspect","work_id":"a","request_id":"not-submitted"}),false).await;
    assert_eq!(unknown["data"]["state"], "unknown");
    let count: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.operations WHERE client_id='cli-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 3);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
}

#[tokio::test]
async fn initialized_clients_recheck_revocation_for_tools_and_discovery() {
    let (_guard, admin, _, store) = setup().await;
    let server = start(store).await;
    let a = connect(&server, "one", A).await.unwrap();
    let b = connect(&server, "one", B).await.unwrap();
    let request = serde_json::to_value(command(
        &prepared(&a).await,
        "reader-cannot-write",
        "session.start",
        json!({"conversation_id":"reader"}),
    ))
    .unwrap();
    assert_eq!(
        call(&a, "awr_team_command", request, true).await["code"],
        "Forbidden"
    );
    admin
        .batch_execute(
            "UPDATE awr_team.credentials SET revoked_at=clock_timestamp() WHERE id='reader-a'",
        )
        .await
        .unwrap();
    assert!(a.list_all_tools().await.is_err());
    assert!(
        a.call_tool(
            CallToolRequestParams::new("awr_team_query").with_arguments(
                json!({"protocol_version":1,"op":"work.list"})
                    .as_object()
                    .unwrap()
                    .clone()
            )
        )
        .await
        .is_err()
    );
    assert!(b.list_all_tools().await.is_ok());
    admin.batch_execute("UPDATE awr_team.workstream_grants SET can_read=false,grant_version=grant_version+1 WHERE client_id='cli-b'").await.unwrap();
    assert!(b.list_all_tools().await.is_err());
    let count: i64 = admin
        .query_one("SELECT count(*) FROM awr_team.operations", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    a.cancel().await.unwrap();
    b.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_rejects_forged_identity_unsupported_operations_and_context_truncation() {
    let (_guard, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let server = start(store).await;
    let a = connect(&server, "one", A).await.unwrap();
    for field in [
        "tenant_id",
        "project_id",
        "actor_id",
        "client_id",
        "grants",
        "project",
    ] {
        let mut query = json!({"protocol_version":1,"op":"capabilities"});
        query[field] = json!("forged");
        assert_eq!(
            call(&a, "awr_team_query", query, true).await["code"],
            "InvalidInput"
        );
        let mut c = serde_json::to_value(command(
            &prepared(&a).await,
            "forged",
            "session.start",
            json!({"conversation_id":"fake"}),
        ))
        .unwrap();
        c[field] = json!("forged");
        assert_eq!(
            call(&a, "awr_team_command", c, true).await["code"],
            "InvalidInput"
        );
    }
    for op in ["execution.start", "work.complete", "claim.acquire"] {
        let c = serde_json::to_value(command(&prepared(&a).await, "unimplemented", op, json!({})))
            .unwrap();
        assert_eq!(
            call(&a, "awr_team_command", c, true).await["code"],
            "Unsupported"
        );
    }
    assert_eq!(
        call(
            &a,
            "awr_team_query",
            json!({"protocol_version":2,"op":"capabilities"}),
            true
        )
        .await["code"],
        "Unsupported"
    );
    assert_eq!(
        call(
            &a,
            "awr_team_query",
            json!({"protocol_version":1,"op":"work.prepare","work_id":"a","max_context_bytes":1}),
            true
        )
        .await["code"],
        "ContextIncomplete"
    );
    let count: i64 = admin
        .query_one("SELECT count(*) FROM awr_team.operations", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    a.cancel().await.unwrap();
}

#[tokio::test]
async fn mcp_http_perimeter_bounds_bodies_and_refuses_sticky_transport_authority() {
    let (_guard, _, _, store) = setup().await;
    let server = start(store).await;
    let request = rpc("tools/list", json!({}));
    let response = raw(&server, A).json(&request).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers().get("mcp-session-id").is_none());
    for (header, value) in [("host", "attacker.invalid"), ("origin", "http://localhost")] {
        assert_eq!(
            raw(&server, A)
                .header(header, value)
                .json(&request)
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    assert_eq!(
        raw(&server, A)
            .header("authorization", format!("Bearer {B}"))
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        raw(&server, "invalid")
            .header("mcp-session-id", "copied-session")
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        raw(&server, A)
            .header("content-type", "application/json")
            .body("x".repeat(65537))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
    let bad = raw(&server, A)
        .header("content-type", "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    // RMCP reports an undecodable JSON body as unsupported media (415).
    assert_eq!(bad.status(), 415);
    assert!(!bad.text().await.unwrap().contains(A));
    let methods = raw(&server, A)
        .json(&rpc(
            "tools/call",
            json!({"name":"awr_work_complete","arguments":{}}),
        ))
        .send()
        .await
        .unwrap();
    let result: Value = methods.json().await.unwrap();
    assert!(result.get("error").is_some() || result["result"]["isError"] == true);
}

#[tokio::test]
async fn mcp_envelope_limit_never_returns_a_partial_large_checkpoint() {
    let (_guard, admin, _, store) = setup().await;
    // Operator/imported historical data can exceed today's journal write limit.
    let long = "PRIVATE_LARGE_CONTENT".repeat(30000);
    admin
        .execute(
            "UPDATE awr_team.checkpoints SET next_action=$1 WHERE id='cp-session-a'",
            &[&long],
        )
        .await
        .unwrap();
    let server = start(store).await;
    let result=raw(&server,A).json(&rpc("tools/call",json!({"name":"awr_team_query","arguments":{"protocol_version":1,"op":"session.inspect","session_id":"session-a"}}))).send().await.unwrap();
    assert_eq!(result.status(), 409);
    assert_eq!(result.headers()["cache-control"], "no-store");
    let body = result.text().await.unwrap();
    assert!(body.contains("ResponseTooLarge"));
    assert!(!body.contains("PRIVATE_LARGE_CONTENT"));
}
