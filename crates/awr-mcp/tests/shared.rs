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
        fs::write(root.join("work.yaml"), format!("work_items:\n- id: W\n  title: {title}\n  status: ready\n  next_action: Draft the guide\n  acceptance: [Deliver the reviewed guide]\n- id: NEXT\n  title: Prepare follow-up\n  status: ready\n  next_action: Draft the follow-up\n  acceptance: [Deliver the follow-up]\n")).unwrap();
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
    async fn call(&self, credential: &str, name: &str, arguments: Value) -> Value {
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
    fs::write(&path, format!("version=1\n[[projects]]\nkey='alpha'\nroot={}\nproject_id='{}'\n[[projects]]\nkey='beta'\nroot={}\nproject_id='{}'\n[[clients]]\nid='writer'\ntoken_env='AWR_FIXTURE_WRITER'\nwrite=['alpha','beta']\n[[clients]]\nid='reader'\ntoken_env='AWR_FIXTURE_READER'\nread=['alpha']\n", json!(a.root), a.id, json!(b.root), b.id)).unwrap();
    path
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
    assert_eq!(a.revision(), before + 1);
    assert_eq!(b.revision(), beta_before + 1);
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
    assert_eq!(current["project_revision"], before + 1);
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
