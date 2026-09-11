use awr_core::Id;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
struct Project(PathBuf);
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_awr"))
        .args(["--project", root.to_str().unwrap(), "--json"])
        .args(args)
        .output()
        .unwrap()
}
fn ok(root: &Path, args: &[&str]) -> Value {
    let r = run(root, args);
    assert!(
        r.status.success(),
        "{args:?}: {}\n{}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    serde_json::from_slice(&r.stdout).unwrap()
}
impl Project {
    fn new() -> Self {
        let p = Self(std::env::temp_dir().join(format!("awr-summary-{}", Id::new())));
        fs::create_dir(&p.0).unwrap();
        fs::write(p.0.join("mapping.toml"),"[project]\nname='Guide'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        p.source("First");
        ok(&p.0, &["init", "--manifest", "mapping.toml", "--accept"]);
        p
    }
    fn source(&self, edition: &str) {
        let mut text="goals:\n- id: G\n  title: Publish a useful guide\n  summary: Explain the complete workflow\n  status: active\nwork_items:\n".to_owned();
        for i in 0..80 {
            text.push_str(&format!("- id: W{i:03}\n  title: {edition} section {i}\n  status: {}\n  goal: G\n  acceptance: [Useful section]\n  next_action: Draft section {i}\n", if i<8 {"in_progress"}else if i<18 {"blocked"}else{"ready"}));
        }
        fs::write(self.0.join("work.yaml"), text).unwrap();
    }
}

#[test]
fn summary_preserves_counts_focus_and_explicit_omissions() {
    let p = Project::new();
    let full = ok(&p.0, &["status"]);
    let summary = ok(&p.0, &["status", "--view", "summary", "--goal", "G"]);
    assert!(full.get("view").is_none());
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["total"], full["total"]);
    assert_eq!(summary["counts"], full["counts"]);
    for key in ["current_total", "ready_count", "blocked_count"] {
        assert_eq!(summary[key], full[key]);
    }
    assert_eq!(summary["current_total"], 8);
    assert_eq!(summary["current"].as_array().unwrap().len(), 5);
    assert_eq!(summary["omissions"]["current"], 3);
    assert!(summary["omissions"]["blockers"].as_u64().unwrap() > 0);
    assert_eq!(summary["source_freshness"]["fresh"], 1);
    assert_eq!(
        summary["suggested_work"]["key"],
        full["suggested_work"]["external_key"]
    );
    assert!(serde_json::to_vec(&summary).unwrap().len() < serde_json::to_vec(&full).unwrap().len());
}
#[test]
fn exact_scope_never_silently_hides_invalid_selection() {
    let p = Project::new();
    let selected = ok(
        &p.0,
        &[
            "status", "--view", "summary", "--goal", "G", "--work", "W000", "--work", "W079",
        ],
    );
    assert_eq!(selected["total"], 2);
    assert_eq!(selected["omissions"]["outside_scope"], 78);
    assert_eq!(selected["current_total"], 1);
    assert_eq!(selected["ready_count"], 1);
    for args in [
        vec!["status", "--work", "W000"],
        vec!["status", "--view", "summary", "--goal", "MISSING"],
        vec!["status", "--view", "summary", "--work", "MISSING"],
        vec!["status", "--view", "summary", "--milestone", "MISSING"],
    ] {
        assert!(!run(&p.0, &args).status.success(), "{args:?}");
    }
    fs::remove_file(p.0.join("work.yaml")).unwrap();
    let cached = ok(&p.0, &["status", "--cached", "--view", "summary"]);
    assert_eq!(cached["snapshot"]["source_currentness_verified"], false);
    assert!(!run(&p.0, &["status", "--view", "summary"]).status.success());
}
