use awr_core::{CheckpointDraft, Id};
use awr_store::Store;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

const WORK: &str = "work_items:\n- id: W\n  title: Recover interrupted work\n  status: in_progress\n  next_action: Continue recovery\n  acceptance: [Preserve recorded work]\n- id: OTHER\n  title: Another task\n  status: ready\n";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-doctor-cli-{}", Id::new()));
        fs::create_dir_all(root.join("decisions")).unwrap();
        fs::write(root.join("work.yaml"), WORK).unwrap();
        fs::write(
            root.join("decisions/ADR.md"),
            "# Preserve history\n\nStatus: accepted\n\n## Decision\nKeep immutable receipts.\n",
        )
        .unwrap();
        fs::write(root.join("sources.toml"),"[project]\nname='Doctor fixture'\nexternal_key='doctor'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='decisions'\nadapter='markdown-directory-v1'\n").unwrap();
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
        assert!(
            r.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn problems(&self) -> Value {
        let r = self.run(&["doctor"]);
        assert!(!r.status.success());
        let v: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["read_only"], true);
        assert_eq!(v["source_refresh_performed"], false);
        v
    }
    fn revision(&self) -> String {
        self.ok(&["session", "list"])["project_revision"].to_string()
    }
    fn start(&self, expires: bool) -> Value {
        let revision = self.revision();
        let mut args = vec![
            "session",
            "start",
            "--work",
            "W",
            "--agent",
            "worker",
            "--provider",
            "fixture",
            "--model",
            "test",
            "--expected-revision",
            &revision,
        ];
        if expires {
            args.extend(["--claim", "--ttl-ms", "1"]);
        }
        self.ok(&args)
    }
    fn repair(&self, action: &str, id: &str) -> Value {
        self.ok(&[
            "doctor",
            "repair",
            action,
            id,
            "--expected-revision",
            &self.revision(),
            "--reason",
            "Writer has stopped; retain its recorded history",
        ])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn has(v: &Value, code: &str) -> bool {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["code"] == code)
}

