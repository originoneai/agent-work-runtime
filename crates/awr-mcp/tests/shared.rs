//! Real TCP/HTTP requests to the native MCP process with independent synthetic projects.
use awr_core::*;
use awr_source::{Manifest, index_project};
use awr_store::Store;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    time::timeout,
};

const WRITER: &str = "synthetic-writer-credential-for-http-fixtures";
const READER: &str = "synthetic-reader-credential-for-http-fixtures";
const COLLEAGUE: &str = "synthetic-colleague-credential-for-http-fixtures";

struct ProjectFixture {
    root: PathBuf,
    id: Id,
}
impl ProjectFixture {
    fn new(title: &str) -> Self {
        let path = std::env::temp_dir().join(format!("awr-shared-project-{}", Id::new()));
        fs::create_dir_all(path.join(".awr")).unwrap();
        let root = path.canonicalize().unwrap();
        fs::write(root.join(".awr/project.toml"), "[project]\nname='Shared MCP fixture'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        fs::write(root.join("work.yaml"), format!("goals:\n- id: GUIDE\n  title: Deliver useful team guidance\n  status: active\n  summary: Review and deliver the team guide and follow-up\n  success_criteria: [The reviewed guidance is available]\nwork_items:\n- id: W\n  title: {title}\n  status: ready\n  goal: GUIDE\n  next_action: Draft the guide\n  acceptance: [Deliver the reviewed guide]\n- id: NEXT\n  title: Prepare follow-up\n  status: ready\n  goal: GUIDE\n  next_action: Draft the follow-up\n  acceptance: [Deliver the follow-up]\n")).unwrap();
        let mut store = Store::open(&root.join(".awr/state.db")).unwrap();
        let report =
            index_project(&mut store, &root, &Manifest::load(&root).unwrap(), false).unwrap();
        assert!(report.ok);
        Self {
            root,
            id: report.project_id,
        }
    }
    fn revision(&self) -> Revision {
        Store::open_readonly(&self.root.join(".awr/state.db"))
            .unwrap()
            .project(self.id)
            .unwrap()
            .project_revision
    }
}
impl Drop for ProjectFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Server {
    child: Child,
    url: String,
}
impl Server {
    async fn start(registry: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_awr-mcp"))
            .arg("--registry")
            .arg(registry)
            .arg("--listen")
            .arg("127.0.0.1:0")
            .env("AWR_FIXTURE_WRITER", WRITER)
            .env("AWR_FIXTURE_READER", READER)
            .env("AWR_FIXTURE_COLLEAGUE", COLLEAGUE)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
        let line = timeout(Duration::from_secs(15), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let url = line
            .strip_prefix("AWR MCP listening at ")
            .unwrap_or_else(|| panic!("startup: {line}"))
            .to_owned();
        Self { child, url }
    }
    async fn stop(&mut self) {
        self.child.kill().await.unwrap();
        self.child.wait().await.unwrap();
    }
    async fn call(&self, credential: &str, name: &str, mut arguments: Value) -> Value {
        if awr_mcp::tools()
            .iter()
            .find(|tool| tool.name == name)
            .is_some_and(|tool| tool.annotations.as_ref().unwrap().read_only_hint == Some(false))
            && name != "awr_operation_recover"
            && arguments.get("request_id").is_none()
        {
            arguments["request_id"] = json!(Id::new().to_string());
        }
        let response = reqwest::Client::new().post(&self.url).bearer_auth(credential)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2026-07-28")
            .header("Mcp-Method", "tools/call")
            .header("Mcp-Name", name)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":name,"arguments":arguments,"_meta":{
                    "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities":{},
                    "io.modelcontextprotocol/clientInfo":{"name":"synthetic-http-client","version":"1"}
                }}})).send().await.unwrap();
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert_eq!(status, 200, "{body}");
        assert!(body.get("error").is_none(), "{body}");
        body["result"].clone()
    }
}
fn ok(result: Value) -> Value {
    assert_ne!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}
fn error(result: Value, code: &str) {
    assert_eq!(result["isError"], true, "{result}");
    assert_eq!(result["structuredContent"]["code"], code, "{result}");
}
fn registry(a: &ProjectFixture, b: &ProjectFixture) -> PathBuf {
    let path = a.root.join("service.toml");
    fs::write(&path, format!("version=1\n[[projects]]\nkey='alpha'\nroot={}\nproject_id='{}'\n[[projects]]\nkey='beta'\nroot={}\nproject_id='{}'\n[[clients]]\nid='writer'\ntoken_env='AWR_FIXTURE_WRITER'\nwrite=['alpha','beta']\n[[clients]]\nid='reader'\ntoken_env='AWR_FIXTURE_READER'\nread=['alpha']\n[[clients]]\nid='colleague'\ntoken_env='AWR_FIXTURE_COLLEAGUE'\nwrite=['alpha']\n", json!(a.root), a.id, json!(b.root), b.id)).unwrap();
    path
}

