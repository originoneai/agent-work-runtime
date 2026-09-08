use awr_core::Id;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

const WORK: &str = "work_items:\n- id: W\n  title: Deliver the report\n  milestone: M1\n  status: in_progress\n  next_action: Review the draft\n  acceptance: [Keep source authority]\n- id: OTHER\n  title: Separate work\n  status: ready\n  next_action: Wait\n";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-proposal-cli-{}", Id::new()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("work.yaml"), WORK).unwrap();
        fs::write(
            root.join("goal.md"),
            "# Deliver a useful report {#goal}\n\nPreserve the source facts.\n",
        )
        .unwrap();
        fs::write(root.join("sources.toml"), "[project]\nname='Proposal fixture'\nexternal_key='proposal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='goal.md'\nadapter='markdown-heading-v1'\n[sources.options]\nstatus='active'\nkey_prefix='goals'\n").unwrap();
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
            String::from_utf8_lossy(&r.stderr),
            String::from_utf8_lossy(&r.stdout)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn revision(&self) -> String {
        self.ok(&["proposal", "list"])["project_revision"].to_string()
    }
    fn create(&self) -> Value {
        self.ok(&[
            "proposal",
            "create",
            "--kind",
            "work",
            "--target",
            "W",
            "--intent",
            "Continue review after producing the draft",
            "--patch",
            r#"{"next_action":"Read the revised report"}"#,
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn review(&self, action: &str, id: &str) -> Output {
        self.run(&[
            "proposal",
            action,
            id,
            "--actor",
            "reviewer",
            "--reason",
            "Reviewed the exact proposal",
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn reviewed(&self, action: &str, id: &str) -> Value {
        let r = self.review(action, id);
        assert!(
            r.status.success(),
            "{} {}",
            String::from_utf8_lossy(&r.stderr),
            String::from_utf8_lossy(&r.stdout)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn error(&self, r: &Output, code: &str) {
        assert!(!r.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&r.stderr).unwrap()["code"],
            code
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn lifecycle_preserves_exact_binding_and_never_labels_approval_as_applied() {
    let f = Fixture::new();
    let rev = f.revision();
    let created = f.create();
    let p = &created["proposal"];
    let id = p["id"].as_str().unwrap();
    assert_eq!(p["status"], "draft");
    assert_eq!(p["expected_revision"].to_string(), rev);
    assert_eq!(p["revision"], 1);
    assert_eq!(
        p["source_id"],
        p["patch"]["target"]["meta"]["source_ref"]["source_id"]
    );
    assert_eq!(
        p["base_fingerprint"],
        p["patch"]["target"]["meta"]["source_ref"]["source_fingerprint"]
    );
    assert_eq!(p["patch"]["target"]["meta"]["external_key"], "W");
    assert!(!p["patch"]["source_config"]["mapping_key"].is_null());
    assert_eq!(created["source_write_performed"], false);
    let ready = f.reviewed("submit", id);
    assert_eq!(ready["proposal"]["status"], "ready");
    let approved = f.reviewed("approve", id);
    assert_eq!(approved["proposal"]["status"], "approved");
    assert_eq!(approved["proposal"]["patch"], p["patch"]);
    assert_eq!(approved["event"]["payload"]["actor"], "reviewer");
    let apply = f.review("apply", id);
    f.error(&apply, "proposal_required");
    let report: Value = serde_json::from_slice(&apply.stdout).unwrap();
    assert_eq!(report["code"], "proposal_required");
    assert_eq!(report["proposal"]["status"], "approved");
    assert_eq!(report["event"]["event_type"], "proposal.required");
    assert_eq!(report["source_write_performed"], false);
    assert_eq!(
        report["proposal"]["base_fingerprint"],
        p["base_fingerprint"]
    );
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);
    assert_eq!(
        f.ok(&["work", "show", "W"])["work"]["next_action"],
        "Review the draft"
    );
    assert!(
        f.ok(&["proposal", "list", "--status", "applied"])["proposals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let summary = f.ok(&["proposal", "show", id]);
    assert!(summary["proposal"].get("patch").is_none());
    assert_eq!(
        f.ok(&["proposal", "show", id, "--full"])["proposal"]["patch"],
        p["patch"]
    );
    assert_eq!(f.reviewed("reject", id)["proposal"]["status"], "rejected");
    let rev = f.revision();
    f.error(&f.review("approve", id), "InvalidTransition");
    f.error(&f.review("apply", id), "InvalidTransition");
    assert_eq!(f.revision(), rev);
}

#[test]
fn unindexed_file_change_is_a_durable_conflict_and_queries_work_without_manifest() {
    let f = Fixture::new();
    let created = f.create();
    let id = created["proposal"]["id"].as_str().unwrap();
    f.reviewed("submit", id);
    let new_work = WORK.replace("Review the draft", "Keep the author's newer changes");
    fs::write(f.0.join("work.yaml"), &new_work).unwrap();
    let conflict = f.review("approve", id);
    f.error(&conflict, "SourceConflict");
    let report: Value = serde_json::from_slice(&conflict.stdout).unwrap();
    assert_eq!(report["proposal"]["status"], "conflict");
    assert_eq!(report["event"]["event_type"], "proposal.conflict");
    assert_eq!(report["source_refresh_performed"], false);
    assert_eq!(report["proposal"]["patch"], created["proposal"]["patch"]);
    fs::remove_file(f.0.join(".awr/project.toml")).unwrap();
    let rev = f.revision();
    assert_eq!(
        f.ok(&["proposal", "show", id])["proposal"]["status"],
        "conflict"
    );
    assert_eq!(
        f.ok(&["proposal", "list", "--status", "conflict"])["proposals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(f.revision(), rev);
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), new_work);
}

#[test]
fn mapping_changes_conflict_and_unreadable_sources_fail_without_fabricating_writes() {
    let f = Fixture::new();
    let created = f.create();
    let id = created["proposal"]["id"].as_str().unwrap();
    let path = f.0.join(".awr/project.toml");
    let mut manifest = awr_source::Manifest::load(&f.0).unwrap();
    manifest.sources[0]
        .options
        .insert("root_key".into(), toml::Value::String("work_items".into()));
    fs::write(&path, toml::to_string(&manifest).unwrap()).unwrap();
    let conflict = f.review("submit", id);
    f.error(&conflict, "SourceConflict");
    assert_eq!(
        serde_json::from_slice::<Value>(&conflict.stdout).unwrap()["proposal"]["status"],
        "conflict"
    );
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);

    for scan_first in [false, true] {
        let f = Fixture::new();
        let created = f.create();
        let id = created["proposal"]["id"].as_str().unwrap();
        fs::remove_file(f.0.join("work.yaml")).unwrap();
        if scan_first {
            // Already-observed unavailability has the same meaning as a direct read failure.
            f.error(&f.run(&["source", "scan"]), "SourceStale");
        }
        let failed = f.review("submit", id);
        f.error(&failed, "SourceUnavailable");
        assert_eq!(
            serde_json::from_slice::<Value>(&failed.stdout).unwrap()["proposal"]["status"],
            "failed"
        );
        assert!(!f.0.join("work.yaml").exists());
    }
    // Rejection does not need to read the source or manifest.
    let f = Fixture::new();
    let created = f.create();
    fs::remove_file(f.0.join(".awr/project.toml")).unwrap();
    assert_eq!(
        f.reviewed("reject", created["proposal"]["id"].as_str().unwrap())["proposal"]["status"],
        "rejected"
    );
}

#[test]
fn revisions_and_invalid_actions_do_not_advance_proposal_or_project() {
    let f = Fixture::new();
    let created = f.create();
    let id = created["proposal"]["id"].as_str().unwrap();
    let rev = f.revision();
    f.error(&f.review("approve", id), "InvalidTransition");
    f.error(
        &f.run(&[
            "proposal",
            "submit",
            id,
            "--actor",
            "reviewer",
            "--reason",
            "Reviewed",
            "--expected-revision",
            "0",
        ]),
        "RevisionConflict",
    );
    f.error(
        &f.run(&[
            "proposal",
            "submit",
            id,
            "--actor",
            "reviewer",
            "--reason",
            " ",
            "--expected-revision",
            &rev,
        ]),
        "InvalidInput",
    );
    assert_eq!(f.revision(), rev);
    assert_eq!(
        f.ok(&["proposal", "show", id, "--full"])["proposal"],
        created["proposal"]
    );
    f.error(
        &f.run(&["proposal", "list", "--limit", "101"]),
        "InvalidInput",
    );
    for patch in [
        "{}",
        "[]",
        r#"{"id":"overwrite"}"#,
        r#"{"source_ref":null}"#,
    ] {
        f.error(
            &f.run(&[
                "proposal",
                "create",
                "--kind",
                "work",
                "--target",
                "W",
                "--intent",
                "Invalid patch",
                "--patch",
                patch,
                "--expected-revision",
                &rev,
            ]),
            "InvalidInput",
        );
    }
    assert_eq!(f.revision(), rev);
    assert_eq!(
        f.ok(&["proposal", "list"])["proposals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn markdown_proposals_remain_reviewable_and_explicitly_require_manual_handling() {
    let f = Fixture::new();
    fs::write(
        f.0.join("change.json"),
        serde_json::to_vec(&json!({"summary":"Revise the report goal"})).unwrap(),
    )
    .unwrap();
    let before = fs::read(f.0.join("goal.md")).unwrap();
    let p = f.ok(&[
        "proposal",
        "create",
        "--kind",
        "goal",
        "--target",
        "goals#goal",
        "--intent",
        "Clarify the project goal",
        "--patch-file",
        "change.json",
        "--expected-revision",
        &f.revision(),
    ]);
    let id = p["proposal"]["id"].as_str().unwrap();
    f.reviewed("submit", id);
    f.reviewed("approve", id);
    let apply = f.review("apply", id);
    f.error(&apply, "proposal_required");
    assert_eq!(
        serde_json::from_slice::<Value>(&apply.stdout).unwrap()["proposal"]["status"],
        "approved"
    );
    assert_eq!(fs::read(f.0.join("goal.md")).unwrap(), before);
}

#[test]
fn session_binding_and_source_drift_require_explicit_reindex_without_hidden_writes() {
    let f = Fixture::new();
    let session = f.ok(&[
        "session",
        "start",
        "--work",
        "OTHER",
        "--agent",
        "author",
        "--provider",
        "fixture",
        "--model",
        "local",
        "--expected-revision",
        &f.revision(),
    ]);
    let sid = session["session"]["id"].as_str().unwrap();
    let r = f.run(&[
        "proposal",
        "create",
        "--kind",
        "work",
        "--target",
        "W",
        "--intent",
        "Review report",
        "--patch",
        r#"{"next_action":"Review"}"#,
        "--session",
        sid,
        "--expected-revision",
        &f.revision(),
    ]);
    f.error(&r, "InvalidInput");
    assert!(
        f.ok(&["proposal", "list"])["proposals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let old_rev = f.revision();
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace("Review the draft", "New source action"),
    )
    .unwrap();
    let stale = f.run(&[
        "proposal",
        "create",
        "--kind",
        "work",
        "--target",
        "W",
        "--intent",
        "Review report",
        "--patch",
        r#"{"next_action":"Review"}"#,
        "--expected-revision",
        &old_rev,
    ]);
    f.error(&stale, "SourceConflict");
    assert_eq!(f.revision(), old_rev);
    assert!(
        f.ok(&["proposal", "list"])["proposals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    f.ok(&["source", "reindex"]);
    let fresh = f.create();
    assert_eq!(fresh["proposal"]["patch"]["target"]["meta"]["revision"], 2);
}

#[test]
fn git_proposal_binds_the_resolved_snapshot_and_conflicts_when_the_ref_moves() {
    let f = Fixture::new();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(&f.0)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-b", "main"]);
    git(&["add", "goal.md"]);
    git(&[
        "-c",
        "user.name=Proposal fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "Initial source",
    ]);
    let mut manifest = awr_source::Manifest::load(&f.0).unwrap();
    manifest.sources[1].path = None;
    manifest.sources[1].locator = Some("git://main:goal.md".into());
    manifest.sources[1]
        .options
        .insert("key_prefix".into(), toml::Value::String("git-goals".into()));
    fs::write(
        f.0.join(".awr/project.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    f.ok(&["source", "reindex"]);
    let proposal = f.ok(&[
        "proposal",
        "create",
        "--kind",
        "goal",
        "--target",
        "git-goals#goal",
        "--intent",
        "Review a committed goal",
        "--patch",
        r#"{"summary":"Clarify the goal"}"#,
        "--expected-revision",
        &f.revision(),
    ]);
    let id = proposal["proposal"]["id"].as_str().unwrap();
    let locator = proposal["proposal"]["patch"]["target"]["meta"]["source_ref"]["locator"]
        .as_str()
        .unwrap();
    assert!(locator.starts_with("git://"));
    assert!(!locator.contains("main:"));
    f.reviewed("submit", id);
    f.reviewed("approve", id);
    let bytes = fs::read(f.0.join("goal.md")).unwrap();
    // Identical bytes at a new commit still disagree with the immutable source reference.
    git(&[
        "-c",
        "user.name=Proposal fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "--allow-empty",
        "-m",
        "Advance source ref",
    ]);
    let conflict = f.review("apply", id);
    f.error(&conflict, "SourceConflict");
    let report: Value = serde_json::from_slice(&conflict.stdout).unwrap();
    assert_eq!(report["proposal"]["status"], "conflict");
    assert_eq!(report["source_write_performed"], false);
    assert_eq!(report["proposal"]["patch"], proposal["proposal"]["patch"]);
    assert_eq!(fs::read(f.0.join("goal.md")).unwrap(), bytes);
}
