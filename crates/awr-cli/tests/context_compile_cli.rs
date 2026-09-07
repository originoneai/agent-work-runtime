use awr_core::*;
use awr_store::Store;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};
const WORK: &str = "work_items:\n- id: W\n  title: Build resumable context\n  milestone: M4\n  status: in_progress\n  next_action: Continue the current task\n  acceptance: [Keep exact acceptance text]\n  depends_on: [DEP]\n- id: DEP\n  title: Required input\n  status: blocked\n  blocker: Waiting for the source\n  next_action: Read the missing input\n  acceptance: [Input is available]\n- id: OLD\n  title: UNRELATED_TASK_BODY_SENTINEL\n  status: completed\n  summary: UNRELATED_TASK_BODY_SENTINEL\n";
const RULE: &str = "# Authority {#authority severity=hard scope=project value=*}\n\nPreserve all hard source facts exactly.\n";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-context-cli-{}", Id::new()));
        fs::create_dir_all(root.join("decisions")).unwrap();
        fs::write(root.join("work.yaml"), WORK).unwrap();
        fs::write(root.join("rules.md"), RULE).unwrap();
        fs::write(
            root.join("goal.md"),
            "# Build a durable agent runtime\n\nPersist work and resume using current facts.\n",
        )
        .unwrap();
        fs::write(root.join("decisions/adr.md"),"---\naffected_keys: [W]\n---\n\n# Persistence decision\n\nStatus: accepted\n\n## Decision\n\nPersist runtime history in SQLite.\n\n## Rationale\n\nRAW_RATIONALE_SENTINEL\n").unwrap();
        fs::write(root.join("sources.toml"),"[project]\nname='L1 fixture'\nexternal_key='l1'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='goal.md'\nadapter='markdown-heading-v1'\n[sources.options]\nstatus='active'\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='decisions'\nadapter='markdown-directory-v1'\n").unwrap();
        let f = Self(root);
        f.ok(&["init", "--manifest", "sources.toml", "--accept"]);
        f
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .arg("--project")
            .arg(&self.0)
            .arg("--json")
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let o = self.run(args);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    }
    fn revision(&self) -> String {
        self.ok(&["status"])["project_revision"].to_string()
    }
    fn start(&self, agent: &str) -> Value {
        self.ok(&[
            "session",
            "start",
            "--work",
            "W",
            "--agent",
            agent,
            "--provider",
            "fixture",
            "--model",
            "test",
            "--expected-revision",
            &self.revision(),
        ])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn l1_assembles_exact_current_facts_with_checkpoint_delta_and_no_raw_bodies() {
    let f = Fixture::new();
    let start = f.start("executor");
    let sid = start["session"]["id"].as_str().unwrap();
    let initial = f.ok(&["context", "compile", "--work", "W", "--session", sid]);
    let hash = initial["work_context"]["context_hash"].as_str().unwrap();
    let checkpoint = f.ok(&[
        "session",
        "checkpoint",
        "--session",
        sid,
        "--context-hash",
        hash,
        "--digest",
        "Stored implementation progress",
        "--next-action",
        "Read updated source rules",
        "--open-loop",
        "Review the outstanding input",
        "--expected-revision",
        &f.revision(),
    ]);
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace(
            "Continue the current task",
            "Continue using the updated source",
        ),
    )
    .unwrap();
    let revision = f.revision();
    {
        let mut store = Store::open(&f.0.join(".awr/state.db")).unwrap();
        let project = store.project_by_root(&f.0).unwrap();
        let mut draft = EventDraft::new("work.observed", "Critical input change requires review");
        draft.importance = "critical".into();
        draft.session_id = Some(sid.parse().unwrap());
        draft.payload = serde_json::json!({"body":"RAW_EVENT_PAYLOAD_SENTINEL".repeat(5000)});
        store
            .append_event(project.id, revision.parse().unwrap(), draft)
            .unwrap();
    }
    let compiled = f.ok(&["context", "compile", "--work", "W", "--session", sid]);
    let pack = &compiled["work_context"];
    let text = pack["rendered_context"].as_str().unwrap();
    assert_eq!(compiled["level"], "L1");
    assert_eq!(compiled["completeness"]["complete"], true);
    assert_eq!(compiled["completeness"]["goal_context_complete"], true);
    assert_eq!(compiled["checkpoint_id"], checkpoint["checkpoint"]["id"]);
    assert!(pack["token_estimate"].as_u64().unwrap() <= 5000);
    for fact in [
        "Keep exact acceptance text",
        "Preserve all hard source facts exactly.",
        "Continue using the updated source",
        "Waiting for the source",
        "Read the missing input",
        "Persist runtime history in SQLite.",
        "Critical input change requires review",
        "Review the outstanding input",
        "Severity: hard",
        "no_evidence",
    ] {
        assert!(text.contains(fact), "missing {fact}");
    }
    for hidden in [
        "RAW_EVENT_PAYLOAD_SENTINEL",
        "RAW_RATIONALE_SENTINEL",
        "UNRELATED_TASK_BODY_SENTINEL",
    ] {
        assert!(!serde_json::to_string(&compiled).unwrap().contains(hidden));
    }
    assert_eq!(
        compiled["completeness"]["unresolved_required_dependencies"],
        serde_json::json!(["DEP"])
    );
    assert_ne!(
        pack["context_hash"],
        initial["work_context"]["context_hash"]
    );
    let again = f.ok(&["context", "compile", "--work", "W", "--session", sid]);
    assert_eq!(compiled, again);
    let plain = Command::new(env!("CARGO_BIN_EXE_awr"))
        .arg("--project")
        .arg(&f.0)
        .args(["context", "compile", "--work", "W", "--session", sid])
        .output()
        .unwrap();
    assert!(plain.status.success());
    assert_eq!(String::from_utf8(plain.stdout).unwrap(), text);
    let store = Store::open_readonly(&f.0.join(".awr/state.db")).unwrap();
    let project = store.project_by_root(&f.0).unwrap();
    assert_eq!(
        store.work_item(project.id, "W").unwrap().item.status,
        WorkStatus::InProgress
    );
}