fn start_args(
    project: &str,
    work: &str,
    conversation: &str,
    revision: Revision,
    claim: bool,
) -> Value {
    json!({"project":project,"work":work,"conversation":conversation,"agent":"guide-editor","provider":"synthetic","model":"fixture","expected_revision":revision,"claim":claim,"request_id":Id::new().to_string()})
}

#[tokio::test]
async fn lifecycle_binds_conversations_and_preserves_checkpoints_across_connections_and_restart() {
    let a = ProjectFixture::new("Write the team guide");
    let b = ProjectFixture::new("Write the other guide");
    let config = registry(&a, &b);
    let mut server = Server::start(&config).await;
    let args = start_args("alpha", "W", "guide-discussion", a.revision(), true);
    let started = ok(server.call(WRITER, "awr_session_start", args.clone()).await);
    let sid = started["session"]["id"].clone();
    let revision = a.revision();
    let repeat = ok(server.call(WRITER, "awr_session_start", args).await);
    assert_eq!(repeat["session"]["id"], sid);
    assert_eq!(repeat["operation"]["replayed"], true);
    assert_eq!(a.revision(), revision);
    error(
        server
            .call(
                WRITER,
                "awr_session_start",
                start_args("alpha", "NEXT", "guide-discussion", a.revision(), false),
            )
            .await,
        "SourceConflict",
    );
    let other = ok(server
        .call(
            WRITER,
            "awr_session_start",
            start_args("alpha", "NEXT", "follow-up", a.revision(), true),
        )
        .await);
    let beta = ok(server
        .call(
            WRITER,
            "awr_session_start",
            start_args("beta", "W", "guide-discussion", b.revision(), true),
        )
        .await);
    assert_ne!(sid, beta["session"]["id"]);
    let colleague = ok(server
        .call(
            COLLEAGUE,
            "awr_session_start",
            start_args("alpha", "W", "guide-discussion", a.revision(), false),
        )
        .await);
    assert_ne!(sid, colleague["session"]["id"]);
    error(server.call(COLLEAGUE,"awr_session_end",json!({"project":"alpha","session":sid,"outcome":"ended","expected_revision":a.revision()})).await,"RuleViolation");
    error(server.call(WRITER,"awr_session_get",json!({"project":"alpha","session":other["session"]["id"],"conversation":"guide-discussion"})).await,"RuleViolation");
    let context = ok(server
        .call(
            WRITER,
            "awr_context_compile",
            json!({"project":"alpha","conversation":"guide-discussion"}),
        )
        .await);
    let checkpoint=ok(server.call(WRITER,"awr_session_checkpoint",json!({"project":"alpha","conversation":"guide-discussion","expected_revision":a.revision(),"context_hash":context["work_context"]["context_hash"],"digest":"The introduction is drafted; review examples next.","next_action":"Review the examples","open_loops":["Confirm the example order"]})).await);
    let cp = checkpoint["checkpoint"]["id"].clone();
    let listed = ok(server
        .call(
            WRITER,
            "awr_session_list",
            json!({"project":"alpha","limit":1}),
        )
        .await);
    assert_eq!(listed["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(listed["sessions"][0]["id"], other["session"]["id"]);
    let older = ok(server
        .call(
            WRITER,
            "awr_session_list",
            json!({"project":"alpha","limit":1,"before_revision":listed["next_before_revision"]}),
        )
        .await);
    assert_eq!(older["sessions"][0]["id"], sid);
    server.stop().await;
    let mut server = Server::start(&config).await;
    let restored = ok(server
        .call(
            WRITER,
            "awr_session_get",
            json!({"project":"alpha","conversation":"guide-discussion"}),
        )
        .await);
    assert_eq!(restored["session"]["id"], sid);
    assert_eq!(restored["session"]["status"], "active");
    assert_eq!(restored["checkpoint"]["id"], cp);
    let resumed=ok(server.call(WRITER,"awr_session_resume",json!({"project":"alpha","session":sid,"conversation":"guide-discussion","agent":"guide-editor","provider":"synthetic","model":"fixture","expected_revision":a.revision()})).await);
    assert_eq!(resumed["checkpoint_id"], cp);
    assert_ne!(resumed["resumed"]["session"]["id"], sid);
    assert!(resumed["resumed"]["claim"].is_object());
    let current = ok(server
        .call(
            WRITER,
            "awr_session_get",
            json!({"project":"alpha","conversation":"guide-discussion"}),
        )
        .await);
    assert_eq!(
        current["session"]["id"],
        resumed["resumed"]["session"]["id"]
    );
    assert_eq!(current["inherited_checkpoint"]["id"], cp);
    // Ending one session releases only its claims; another conversation stays active.
    ok(server.call(WRITER,"awr_session_end",json!({"project":"alpha","conversation":"guide-discussion","expected_revision":a.revision(),"outcome":"ended"})).await);
    let other = ok(server
        .call(
            WRITER,
            "awr_session_get",
            json!({"project":"alpha","conversation":"follow-up"}),
        )
        .await);
    assert_eq!(other["session"]["status"], "active");
    assert_eq!(other["claims"][0]["status"], "active");
    // Runtime inspection/cleanup survive unavailable source files.
    fs::remove_file(a.root.join("work.yaml")).unwrap();
    ok(server
        .call(
            WRITER,
            "awr_session_get",
            json!({"project":"alpha","conversation":"follow-up"}),
        )
        .await);
    ok(server.call(WRITER,"awr_session_end",json!({"project":"alpha","conversation":"follow-up","expected_revision":a.revision(),"outcome":"interrupted"})).await);
    server.stop().await;
}

#[tokio::test]
async fn one_endpoint_routes_independent_clients_projects_and_conflicting_writes() {
    let a = ProjectFixture::new("Write the alpha guide");
    let b = ProjectFixture::new("Write the beta guide");
    let mut server = Server::start(&registry(&a, &b)).await;
    let catalog = ok(server.call(READER, "awr_projects_list", json!({})).await);
    assert_eq!(catalog["projects"].as_array().unwrap().len(), 1);
    assert_eq!(catalog["projects"][0]["key"], "alpha");
    assert!(!catalog.to_string().contains(a.root.to_str().unwrap()));
    let (alpha, beta) = tokio::join!(
        server.call(
            READER,
            "awr_work_get",
            json!({"project":"alpha","work":"W"})
        ),
        server.call(WRITER, "awr_work_get", json!({"project":"beta","work":"W"}))
    );
    assert_eq!(ok(alpha)["work"]["title"], "Write the alpha guide");
    assert_eq!(ok(beta)["work"]["title"], "Write the beta guide");
    error(
        server
            .call(READER, "awr_work_get", json!({"project":"beta","work":"W"}))
            .await,
        "RuleViolation",
    );
    error(
        server
            .call(WRITER, "awr_work_get", json!({"project":a.root,"work":"W"}))
            .await,
        "RuleViolation",
    );
    error(
        server
            .call(WRITER, "awr_work_get", json!({"work":"W"}))
            .await,
        "InvalidInput",
    );
    let before = a.revision();
    let event = json!({"project":"alpha","expected_revision":before,"event_type":"work.observed","summary":"Reviewed the guide outline"});
    error(
        server.call(READER, "awr_event_append", event.clone()).await,
        "RuleViolation",
    );
    assert_eq!(a.revision(), before);
    let beta_before = b.revision();
    let (one,two,other_project) = tokio::join!(
        server.call(WRITER,"awr_event_append",event.clone()),
        server.call(WRITER,"awr_event_append",event),
        server.call(WRITER,"awr_event_append",json!({"project":"beta","expected_revision":beta_before,"event_type":"work.observed","summary":"Reviewed beta input"})));
    let results = [one, two];
    assert_eq!(results.iter().filter(|r| r["isError"] != true).count(), 1);
    error(
        results
            .iter()
            .find(|r| r["isError"] == true)
            .unwrap()
            .clone(),
        "RevisionConflict",
    );
    ok(other_project);
    assert_eq!(a.revision(), before + 3);
    assert_eq!(b.revision(), beta_before + 3);
    server.stop().await;
    // A fresh service keeps the same project identities and committed revisions.
    let mut restarted = Server::start(&registry(&a, &b)).await;
    let current = ok(restarted
        .call(
            WRITER,
            "awr_work_get",
            json!({"project":"alpha","work":"W"}),
        )
        .await);
    assert_eq!(current["project_revision"], before + 3);
    restarted.stop().await;
}

#[tokio::test]
async fn every_http_request_requires_credentials_and_allowed_browser_origin() {
    let a = ProjectFixture::new("First guide");
    let b = ProjectFixture::new("Second guide");
    let mut server = Server::start(&registry(&a, &b)).await;
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .post(&server.url)
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(&server.url)
            .bearer_auth("incorrect")
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(&server.url)
            .bearer_auth(WRITER)
            .header("Origin", "https://unregistered.example")
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(&server.url)
            .bearer_auth(WRITER)
            .header("Host", "unregistered.example")
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    server.stop().await;
}

#[tokio::test]
async fn wait_reply_and_successor_keep_progress_without_automatic_execution() {
    let a = ProjectFixture::new("Prepare the onboarding guide");
    let b = ProjectFixture::new("Prepare the reference guide");
    let config = registry(&a, &b);
    let mut server = Server::start(&config).await;
    let started = ok(server
        .call(
            WRITER,
            "awr_session_start",
            start_args("alpha", "W", "onboarding", a.revision(), true),
        )
        .await);
    let context = ok(server
        .call(
            WRITER,
            "awr_context_compile",
            json!({"project":"alpha","conversation":"onboarding"}),
        )
        .await);
    let wait_args = json!({"project":"alpha","conversation":"onboarding","expected_revision":a.revision(),"request_id":"ask-example-order","question":"Should the practical example come before the reference material?","context_hash":context["work_context"]["context_hash"],"digest":"The introduction is drafted and the example order needs a decision.","next_action":"Arrange the examples after the reply","open_loops":["Confirm the example order"]});
    let waiting = ok(server
        .call(WRITER, "awr_session_wait", wait_args.clone())
        .await);
    assert_eq!(waiting["wait"]["status"], "waiting_user");
    assert_eq!(
        waiting["wait"]["checkpoint_id"],
        waiting["checkpoint"]["id"]
    );
    let before = a.revision();
    ok(server.call(WRITER, "awr_session_wait", wait_args).await);
    assert_eq!(a.revision(), before);
    error(server.call(WRITER,"awr_work_transition",json!({"project":"alpha","conversation":"onboarding","work":"W","action":"progress","reason":"Proceed with the guide","expected_revision":a.revision(),"next_action":"Edit examples"})).await,"InvalidTransition");
    server.stop().await;
    let mut server = Server::start(&config).await;
    let state = ok(server
        .call(
            WRITER,
            "awr_session_get",
            json!({"project":"alpha","conversation":"onboarding"}),
        )
        .await);
    assert_eq!(state["continuity_state"], "waiting_user");
    assert_eq!(state["checkpoint"]["id"], waiting["checkpoint"]["id"]);
    error(server.call(COLLEAGUE,"awr_session_reply",json!({"project":"alpha","wait":waiting["wait"]["id"],"reply":"A reply from a different client","expected_revision":a.revision()})).await,"RuleViolation");
    let reply = json!({"project":"alpha","wait":waiting["wait"]["id"],"reply":"Put the practical example first, followed by the reference material.","expected_revision":a.revision(),"request_id":"example-order-reply"});
    let answered = ok(server
        .call(WRITER, "awr_session_reply", reply.clone())
        .await);
    assert_eq!(answered["wait"]["status"], "answered");
    let before = a.revision();
    ok(server.call(WRITER, "awr_session_reply", reply).await);
    assert_eq!(a.revision(), before);
    let context = ok(server
        .call(
            WRITER,
            "awr_context_compile",
            json!({"project":"alpha","conversation":"onboarding"}),
        )
        .await);
    assert_eq!(
        context["continuity"]["waits"][0]["reply"],
        answered["wait"]["reply"]
    );
    let resumed=ok(server.call(WRITER,"awr_session_resume",json!({"project":"alpha","session":started["session"]["id"],"conversation":"onboarding-next-day","agent":"guide-editor","provider":"synthetic","model":"fixture","expected_revision":a.revision()})).await);
    assert_eq!(
        resumed["predecessor_waits"][0]["reply"],
        answered["wait"]["reply"]
    );
    let context = ok(server
        .call(
            WRITER,
            "awr_context_compile",
            json!({"project":"alpha","conversation":"onboarding-next-day"}),
        )
        .await);
    assert_eq!(
        context["continuity"]["waits"][0]["reply"],
        answered["wait"]["reply"]
    );
    assert_eq!(
        fs::read_to_string(a.root.join("work.yaml"))
            .unwrap()
            .matches("status: ready")
            .count(),
        2
    );
    server.stop().await;
}

#[tokio::test]
async fn missing_response_receipt_is_recovered_from_correlated_events_without_replaying() {
    let a = ProjectFixture::new("Prepare the guide");
    let b = ProjectFixture::new("Prepare the reference");
    let config = registry(&a, &b);
    let mut server = Server::start(&config).await;
    let conn = rusqlite::Connection::open(a.root.join(".awr/state.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER fixture_deny_response BEFORE INSERT ON events WHEN new.event_type='mcp.operation_finished' BEGIN SELECT RAISE(ABORT,'synthetic response receipt failure'); END;").unwrap();
    let mut args = start_args("alpha", "W", "lost-response", a.revision(), true);
    args["request_id"] = json!("lost-start-result");
    let result = server.call(WRITER, "awr_session_start", args.clone()).await;
    assert_eq!(result["isError"], true);
    assert_eq!(result["structuredContent"]["write_outcome"], "unknown");
    let receipt = ok(server
        .call(
            WRITER,
            "awr_operation_get",
            json!({"project":"alpha","request_id":"lost-start-result"}),
        )
        .await);
    assert_eq!(receipt["outcome"], "unknown");
    assert_eq!(receipt["domain_receipts"].as_array().unwrap().len(), 1);
    assert_eq!(
        receipt["domain_receipts"][0]["event_type"],
        "session.started"
    );
    let before = a.revision();
    server.call(WRITER, "awr_session_start", args.clone()).await;
    assert_eq!(a.revision(), before);
    let mut conflict = args.clone();
    conflict["model"] = json!("different-fixture");
    error(
        server.call(WRITER, "awr_session_start", conflict).await,
        "SourceConflict",
    );
    error(
        server
            .call(
                COLLEAGUE,
                "awr_operation_get",
                json!({"project":"alpha","request_id":"lost-start-result"}),
            )
            .await,
        "NotFound",
    );
    conn.execute_batch("DROP TRIGGER fixture_deny_response;")
        .unwrap();
    drop(conn);
    server.stop().await;
    let mut server = Server::start(&config).await;
    let recover = json!({"project":"alpha","request_id":"lost-start-result","expected_revision":a.revision()});
    let recovered = ok(server
        .call(WRITER, "awr_operation_recover", recover.clone())
        .await);
    assert_eq!(recovered["write_outcome"], "committed");
    assert_eq!(recovered["recovered"], true);
    let before = a.revision();
    ok(server.call(WRITER, "awr_operation_recover", recover).await);
    assert_eq!(a.revision(), before);
    ok(server.call(WRITER, "awr_session_start", args).await);
    assert_eq!(a.revision(), before);
    let sessions = ok(server
        .call(WRITER, "awr_session_list", json!({"project":"alpha"}))
        .await);
    assert_eq!(sessions["sessions"].as_array().unwrap().len(), 1);
    // Generic event callers cannot forge the runtime correlation metadata.
    error(server.call(WRITER,"awr_event_append",json!({"project":"alpha","expected_revision":a.revision(),"event_type":"work.observed","summary":"Observation with a forged correlation","payload":{"mcp_operation_id":receipt["operation"]["id"]}})).await,"InvalidInput");
    // A start marker alone remains unknown; it never authorizes replay or false success.
    let mut store = Store::open_existing(&a.root.join(".awr/state.db")).unwrap();
    store
        .begin_mcp_operation(
            a.id,
            a.revision(),
            "writer",
            "no-terminal-receipt",
            "awr_session_start",
            &"a".repeat(64),
        )
        .unwrap();
    drop(store);
    let before = a.revision();
    let unresolved=server.call(WRITER,"awr_operation_recover",json!({"project":"alpha","request_id":"no-terminal-receipt","expected_revision":before})).await;
    assert_eq!(unresolved["isError"], true);
    assert_eq!(unresolved["structuredContent"]["write_outcome"], "unknown");
    assert_eq!(a.revision(), before);
    server.stop().await;
}

#[tokio::test]
async fn authorized_source_refresh_is_available_at_the_shared_endpoint() {
    let a = ProjectFixture::new("Draft the first guide");
    let b = ProjectFixture::new("Draft the second guide");
    let mut server = Server::start(&registry(&a, &b)).await;
    let path = a.root.join("work.yaml");
    let source = fs::read_to_string(&path)
        .unwrap()
        .replace("Draft the first guide", "Review the updated guide");
    fs::write(&path, &source).unwrap();
    error(
        server
            .call(
                READER,
                "awr_source_reindex",
                json!({"project":"alpha","expected_revision":a.revision()}),
            )
            .await,
        "RuleViolation",
    );
    let refreshed = ok(server
        .call(
            WRITER,
            "awr_source_reindex",
            json!({"project":"alpha","expected_revision":a.revision()}),
        )
        .await);
    assert_eq!(refreshed["source_write_performed"], false);
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    let work = ok(server
        .call(
            READER,
            "awr_work_get",
            json!({"project":"alpha","work":"W"}),
        )
        .await);
    assert_eq!(work["work"]["title"], "Review the updated guide");
    server.stop().await;
}