#[test]
fn source_and_artifact_diagnosis_reads_current_files_without_changing_the_projection() {
    let f = Fixture::new();
    let started = f.start(false);
    fs::write(f.0.join("report.txt"), "PRIVATE_ARTIFACT_BODY").unwrap();
    let imported = f.ok(&[
        "artifact",
        "add",
        "report.txt",
        "--type",
        "report",
        "--mime",
        "text/plain",
        "--source-event",
        started["event"]["id"].as_str().unwrap(),
        "--expected-revision",
        &f.revision(),
    ]);
    let healthy = f.ok(&["doctor"]);
    assert_eq!(healthy["database_ok"], true);
    assert_eq!(healthy["sources_checked"], 2);
    assert_eq!(healthy["artifacts_checked"], 1);
    let revision = f.revision();
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace("Continue recovery", "Read changed source facts"),
    )
    .unwrap();
    fs::write(
        f.0.join("decisions/New.md"),
        "# New decision\n\nStatus: accepted\n\n## Decision\nKeep new changes.\n",
    )
    .unwrap();
    let orphan = f.0.join(".awr/artifacts/unregistered");
    fs::write(&orphan, "PRIVATE_ORPHAN_BODY").unwrap();
    let path = f.0.join(imported["artifact"]["locator"].as_str().unwrap());
    fs::write(&path, "X".repeat("PRIVATE_ARTIFACT_BODY".len())).unwrap();
    let state = f.problems();
    for code in [
        "source_fingerprint_changed",
        "source_not_indexed",
        "artifact_digest_mismatch",
        "orphan_artifact",
    ] {
        assert!(has(&state, code), "missing {code}: {state}");
    }
    assert!(!state.to_string().contains("PRIVATE_"));
    assert_eq!(f.revision(), revision);
    assert!(orphan.exists());
    let store = Store::open_readonly(&f.0.join(".awr/state.db")).unwrap();
    let p = store.project_by_root(&f.0).unwrap();
    let work = store.work_item(p.id, "W").unwrap();
    assert_eq!(work.item.next_action, "Continue recovery");
    assert_eq!(work.source.freshness, awr_core::Freshness::Fresh);
    drop(store);
    fs::remove_file(f.0.join("decisions/ADR.md")).unwrap();
    assert!(has(&f.problems(), "source_no_longer_selected"));
    fs::remove_file(path).unwrap();
    assert!(has(&f.problems(), "artifact_missing"));
    assert!(
        f.ok(&["doctor", "--database-only"])["ok"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn selected_repairs_close_expiry_and_incomplete_save_without_creating_a_checkpoint() {
    let f = Fixture::new();
    let started = f.start(true);
    let sid = started["session"]["id"].as_str().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let attempt = {
        let mut store = Store::open(&f.0.join(".awr/state.db")).unwrap();
        let p = store.project_by_root(&f.0).unwrap();
        store
            .begin_checkpoint_save(
                p.id,
                p.project_revision,
                sid.parse().unwrap(),
                CheckpointDraft {
                    context_hash: "a".repeat(64),
                    digest: "Uncommitted save draft".into(),
                    next_action: "Keep recovery available".into(),
                    open_loops: vec!["Review interrupted work".into()],
                    changed_entities: vec![],
                },
            )
            .unwrap()
    };
    let before = f.problems();
    assert!(has(&before, "expired_claim"));
    assert!(has(&before, "incomplete_checkpoint"));
    let revision = f.revision();
    let stale = f.run(&[
        "doctor",
        "repair",
        "expire-claim",
        started["claim"]["id"].as_str().unwrap(),
        "--expected-revision",
        "0",
        "--reason",
        "Expired",
    ]);
    assert!(!stale.status.success());
    assert_eq!(f.revision(), revision);
    let result = f.repair("expire-claim", started["claim"]["id"].as_str().unwrap());
    assert_eq!(result["repair_applied"], true);
    assert_eq!(result["remaining_problems_evaluated"], false);
    let pending = f.problems();
    assert!(!has(&pending, "expired_claim"));
    assert!(has(&pending, "incomplete_checkpoint"));
    f.repair("abandon-checkpoint", &attempt.id.to_string());
    let shown = f.ok(&["session", "show", sid]);
    assert!(shown["checkpoint"].is_null());
    assert_eq!(shown["checkpoint_saves"]["incomplete_count"], 0);
    assert_eq!(
        shown["checkpoint_saves"]["attempts"][0]["status"],
        "abandoned"
    );
    let intact = f.ok(&["doctor"]);
    assert!(!has(&intact, "incomplete_checkpoint"));
    f.repair("interrupt-session", sid);
    assert_eq!(
        f.ok(&["session", "show", sid])["session"]["status"],
        "interrupted"
    );
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);
}

#[test]
fn missing_sources_do_not_hide_runtime_findings_or_prevent_explicit_cleanup() {
    let f = Fixture::new();
    let started = f.start(true);
    std::thread::sleep(std::time::Duration::from_millis(5));
    fs::remove_file(f.0.join(".awr/project.toml")).unwrap();
    let before = f.revision();
    let report = f.problems();
    assert!(has(&report, "source_manifest_unavailable"));
    assert!(has(&report, "expired_claim"));
    assert_eq!(f.revision(), before);
    f.repair("expire-claim", started["claim"]["id"].as_str().unwrap());
    let after = f.problems();
    assert!(has(&after, "source_manifest_unavailable"));
    assert!(!has(&after, "expired_claim"));
    assert!(!f.0.join(".awr/project.toml").exists());
}

#[test]
fn invalid_repair_selection_and_missing_database_do_not_create_or_change_state() {
    let f = Fixture::new();
    let started = f.start(false);
    let sid = started["session"]["id"].as_str().unwrap();
    let revision = f.revision();
    let mixed = f.run(&[
        "doctor",
        "--database-only",
        "repair",
        "interrupt-session",
        sid,
        "--expected-revision",
        &revision,
        "--reason",
        "Stop selected writer",
    ]);
    assert!(!mixed.status.success());
    assert_eq!(f.revision(), revision);
    let empty = f.run(&[
        "doctor",
        "repair",
        "interrupt-session",
        sid,
        "--expected-revision",
        &revision,
        "--reason",
        " ",
    ]);
    assert!(!empty.status.success());
    assert_eq!(f.revision(), revision);
    let missing = f.0.join("missing.db");
    let result = f.run(&[
        "doctor",
        "--database",
        missing.to_str().unwrap(),
        "repair",
        "interrupt-session",
        sid,
        "--expected-revision",
        &revision,
        "--reason",
        "Stop selected writer",
    ]);
    assert!(!result.status.success());
    assert!(!missing.exists());
    assert_eq!(f.revision(), revision);
}