#[test]
fn incomplete_or_absent_work_returns_diagnostics_and_hard_overflow_returns_no_pack() {
    let f = Fixture::new();
    f.ok(&["context", "compile", "--work", "W"]);
    fs::remove_file(f.0.join("rules.md")).unwrap();
    let failed = f.run(&["context", "compile", "--work", "W"]);
    assert!(!failed.status.success());
    let report: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(report["completeness"]["source_fresh"], false);
    assert_eq!(report["completeness"]["rules_complete"], false);
    assert!(
        report["work_context"]["rendered_context"]
            .as_str()
            .unwrap()
            .contains("CONTEXT INCOMPLETE")
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&failed.stderr).unwrap()["code"],
        "ContextIncomplete"
    );
    let missing = f.run(&["context", "compile", "--work", "ABSENT"]);
    assert!(!missing.status.success());
    let report: Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert!(report["work_context"].is_null());
    assert_eq!(report["completeness"]["work_item_found"], false);
    fs::write(
        f.0.join("rules.md"),
        format!(
            "{RULE}\n{}REQUIRED_RULE_TAIL\n",
            "Required obligation. ".repeat(10000)
        ),
    )
    .unwrap();
    let large = f.run(&["context", "compile", "--work", "W"]);
    assert!(!large.status.success() && large.stdout.is_empty());
    let error: Value = serde_json::from_slice(&large.stderr).unwrap();
    assert_eq!(error["code"], "BudgetExceeded");
    assert!(error["details"]["required"].as_u64().unwrap() > 5000);
    assert!(
        fs::read_to_string(f.0.join("rules.md"))
            .unwrap()
            .contains("REQUIRED_RULE_TAIL")
    );
}

#[test]
fn ambiguous_sessions_and_unknown_path_rules_require_explicit_scope() {
    let f = Fixture::new();
    let first = f.start("first");
    f.start("second");
    let ambiguous = f.run(&["context", "compile", "--work", "W"]);
    assert!(!ambiguous.status.success() && ambiguous.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&ambiguous.stderr).unwrap()["code"],
        "InvalidInput"
    );
    let sid = first["session"]["id"].as_str().unwrap();
    f.ok(&["context", "compile", "--session", sid]);
    fs::write(f.0.join("rules.md"),format!("{RULE}\n# Rust paths {{#rust severity=hard scope=path value=src/**}}\n\nKeep the Rust source consistent.\n")).unwrap();
    let unknown = f.run(&["context", "compile", "--session", sid]);
    assert!(!unknown.status.success());
    let p: Value = serde_json::from_slice(&unknown.stdout).unwrap();
    assert_eq!(p["completeness"]["rules_complete"], false);
    let concrete = f.ok(&[
        "context",
        "compile",
        "--session",
        sid,
        "--path",
        "src/main.rs",
    ]);
    assert_eq!(concrete["completeness"]["complete"], true);
    assert!(
        concrete["work_context"]["rendered_context"]
            .as_str()
            .unwrap()
            .contains("Keep the Rust source consistent.")
    );
    let different = f.ok(&[
        "context",
        "compile",
        "--session",
        sid,
        "--path",
        "src/lib.rs",
    ]);
    assert_ne!(
        concrete["work_context"]["context_hash"],
        different["work_context"]["context_hash"]
    );
}

#[test]
fn large_optional_goal_body_is_omitted_whole_and_explicit_missing_goal_is_incomplete() {
    let f = Fixture::new();
    fs::write(
        f.0.join("goal.md"),
        format!(
            "# Durable runtime {{status=active}}\n\n{}OPTIONAL_GOAL_BODY_TAIL\n",
            "Historical goal discussion. ".repeat(10000)
        ),
    )
    .unwrap();
    let report = f.ok(&["context", "compile", "--work", "W"]);
    let pack = &report["work_context"];
    assert!(pack["token_estimate"].as_u64().unwrap() <= 5000);
    assert!(
        !pack["rendered_context"]
            .as_str()
            .unwrap()
            .contains("OPTIONAL_GOAL_BODY_TAIL")
    );
    assert!(
        pack["omitted_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["key"].as_str().unwrap().starts_with("goal-body:"))
    );
    let missing = f.run(&["context", "compile", "--work", "W", "--goal", "ABSENT-GOAL"]);
    assert!(!missing.status.success());
    let report: Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(report["completeness"]["goal_context_complete"], false);
    assert!(
        report["completeness"]["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["code"] == "goal_not_found")
    );
}
