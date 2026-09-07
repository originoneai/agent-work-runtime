use awr_core::Id;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-bootstrap-cli-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("work.yaml"),"work_items:\n- id: W\n  title: Implement recovery\n  milestone: M1\n  status: in_progress\n  next_action: Read checkpoint and continue\n  acceptance: [L1_ACCEPTANCE_SENTINEL]\n- id: OLD\n  title: UNRELATED_HISTORY_SENTINEL\n  status: completed\n").unwrap();
        fs::write(root.join("rules.md"),"# Authority {#authority severity=hard scope=project value=*}\n\nPreserve authoritative source facts exactly.\n").unwrap();
        fs::write(root.join("sources.toml"),"[project]\nname='Bootstrap'\nexternal_key='bootstrap'\n\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n\n[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n[sources.options]\nkey_prefix='rules'\n").unwrap();
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
        let r = self.run(args);
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        serde_json::from_slice(&r.stdout).unwrap()
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
fn bootstrap_is_deterministic_and_refreshes_current_source_facts() {
    let f = Fixture::new();
    let first = f.ok(&["context", "bootstrap"]);
    let same = f.ok(&["context", "bootstrap"]);
    assert_eq!(first, same);
    assert!(first["token_estimate"].as_u64().unwrap() <= 1000);
    assert_eq!(first["context"]["work"]["external_key"], "W");
    assert_eq!(first["context"]["work"]["phase"], "M1");
    assert_eq!(
        first["context"]["critical_rules"][0]["text"],
        "Authority\n\nPreserve authoritative source facts exactly."
    );
    assert_eq!(first["context"]["complete"], true);
    assert_eq!(first["context"]["execution_context_complete"], false);
    assert!(
        !first["rendered_context"]
            .as_str()
            .unwrap()
            .contains("SENTINEL")
    );
    let file = f.0.join("work.yaml");
    let content = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        content
            .replace("in_progress", "blocked\n  blocker: Await dependency")
            .replace("Read checkpoint and continue", "Resolve dependency first"),
    )
    .unwrap();
    let changed = f.ok(&["context", "bootstrap", "--work", "W"]);
    assert_eq!(changed["context"]["work"]["raw_status"], "blocked");
    assert_eq!(
        changed["context"]["work"]["next_action"],
        "Resolve dependency first"
    );
    assert_eq!(changed["context"]["work"]["blocker"], "Await dependency");
    assert_ne!(first["context_hash"], changed["context_hash"]);
    assert!(
        changed["context"]["project_revision"].as_u64().unwrap()
            > first["context"]["project_revision"].as_u64().unwrap()
    );
    let oversized = format!(
        "# Hard {{severity=hard scope=project value=*}}\n\n{} KEEP_HARD_TAIL\n",
        "Required obligation. ".repeat(5000)
    );
    fs::write(f.0.join("rules.md"), oversized).unwrap();
    let fail = f.run(&["context", "bootstrap", "--work", "W"]);
    assert!(!fail.status.success());
    assert!(fail.stdout.is_empty());
    let error: Value = serde_json::from_slice(&fail.stderr).unwrap();
    assert_eq!(error["code"], "BudgetExceeded");
    assert!(error["details"]["required"].as_u64().unwrap() > 1000);
    assert!(
        fs::read_to_string(f.0.join("rules.md"))
            .unwrap()
            .contains("KEEP_HARD_TAIL")
    );
}

#[test]
fn bootstrap_recovers_checkpoint_after_session_end_and_rejects_ambiguity() {
    let f = Fixture::new();
    let first = f.start("first");
    let id = first["session"]["id"].as_str().unwrap();
    let cp = f.ok(&[
        "session",
        "checkpoint",
        "--session",
        id,
        "--context-hash",
        &"a".repeat(64),
        "--digest",
        "Implementation persisted",
        "--next-action",
        "Resume review",
        "--open-loop",
        "Unfinished artifact review",
        "--expected-revision",
        &f.revision(),
    ]);
    f.ok(&[
        "session",
        "end",
        "--session",
        id,
        "--outcome",
        "incomplete",
        "--expected-revision",
        &cp["event"]["project_revision"].to_string(),
    ]);
    let boot = f.ok(&["context", "bootstrap"]);
    assert_eq!(boot["context"]["checkpoint"]["id"], cp["checkpoint"]["id"]);
    assert_eq!(
        boot["context"]["checkpoint"]["open_loops"][0],
        "Unfinished artifact review"
    );
    assert!(boot["token_estimate"].as_u64().unwrap() <= 1000);
    let second = f.start("second");
    f.start("third");
    let ambiguous = f.run(&["context", "bootstrap"]);
    assert!(!ambiguous.status.success());
    assert!(ambiguous.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&ambiguous.stderr).unwrap()["code"],
        "InvalidInput"
    );
    let selected = f.ok(&[
        "context",
        "bootstrap",
        "--session",
        second["session"]["id"].as_str().unwrap(),
    ]);
    assert_eq!(selected["context"]["session"]["agent_id"], "second");
    assert_eq!(
        selected["context"]["checkpoint"]["id"],
        cp["checkpoint"]["id"]
    );
    assert_eq!(
        selected["context"]["checkpoint_origin"],
        "previous_closed_session"
    );
}

#[test]
fn unknown_rules_and_unavailable_sources_remain_explicit_incomplete_context() {
    let f = Fixture::new();
    f.ok(&["context", "bootstrap"]);
    fs::write(
        f.0.join("rules.md"),
        "# Unknown\n\nUnclassified obligation\n",
    )
    .unwrap();
    let r = f.run(&["context", "bootstrap"]);
    assert!(!r.status.success());
    let pack: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(pack["context"]["complete"], false);
    assert!(
        pack["rendered_context"]
            .as_str()
            .unwrap()
            .contains("CONTEXT INCOMPLETE")
    );
    assert!(
        pack["context"]["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["code"] == "rule_applicability_unknown")
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&r.stderr).unwrap()["code"],
        "ContextIncomplete"
    );
    fs::remove_file(f.0.join("rules.md")).unwrap();
    let missing = f.run(&["context", "bootstrap", "--budget", "3000"]);
    assert!(!missing.status.success());
    let pack: Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(pack["context"]["complete"], false);
    assert!(
        pack["context"]["source_revisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["freshness"] == "unavailable")
    );
    assert_eq!(pack["context"]["work"]["external_key"], "W");
}
