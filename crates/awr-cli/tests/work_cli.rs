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
        let root = std::env::temp_dir().join(format!("awr-work-cli-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("work-ledger.yaml"),"work_items:\n- id: W-1\n  title: Build feature\n  status: ready\n  next_action: Implement query\n  depends_on: [BASE]\n  acceptance: [Preserve provenance]\n- id: BASE\n  title: Base\n  status: completed\n- id: W-2\n  title: Waiting\n  status: planned\n  depends_on: [MISSING]\n- id: OTHER\n  title: UNRELATED_LEDGER_SENTINEL\n  status: completed\n  summary: PRIVATE_FULL_HISTORY_SENTINEL\n").unwrap();
        let f = Self(root);
        assert!(f.run(&["init", "--accept"]).status.success());
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
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn compact_queries_refresh_source_state_and_explain_dependencies() {
    let f = Fixture::new();
    let status = f.ok(&["status"]);
    assert_eq!(status["total"], 4);
    assert_eq!(status["ready_count"], 1);
    // Scheduler readiness alone does not establish a goal or business execution readiness.
    assert!(status["suggested_work"].is_null());
    assert_eq!(status["organization"]["state"], "needs_organization");
    assert_eq!(status["organization"]["business_execution_ready"], false);
    let ready = f.ok(&["ready", "--limit", "1"]);
    assert_eq!(ready["ready"][0]["external_key"], "W-1");
    assert_eq!(ready["diagnostic_counts"]["missing_dependency"], 1);
    let work = f.ok(&["work", "show", "W-1"]);
    assert_eq!(work["work"]["next_action"], "Implement query");
    assert_eq!(work["acceptance"][0], "Preserve provenance");
    assert_eq!(work["required_dependencies"][0]["external_key"], "BASE");
    assert!(!work.to_string().contains("SENTINEL"));
    assert!(work.get("events").is_none());
    let source = f.0.join("work-ledger.yaml");
    let text = fs::read_to_string(&source).unwrap();
    fs::write(
        &source,
        text.replace("status: ready", "status: blocked\n  blocker: Needs input")
            .replace("Implement query", "Resolve input"),
    )
    .unwrap();
    let changed = f.ok(&["work", "show", "W-1"]);
    assert_eq!(changed["work"]["status"], "blocked");
    assert_eq!(changed["work"]["next_action"], "Resolve input");
    assert_eq!(changed["work"]["blocker"], "Needs input");
    assert!(
        changed["project_revision"].as_u64().unwrap() > work["project_revision"].as_u64().unwrap()
    );
    assert_eq!(f.ok(&["ready"])["ready_total"], 0);
    assert!(!f.run(&["work", "show", "MISSING"]).status.success());
    assert!(!f.run(&["ready", "--limit", "0"]).status.success());
}

#[test]
fn unavailable_source_is_reported_without_silent_cached_success() {
    let f = Fixture::new();
    fs::remove_file(f.0.join("work-ledger.yaml")).unwrap();
    for args in [
        &["status"][..],
        &["ready"][..],
        &["work", "show", "W-1"][..],
    ] {
        let output = f.run(args);
        assert!(!output.status.success());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["ok"], false);
        assert!(!report["source_issues"].as_array().unwrap().is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
            "SourceStale"
        );
    }
}

#[test]
fn search_cli_combines_text_and_structured_filters() {
    let f = Fixture::new();
    let result = f.ok(&[
        "search", "Build", "--type", "work", "--status", "ready", "--work", "W-1",
    ]);
    assert_eq!(result["hits"].as_array().unwrap().len(), 1);
    assert_eq!(result["hits"][0]["external_key"], "W-1");
    assert!(result["hits"][0]["rank"].as_f64().unwrap() < 0.0);
    assert!(result["hits"][0]["source_ref"].is_object());
    let no_match = f.ok(&["search", "Build", "--status", "completed"]);
    assert!(no_match["hits"].as_array().unwrap().is_empty());
    assert!(!f.run(&["search", "Build", "--limit", "0"]).status.success());
    let diagnosed = f.run(&["doctor"]);
    assert!(!diagnosed.status.success());
    let diagnosis: Value = serde_json::from_slice(&diagnosed.stdout).unwrap();
    assert_eq!(diagnosis["schema_version"], awr_store::SCHEMA_VERSION);
    assert_eq!(diagnosis["database_ok"], true);
    assert!(
        diagnosis["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| {
                finding["code"] == "missing_dependency"
                    && finding["message"].as_str().unwrap().contains("MISSING")
            })
    );
}
