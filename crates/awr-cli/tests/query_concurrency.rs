use awr_core::Id;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{Arc, Barrier},
    thread,
    time::Duration,
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
        let p = Self(std::env::temp_dir().join(format!("awr-parallel-{}", Id::new())));
        fs::create_dir(&p.0).unwrap();
        fs::write(p.0.join("mapping.toml"),"[project]\nname='Guide'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        p.source("First");
        ok(&p.0, &["init", "--manifest", "mapping.toml", "--accept"]);
        p
    }
    fn source(&self, edition: &str) {
        let mut text="goals:\n- id: G\n  title: Publish a useful guide\n  summary: Explain the complete workflow\n  status: active\nwork_items:\n".to_owned();
        for i in 0..80 {
            text.push_str(&format!("- id: W{i:03}\n  title: {edition} section {i}\n  status: ready\n  goal: G\n  acceptance: [Useful section]\n  next_action: Draft section {i}\n"));
        }
        fs::write(self.0.join("work.yaml"), text).unwrap();
    }
}
#[test]
fn stable_and_changed_sources_support_parallel_status_work_and_context_reads() {
    let p = Project::new();
    for edition in ["First", "Revised"] {
        if edition == "Revised" {
            p.source(edition);
        }
        let barrier = Arc::new(Barrier::new(12));
        let joins = (0..12)
            .map(|i| {
                let root = p.0.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    match i % 3 {
                        0 => {
                            let v = ok(&root, &["status"]);
                            assert_eq!(v["snapshot"]["coherent"], true);
                            assert_eq!(v["total"], 80);
                            v["project_revision"].clone()
                        }
                        1 => {
                            let v = ok(&root, &["work", "show", "W000"]);
                            assert_eq!(v["snapshot"]["coherent"], true);
                            assert_eq!(v["work"]["title"], format!("{edition} section 0"));
                            v["project_revision"].clone()
                        }
                        _ => {
                            let v = ok(&root, &["context", "compile", "--work", "W000"]);
                            assert_eq!(v["completeness"]["complete"], true);
                            v["project_revision"].clone()
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        let revisions = joins
            .into_iter()
            .map(|j| j.join().unwrap())
            .collect::<Vec<_>>();
        assert!(revisions.iter().all(|r| r == &revisions[0]));
    }
}
#[test]
fn runtime_revision_and_source_token_have_distinct_lifetimes() {
    let p = Project::new();
    let before = ok(&p.0, &["status"]);
    ok(
        &p.0,
        &[
            "session",
            "start",
            "--work",
            "W000",
            "--agent",
            "fixture",
            "--provider",
            "local",
            "--model",
            "none",
            "--expected-revision",
            &before["project_revision"].to_string(),
        ],
    );
    let after = ok(&p.0, &["status"]);
    assert_ne!(before["project_revision"], after["project_revision"]);
    assert_eq!(
        before["snapshot"]["source_state_fingerprint"],
        after["snapshot"]["source_state_fingerprint"]
    );
    p.source("Updated");
    let changed = ok(&p.0, &["status"]);
    assert_ne!(
        after["snapshot"]["source_state_fingerprint"],
        changed["snapshot"]["source_state_fingerprint"]
    );
}
#[test]
fn cached_read_is_explicit_and_source_failures_remain_visible_on_refresh() {
    let p = Project::new();
    fs::remove_file(p.0.join("work.yaml")).unwrap();
    let bytes = fs::read(p.0.join(".awr/state.db")).unwrap();
    let names = || {
        fs::read_dir(p.0.join(".awr"))
            .unwrap()
            .map(|p| p.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before = names();
    let cached = ok(&p.0, &["status", "--cached"]);
    assert_eq!(cached["read_only"], true);
    assert_eq!(cached["source_refresh_performed"], false);
    assert_eq!(cached["snapshot"]["source_currentness_verified"], false);
    assert_eq!(cached["total"], 80);
    assert!(fs::read(p.0.join(".awr/state.db")).unwrap() == bytes);
    assert_eq!(names(), before);
    let failure = run(&p.0, &["status"]);
    assert!(!failure.status.success());
    let value: Value = serde_json::from_slice(&failure.stdout).unwrap();
    assert_eq!(value["ok"], false);
    assert!(!value["source_issues"].as_array().unwrap().is_empty());
    assert_eq!(value["snapshot"]["source_currentness_verified"], false);
}
#[test]
fn a_reader_waits_for_the_source_transition_guard_then_uses_a_coherent_snapshot() {
    let p = Project::new();
    let store = awr_store::Store::open_existing(&p.0.join(".awr/state.db")).unwrap();
    let guard = store.lock_sources().unwrap();
    let root = p.0.clone();
    let worker = thread::spawn(move || ok(&root, &["status"]));
    thread::sleep(Duration::from_millis(100));
    drop(guard);
    let result = worker.join().unwrap();
    assert_eq!(result["snapshot"]["coherent"], true);
}
