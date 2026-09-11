use awr_core::*;
use awr_source::{Manifest, index_project};
use awr_store::Store;
use rmcp::{
    RoleClient, ServiceExt, model::*, service::RunningService, transport::TokioChildProcess,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, time::Duration};
use tokio::{process::Command, time::timeout};

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CRITERION: &str = "Deliver the reviewed analysis";
const WORK: &str = "work_items:\n- id: W\n  title: Prepare customer analysis\n  status: ready\n  owner: business-coordinator\n  next_action: Draft the analysis\n  depends_on: [D]\n  acceptance: [Deliver the reviewed analysis]\n  verification:\n    evidence_level: none\n  evidence: []\n- id: D\n  title: Required input\n  status: completed\n- id: NEXT\n  title: Prepare follow-up\n  status: ready\n  next_action: Review next steps\n  acceptance: [Follow-up is available]\n";
const MANIFEST: &str = "[project]\nname='MCP fixture'\nexternal_key='mcp-fixture'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='goal.md'\nadapter='markdown-heading-v1'\n[sources.options]\nstatus='active'\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='decisions'\nadapter='markdown-directory-v1'\n";

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("awr-mcp-{}", Id::new()));
        fs::create_dir_all(path.join(".awr")).unwrap();
        let root = path.canonicalize().unwrap();
        fs::create_dir(root.join("decisions")).unwrap();
        fs::write(root.join("work.yaml"), WORK).unwrap();
        fs::write(root.join("rules.md"), "# Authority {#authority severity=hard scope=project value=*}\n\nPreserve exact acceptance and source facts.\n").unwrap();
        fs::write(
            root.join("goal.md"),
            "# Deliver useful analysis\n\nPersist work and deliver reviewed customer results.\n",
        )
        .unwrap();
        fs::write(root.join(".awr/project.toml"), MANIFEST).unwrap();
        let f = Self { root };
        let mut store = Store::open(&f.db()).unwrap();
        assert!(
            index_project(
                &mut store,
                &f.root,
                &Manifest::load(&f.root).unwrap(),
                false
            )
            .unwrap()
            .ok
        );
        f
    }
    fn db(&self) -> PathBuf {
        self.root.join(".awr/state.db")
    }
    fn store(&self) -> (Store, Project) {
        let store = Store::open_existing(&self.db()).unwrap();
        let project = store.project_by_root(&self.root).unwrap();
        (store, project)
    }
    fn rev(&self) -> Revision {
        self.store().1.project_revision
    }
    fn reindex(&self) {
        let (mut store, _) = self.store();
        assert!(
            index_project(
                &mut store,
                &self.root,
                &Manifest::load(&self.root).unwrap(),
                false
            )
            .unwrap()
            .ok
        );
    }
    fn session(&self, key: &str, claim: bool, branch: Option<Id>) -> SessionStarted {
        let (mut store, p) = self.store();
        store
            .start_session(
                p.id,
                p.project_revision,
                SessionDraft {
                    work_item_key: Some(key.into()),
                    agent_id: format!("executor-{}", Id::new()),
                    provider: "fixture".into(),
                    model: "test".into(),
                    branch_id: branch,
                    claim,
                    claim_ttl_ms: Some(600_000),
                },
            )
            .unwrap()
            .0
    }
    fn logical_state(&self) -> BTreeMap<String, Vec<String>> {
        let conn = rusqlite::Connection::open_with_flags(
            self.db(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let tables = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        tables
            .into_iter()
            .map(|table| {
                let mut stmt = conn
                    .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
                    .unwrap();
                let columns = stmt.column_count();
                let mut rows = stmt
                    .query_map([], |r| {
                        (0..columns)
                            .map(|i| r.get_ref(i).map(|v| format!("{v:?}")))
                            .collect::<std::result::Result<Vec<_>, _>>()
                            .map(|v| v.join("|"))
                    })
                    .unwrap()
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap();
                rows.sort();
                (table, rows)
            })
            .collect()
    }
    async fn client(&self) -> RunningService<RoleClient, ()> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_awr-mcp"));
        command.arg("--project").arg(&self.root).kill_on_drop(true);
        timeout(
            Duration::from_secs(20),
            ().serve(TokioChildProcess::new(command).unwrap()),
        )
        .await
        .unwrap()
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn call(client: &RunningService<RoleClient, ()>, name: &str, args: Value) -> CallToolResult {
    timeout(
        Duration::from_secs(30),
        client.call_tool(
            CallToolRequestParams::new(name.to_owned())
                .with_arguments(args.as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}
fn body(result: &CallToolResult) -> Value {
    let value = result
        .structured_content
        .clone()
        .expect("structured result");
    assert_eq!(
        serde_json::from_str::<Value>(&result.content[0].as_text().unwrap().text).unwrap(),
        value
    );
    value
}
fn success(result: CallToolResult) -> Value {
    assert_eq!(result.is_error, Some(false), "{result:?}");
    body(&result)
}
fn error(result: CallToolResult, expected: &str) -> Value {
    assert_eq!(result.is_error, Some(true), "{result:?}");
    let value = body(&result);
    let code = value
        .get("code")
        .or_else(|| value.get("error").and_then(|e| e.get("code")))
        .unwrap();
    assert_eq!(code, expected, "{value}");
    value
}
fn action(f: &Fixture, session: Id, action: &str) -> Value {
    json!({"work":"W","session":session,"action":action,"expected_revision":f.rev(),"reason":"Apply the reviewed work change"})
}

#[tokio::test]
async fn project_organization_guides_repairs_and_preserves_readonly_mcp_state() {
    let f = Fixture::new();
    fs::write(
        f.root.join(".awr/project.toml"),
        MANIFEST.replace("[project]\n", "[project]\ncontext_profile='minimal'\n"),
    )
    .unwrap();
    f.reindex();
    let client = f.client().await;
    let before = f.logical_state();
    let initial = success(call(&client, "awr_project_status", json!({})).await);
    assert_eq!(initial["organization"]["state"], "needs_organization");
    assert!(
        initial["organization"]["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["code"] == "work_goal_missing")
    );
    assert_eq!(initial["organization"]["business_execution_ready"], false);
    assert_eq!(f.logical_state(), before);

    let goal = initial["organization"]["goals"][0]["key"].as_str().unwrap();
    fs::write(f.root.join("work.yaml"), format!("work_items:\n- id: W\n  title: Deliver customer analysis\n  status: ready\n  goal: {goal}\n  acceptance: [Deliver the reviewed analysis]\n  next_action: Draft the requested analysis\n")).unwrap();
    let unchanged = f.logical_state();
    let stale = error(
        call(&client, "awr_project_status", json!({})).await,
        "SourceStale",
    );
    assert_eq!(stale["organization"]["state"], "source_unreadable");
    assert_eq!(stale["organization"]["business_execution_ready"], false);
    assert_eq!(f.logical_state(), unchanged);
    f.reindex();
    let before = f.logical_state();
    let ready = success(call(&client, "awr_project_status", json!({})).await);
    assert_eq!(ready["organization"]["state"], "ready");
    assert_eq!(ready["organization"]["executable_work"], json!(["W"]));
    assert_eq!(ready["suggested_work"]["external_key"], "W");
    let work_ready = success(call(&client, "awr_work_ready", json!({})).await);
    assert_eq!(work_ready["organization"]["state"], "ready");
    assert_eq!(f.logical_state(), before);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn stdio_discovers_tools_and_survives_protocol_and_argument_errors() {
    let f = Fixture::new();
    let before = f.logical_state();
    let client = f.client().await;
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), awr_mcp::TOOL_NAMES.len());
    let names = tools
        .iter()
        .map(|t| t.name.as_ref())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names, awr_mcp::TOOL_NAMES.into_iter().collect());
    assert!(
        serde_json::to_vec(&tools).unwrap().len() < 32_768,
        "catalog must stay compact"
    );
    for tool in &tools {
        let read = ![
            "awr_work_transition",
            "awr_event_append",
            "awr_evidence_record",
            "awr_session_start",
            "awr_session_checkpoint",
            "awr_session_end",
            "awr_session_resume",
            "awr_session_claim",
            "awr_session_wait",
            "awr_session_reply",
            "awr_operation_recover",
            "awr_source_reindex",
        ]
        .contains(&tool.name.as_ref());
        assert_eq!(
            tool.annotations.as_ref().unwrap().read_only_hint,
            Some(read)
        );
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }
    assert!(
        client
            .call_tool(CallToolRequestParams::new("awr_unlisted"))
            .await
            .is_err()
    );
    error(
        call(&client, "awr_work_ready", json!({"limit":0})).await,
        "InvalidInput",
    );
    error(
        call(
            &client,
            "awr_project_status",
            json!({"project":"elsewhere"}),
        )
        .await,
        "InvalidInput",
    );
    error(
        call(&client, "awr_work_get", json!({"work":"MISSING"})).await,
        "NotFound",
    );
    success(call(&client, "awr_project_status", json!({})).await);
    client.cancel().await.unwrap();
    assert_eq!(f.logical_state(), before);
}

#[tokio::test]
async fn all_five_reads_preserve_sources_and_every_runtime_and_search_table() {
    let f = Fixture::new();
    let session = f.session("W", true, None);
    let before = f.logical_state();
    let file = fs::read(f.root.join("work.yaml")).unwrap();
    let client = f.client().await;
    let status = success(call(&client, "awr_project_status", json!({})).await);
    assert_eq!(status["total"], 3);
    let ready = success(call(&client, "awr_work_ready", json!({})).await);
    assert_eq!(ready["ready_total"], 1);
    let work = success(
        call(
            &client,
            "awr_work_get",
            json!({"work":"W","source_sha":SHA}),
        )
        .await,
    );
    assert_eq!(work["acceptance"], json!([CRITERION]));
    assert_eq!(
        work["work"]["active_claims"][0]["session_id"],
        json!(session.session.id)
    );
    let context = success(
        call(
            &client,
            "awr_context_compile",
            json!({"work":"W","session":session.session.id,"source_sha":SHA,"budget":5000}),
        )
        .await,
    );
    assert_eq!(context["completeness"]["complete"], true);
    assert!(
        context["work_context"]["rendered_context"]
            .as_str()
            .unwrap()
            .contains("Preserve exact acceptance and source facts.")
    );
    assert!(context["work_context"]["token_estimate"].as_u64().unwrap() <= 5000);
    let again = success(
        call(
            &client,
            "awr_context_compile",
            json!({"work":"W","session":session.session.id,"source_sha":SHA,"budget":5000}),
        )
        .await,
    );
    assert_eq!(
        again["work_context"]["context_hash"],
        context["work_context"]["context_hash"]
    );
    let found = success(
        call(
            &client,
            "awr_search",
            json!({"text":"customer","kind":"work"}),
        )
        .await,
    );
    assert_eq!(found["hits"][0]["external_key"], "W");
    for value in [status, ready, work, context, found] {
        assert_eq!(value["project_revision"], f.rev());
        assert_eq!(value["read_only"], true);
        assert_eq!(value["source_refresh_performed"], false);
    }
    client.cancel().await.unwrap();
    assert_eq!(fs::read(f.root.join("work.yaml")).unwrap(), file);
    assert_eq!(f.logical_state(), before);
}

#[tokio::test]
async fn stale_files_new_directory_sources_and_changed_mappings_require_explicit_reindex() {
    let f = Fixture::new();
    let client = f.client().await;
    let before = f.logical_state();
    fs::write(
        f.root.join("work.yaml"),
        WORK.replace("Draft the analysis", "Review revised facts"),
    )
    .unwrap();
    for (name, args) in [
        ("awr_project_status", json!({})),
        ("awr_work_ready", json!({})),
        ("awr_work_get", json!({"work":"W"})),
        ("awr_context_compile", json!({"work":"W"})),
        ("awr_search", json!({"text":"analysis"})),
    ] {
        error(call(&client, name, args).await, "SourceStale");
        assert_eq!(f.logical_state(), before);
    }
    f.reindex();
    let work = success(call(&client, "awr_work_get", json!({"work":"W"})).await);
    assert_eq!(work["work"]["next_action"], "Review revised facts");
    let before = f.logical_state();
    fs::write(
        f.root.join("decisions/new.md"),
        "# A newly discovered decision\n\n## Decision\n\nKeep source authority.\n",
    )
    .unwrap();
    error(
        call(&client, "awr_project_status", json!({})).await,
        "SourceStale",
    );
    assert_eq!(f.logical_state(), before);
    f.reindex();
    let before = f.logical_state();
    fs::write(
        f.root.join(".awr/project.toml"),
        MANIFEST.replace("status='active'", "status='planned'"),
    )
    .unwrap();
    error(
        call(&client, "awr_context_compile", json!({"work":"W"})).await,
        "SourceStale",
    );
    assert_eq!(f.logical_state(), before);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn incomplete_context_keeps_diagnostics_and_budget_errors_are_tool_errors() {
    let f = Fixture::new();
    let client = f.client().await;
    let before = f.logical_state();
    let missing = error(
        call(&client, "awr_context_compile", json!({"work":"MISSING"})).await,
        "ContextIncomplete",
    );
    assert_eq!(missing["completeness"]["complete"], false);
    assert!(missing["work_context"].is_null());
    assert!(
        missing["diagnostic_text"]
            .as_str()
            .unwrap()
            .contains("CONTEXT INCOMPLETE")
    );
    error(
        call(
            &client,
            "awr_context_compile",
            json!({"work":"W","budget":1}),
        )
        .await,
        "BudgetExceeded",
    );
    error(
        call(
            &client,
            "awr_context_compile",
            json!({"work":"W","checkpoint":Id::new(),"after_revision":0}),
        )
        .await,
        "InvalidInput",
    );
    assert_eq!(f.logical_state(), before);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn source_actions_use_owned_claims_revision_guards_and_recovery_receipts() {
    let f = Fixture::new();
    let started = f.session("W", true, None);
    let sid = started.session.id;
    let client = f.client().await;
    let before = f.logical_state();
    let mut stale = action(&f, sid, "progress");
    stale["expected_revision"] = json!(f.rev() - 1);
    stale["next_action"] = json!("Review the draft");
    error(
        call(&client, "awr_work_transition", stale).await,
        "RevisionConflict",
    );
    let mut wrong = action(&f, Id::new(), "progress");
    wrong["next_action"] = json!("Review the draft");
    error(
        call(&client, "awr_work_transition", wrong).await,
        "NotFound",
    );
    assert_eq!(f.logical_state(), before);
    for (kind, field, text, status) in [
        ("progress", "next_action", "Review the draft", "in_progress"),
        ("block", "blocker", "Await customer input", "blocked"),
        (
            "unblock",
            "next_action",
            "Review restored input",
            "in_progress",
        ),
        ("cancel", "next_action", "Record cancellation", "cancelled"),
        ("reopen", "next_action", "Replan analysis", "planned"),
    ] {
        let mut args = action(&f, sid, kind);
        args[field] = json!(text);
        let result = success(call(&client, "awr_work_transition", args).await);
        assert_eq!(result["proposal"]["status"], "applied");
        assert!(
            f.root
                .join(result["recovery_directory"].as_str().unwrap())
                .join("before.yaml")
                .is_file()
        );
        let work = success(call(&client, "awr_work_get", json!({"work":"W"})).await);
        assert_eq!(work["work"]["status"], status);
        assert_eq!(work["work"]["owner"], "business-coordinator");
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn completion_requires_actual_bound_evidence_and_releases_only_the_owned_claim() {
    let f = Fixture::new();
    let started = f.session("W", true, None);
    let sid = started.session.id;
    let other = f.session("NEXT", true, None);
    let client = f.client().await;
    let mut progress = action(&f, sid, "progress");
    progress["next_action"] = json!("Review completed analysis");
    success(call(&client, "awr_work_transition", progress).await);
    let before = f.logical_state();
    error(
        call(&client, "awr_work_transition", action(&f, sid, "complete")).await,
        "EvidenceMissing",
    );
    let mut args = action(&f, sid, "complete");
    args["completion"] = json!({"version":1,"source_sha":SHA,"acceptance":[{"criterion":CRITERION,"evidence":["E"]}]});
    error(
        call(&client, "awr_work_transition", args.clone()).await,
        "NotFound",
    );
    assert_eq!(f.logical_state(), before);
    let report = json!({"version":1,"work_item":"W","source_sha":SHA,"command":"review customer analysis","scope":["W"],"verified_at":now_millis().unwrap(),"checks":[{"name":"review and delivery","passed":true,"details":"Reviewed the fixture analysis and receipt","criteria":[CRITERION]}]});
    let bytes = serde_json::to_vec(&report).unwrap();
    fs::write(f.root.join("report.json"), &bytes).unwrap();
    let digest = awr_source::fingerprint(&bytes)
        .trim_start_matches("sha256:")
        .to_owned();
    let recorded=success(call(&client,"awr_evidence_record",json!({"expected_revision":f.rev(),"external_key":"E","work":"W","evidence_type":"completion_report","level":"locally_verified","summary":"Reviewed the fixture analysis","locator":"report.json","sha256":digest,"source_sha":SHA,"command":"review customer analysis","scope":["W"],"verified_at":report["verified_at"]})).await);
    assert_eq!(recorded["validation_basis"], "caller_supplied_bindings");
    let before = f.logical_state();
    fs::write(f.root.join("report.json"), "changed report").unwrap();
    args["expected_revision"] = json!(f.rev());
    let failed = call(&client, "awr_work_transition", args.clone()).await;
    assert_eq!(failed.is_error, Some(true));
    assert_eq!(f.logical_state(), before);
    fs::write(f.root.join("report.json"), &bytes).unwrap();
    args["expected_revision"] = json!(f.rev());
    let result = success(call(&client, "awr_work_transition", args).await);
    assert_eq!(result["event"]["event_type"], "work.completed");
    assert_eq!(
        result["event"]["payload"]["released_claim_ids"],
        json!([started.claim.unwrap().id])
    );
    let (store, p) = f.store();
    assert_eq!(
        store.work_item(p.id, "W").unwrap().item.status,
        WorkStatus::Completed
    );
    assert_eq!(
        store.session(p.id, other.session.id).unwrap().status,
        "active"
    );
    assert_eq!(
        store
            .work_readiness(p.id, "NEXT", None, now_millis().unwrap())
            .unwrap()
            .active_claims[0]
            .id,
        other.claim.unwrap().id
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn generic_events_are_bounded_reserved_and_branch_bound_and_commands_are_inert() {
    let f = Fixture::new();
    let (mut store, p) = f.store();
    let created = awr_runtime::create_branch(
        &mut store,
        &f.root,
        &awr_runtime::CreateBranchRequest {
            name: "review".into(),
            parent: Some("main".into()),
            git_ref: None,
            expected_revision: p.project_revision,
            actor: "fixture".into(),
            reason: "Review isolated work".into(),
        },
    )
    .unwrap();
    drop(store);
    let bid = created.0.id;
    let client = f.client().await;
    let before = f.logical_state();
    error(call(&client,"awr_event_append",json!({"expected_revision":f.rev(),"event_type":"work.completed","summary":"Bypass completion"})).await,"InvalidInput");
    assert_eq!(f.logical_state(), before);
    let recorded=success(call(&client,"awr_event_append",json!({"expected_revision":f.rev(),"work":"W","branch":"review","event_type":"work.observed","importance":"critical","summary":"Review found a useful next step","payload":{"body":"UNINDEXED_PAYLOAD_SENTINEL"}})).await);
    assert_eq!(recorded["event"]["branch_id"], json!(bid));
    let before = f.logical_state();
    let context = success(
        call(
            &client,
            "awr_context_compile",
            json!({"work":"W","branch":"review","detached":true}),
        )
        .await,
    );
    assert!(
        context["work_context"]["rendered_context"]
            .as_str()
            .unwrap()
            .contains("Review found a useful next step")
    );
    let main = success(
        call(
            &client,
            "awr_context_compile",
            json!({"work":"W","branch":"main","detached":true}),
        )
        .await,
    );
    assert!(
        !serde_json::to_string(&main)
            .unwrap()
            .contains("Review found a useful next step")
    );
    let search = success(
        call(
            &client,
            "awr_search",
            json!({"text":"UNINDEXED_PAYLOAD_SENTINEL"}),
        )
        .await,
    );
    assert!(search["hits"].as_array().unwrap().is_empty());
    assert_eq!(f.logical_state(), before);
    let output = f.root.join("must-not-execute");
    success(call(&client,"awr_evidence_record",json!({"expected_revision":f.rev(),"external_key":"observation","work":"W","branch":"review","evidence_type":"note","level":"implemented","summary":"Record an observation","locator":"unread-report.json","scope":["W"],"command":format!("touch {}",output.display())})).await);
    assert!(!output.exists());
    let before = f.logical_state();
    error(call(&client,"awr_evidence_record",json!({"expected_revision":f.rev(),"external_key":"observation","work":"W","branch":"review","evidence_type":"note","level":"implemented","summary":"Duplicate","locator":"unread-report.json","scope":["W"]})).await,"SourceConflict");
    assert_eq!(f.logical_state(), before);
    client.cancel().await.unwrap();
}

#[test]
fn snapshot_limit_and_startup_do_not_initialize_or_modify_a_project() {
    let f = Fixture::new();
    let before = f.logical_state();
    let (store, _) = f.store();
    assert!(store.memory_snapshot(1).is_err());
    let snapshot = store.memory_snapshot(256 * 1024 * 1024).unwrap();
    assert_eq!(
        snapshot.project_by_root(&f.root).unwrap().project_revision,
        f.rev()
    );
    assert_eq!(f.logical_state(), before);
    let empty = f.root.join("empty");
    fs::create_dir(&empty).unwrap();
    assert!(awr_mcp::AwrServer::open(&empty).is_ok());
    assert!(!empty.join(".awr").exists());
}

#[tokio::test]
async fn uninitialized_mcp_project_returns_intake_guidance_without_creating_state() {
    let root = std::env::temp_dir().join(format!("awr-mcp-empty-{}", Id::new()));
    fs::create_dir(&root).unwrap();
    let f = Fixture {
        root: root.canonicalize().unwrap(),
    };
    let client = f.client().await;
    let status = error(
        call(&client, "awr_project_status", json!({})).await,
        "NotFound",
    );
    assert_eq!(status["organization"]["state"], "not_initialized");
    assert_eq!(status["organization"]["business_execution_ready"], false);
    assert!(status["total"].is_null());
    assert!(
        status["organization"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "sources")
    );
    assert!(!f.root.join(".awr").exists());
    error(call(&client,"awr_work_transition",json!({"action":"progress","work":"W","session":Id::new(),"expected_revision":0,"reason":"Record current progress","next_action":"Continue the task"})).await,"NotFound");
    assert!(!f.root.join(".awr").exists());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn mutation_source_refresh_preserves_human_edits_and_requires_a_reviewed_revision() {
    for name in [
        "awr_event_append",
        "awr_evidence_record",
        "awr_work_transition",
    ] {
        let f = Fixture::new();
        let started = f.session("W", true, None);
        let client = f.client().await;
        let revision = f.rev();
        let human = WORK.replace("Draft the analysis", "Preserve this human source edit");
        fs::write(f.root.join("work.yaml"), &human).unwrap();
        let args = match name {
            "awr_event_append" => {
                json!({"expected_revision":revision,"event_type":"work.observed","summary":"Candidate observation"})
            }
            "awr_evidence_record" => {
                json!({"expected_revision":revision,"external_key":"candidate","work":"W","evidence_type":"note","level":"implemented","summary":"Candidate note","locator":"report.json","scope":["W"]})
            }
            _ => {
                json!({"expected_revision":revision,"work":"W","session":started.session.id,"action":"progress","reason":"Advance reviewed work","next_action":"Review the draft"})
            }
        };
        if name == "awr_work_transition" {
            error(call(&client, name, args).await, "SourceConflict");
            assert_eq!(f.rev(), revision);
            error(
                call(&client, "awr_project_status", json!({})).await,
                "SourceStale",
            );
            f.reindex();
        } else {
            error(call(&client, name, args).await, "RevisionConflict");
        }
        assert_eq!(fs::read_to_string(f.root.join("work.yaml")).unwrap(), human);
        assert!(f.rev() > revision);
        let (store, p) = f.store();
        let work = store.work_item(p.id, "W").unwrap();
        assert_eq!(work.item.status, WorkStatus::Ready);
        assert_eq!(work.item.next_action, "Preserve this human source edit");
        assert!(store.evidence(p.id, "candidate").is_err());
        success(call(&client, "awr_project_status", json!({})).await);
        client.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn legacy_stdio_negotiation_keeps_protocol_stdout_and_clean_eof() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let f = Fixture::new();
    let before = f.logical_state();
    let mut child = Command::new(env!("CARGO_BIN_EXE_awr-mcp"))
        .arg("--project")
        .arg(&f.root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"legacy-fixture","version":"1"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"awr_project_status","arguments":{}}}),
    ];
    for request in requests {
        input
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        input.flush().await.unwrap();
        let mut line = String::new();
        timeout(Duration::from_secs(20), output.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], request["id"]);
        assert!(reply.get("error").is_none(), "{reply}");
        match request["id"].as_i64().unwrap() {
            1 => {
                assert_eq!(reply["result"]["protocolVersion"], "2025-11-25");
                input
                    .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
                    .await
                    .unwrap();
                input.flush().await.unwrap();
            }
            2 => assert_eq!(
                reply["result"]["tools"].as_array().unwrap().len(),
                awr_mcp::TOOL_NAMES.len()
            ),
            _ => assert_eq!(reply["result"]["structuredContent"]["read_only"], true),
        }
    }
    drop(input);
    assert!(
        timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert_eq!(f.logical_state(), before);
}
