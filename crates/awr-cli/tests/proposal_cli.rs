use awr_core::*;
use awr_store::Store;
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
    fn approved(&self) -> String {
        let value = self.create();
        let id = value["proposal"]["id"].as_str().unwrap();
        self.reviewed("submit", id);
        self.reviewed("approve", id);
        id.into()
    }
    fn prepared(&self, id: &str) -> awr_source::PreparedYamlMutation {
        let store = Store::open_existing(&self.0.join(".awr/state.db")).unwrap();
        let project = store
            .project_by_root(&self.0.canonicalize().unwrap())
            .unwrap()
            .id;
        let proposal = store.proposal(project, id.parse().unwrap()).unwrap();
        let source = store.source(project, proposal.source_id).unwrap();
        awr_source::prepare_yaml_mutation(
            &self.0,
            &source,
            &proposal,
            store.projection_ids(&source).unwrap(),
        )
        .unwrap()
    }
    // Reproduce a process interruption at an explicit persisted boundary, without a
    // production fault switch or a fake successful apply response.
    fn interrupted(&self, id: &str, phase: &str) -> MutationApplyAttempt {
        let prepared = self.prepared(id);
        let recovery = self.0.join(prepared.plan.recovery_directory());
        fs::create_dir_all(&recovery).unwrap();
        fs::write(recovery.join("before.yaml"), &prepared.before.bytes).unwrap();
        fs::write(recovery.join("after.yaml"), &prepared.after.bytes).unwrap();
        fs::write(
            recovery.join("plan.json"),
            serde_json::to_vec(&prepared.plan).unwrap(),
        )
        .unwrap();
        let mut store = Store::open_existing(&self.0.join(".awr/state.db")).unwrap();
        let project = store
            .project_by_root(&self.0.canonicalize().unwrap())
            .unwrap();
        let attempt = store
            .begin_proposal_apply(
                project.id,
                project.project_revision,
                id.parse().unwrap(),
                prepared.plan,
                "writer",
                "Simulated interruption boundary",
            )
            .unwrap()
            .0;
        if phase != "before" {
            fs::write(&prepared.path, &prepared.after.bytes).unwrap();
        }
        if phase == "projected" {
            let source = store.source(project.id, attempt.source_id).unwrap();
            let proposal = store.proposal(project.id, attempt.proposal_id).unwrap();
            let patch = proposal.bound_patch().unwrap();
            let (_, spec, snapshot) =
                awr_source::inspect_mutation_source(&self.0, &source, &patch).unwrap();
            let (batch, _) = awr_source::parse_mutation_projection(
                &source,
                &spec,
                &snapshot,
                store.projection_ids(&source).unwrap(),
                &patch.target,
            )
            .unwrap();
            store
                .commit_source_projection(&source, &snapshot.fingerprint, batch)
                .unwrap();
        }
        attempt
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
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);
    let report = f.reviewed("apply", id);
    assert_eq!(report["proposal"]["status"], "applied");
    assert_eq!(report["event"]["event_type"], "proposal.applied");
    assert_eq!(report["source_write_performed"], true);
    assert_eq!(report["source_refresh_performed"], true);
    assert_eq!(report["write_outcome"], "applied");
    assert_eq!(
        report["proposal"]["base_fingerprint"],
        p["base_fingerprint"]
    );
    let after = fs::read_to_string(f.0.join("work.yaml")).unwrap();
    assert!(after.ends_with(
        "\n- id: OTHER\n  title: Separate work\n  status: ready\n  next_action: Wait\n"
    ));
    let recovery = f.0.join(report["recovery_directory"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(recovery.join("before.yaml")).unwrap(),
        WORK
    );
    assert_eq!(
        fs::read_to_string(recovery.join("after.yaml")).unwrap(),
        after
    );
    assert_eq!(
        f.ok(&["work", "show", "W"])["work"]["next_action"],
        "Read the revised report"
    );
    assert_eq!(
        f.ok(&["proposal", "list", "--status", "applied"])["proposals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let summary = f.ok(&["proposal", "show", id]);
    assert!(summary["proposal"].get("patch").is_none());
    assert_eq!(
        summary["apply_attempt"]["resolved_event_id"],
        report["event"]["id"]
    );
    assert_eq!(
        f.ok(&["proposal", "show", id, "--full"])["proposal"]["patch"],
        p["patch"]
    );
    let rev = f.revision();
    f.error(&f.review("reject", id), "InvalidTransition");
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
    manifest.sources[0].options.insert(
        "status_map".into(),
        toml::Value::Table(toml::Table::from_iter([(
            "pending".into(),
            toml::Value::String("planned".into()),
        )])),
    );
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

#[test]
fn recovery_resumes_before_write_after_write_and_after_projection_without_duplicate_writes() {
    for phase in ["before", "after", "projected"] {
        let f = Fixture::new();
        let id = f.approved();
        let attempt = f.interrupted(&id, phase);
        let pending = f.review("apply", &id);
        f.error(&pending, "MutationIncomplete");
        let report: Value = serde_json::from_slice(&pending.stdout).unwrap();
        assert_eq!(report["write_outcome"], "pending_recovery");
        assert_eq!(report["source_write_performed"], Value::Null);
        assert_eq!(report["apply_attempt"]["event_id"], json!(attempt.event_id));
        f.error(&f.review("reject", &id), "InvalidTransition");
        let before = fs::read(f.0.join("work.yaml")).unwrap();
        let revision = f.revision();
        let diagnosis = f.run(&["doctor"]);
        assert!(!diagnosis.status.success());
        let diagnosis: Value = serde_json::from_slice(&diagnosis.stdout).unwrap();
        let finding = diagnosis["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["code"] == "incomplete_mutation")
            .unwrap();
        assert!(finding.to_string().contains("proposal recover"));
        assert!(finding.to_string().contains(&attempt.event_id.to_string()));
        assert_eq!(f.revision(), revision);
        assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
        f.error(
            &f.run(&[
                "proposal",
                "recover",
                &id,
                "--actor",
                "reviewer",
                "--reason",
                "Resume interrupted write",
                "--expected-revision",
                "0",
            ]),
            "RevisionConflict",
        );
        assert_eq!(f.revision(), revision);
        assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
        let result = f.reviewed("recover", &id);
        assert_eq!(result["proposal"]["status"], "applied");
        assert_eq!(result["write_outcome"], "recovered");
        assert_eq!(result["source_write_performed"], phase == "before");
        assert_eq!(result["source_refresh_performed"], phase != "projected");
        assert_eq!(
            result["apply_attempt"]["resolved_event_id"],
            result["event"]["id"]
        );
        assert_eq!(
            f.ok(&["work", "show", "W"])["work"]["next_action"],
            "Read the revised report"
        );
        assert_eq!(
            f.ok(&["work", "show", "W"])["work"]["status"],
            "in_progress"
        );
        f.error(&f.review("recover", &id), "InvalidTransition");
    }
}

#[test]
fn recovery_preserves_a_newer_source_and_rejects_damaged_snapshots() {
    for damaged in [false, true] {
        let f = Fixture::new();
        let id = f.approved();
        let attempt = f.interrupted(&id, "before");
        let recovery = f.0.join(attempt.plan.recovery_directory());
        if damaged {
            fs::write(recovery.join("after.yaml"), b"changed recovery snapshot").unwrap();
        } else {
            fs::write(
                f.0.join("work.yaml"),
                format!("{WORK}# A newer human edit\n"),
            )
            .unwrap();
        }
        let before = fs::read(f.0.join("work.yaml")).unwrap();
        let result = f.review("recover", &id);
        f.error(&result, "SourceConflict");
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["proposal"]["status"], "conflict");
        assert_eq!(report["source_write_performed"], false);
        assert!(!report["apply_attempt"]["resolved_event_id"].is_null());
        assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
        assert_eq!(
            fs::read_to_string(recovery.join("before.yaml")).unwrap(),
            WORK
        );
    }
}

#[test]
fn application_journal_enforces_source_reservation_revision_and_post_write_proof() {
    let f = Fixture::new();
    let first = f.approved();
    let second = f.approved();
    let second_plan = f.prepared(&second).plan;
    let attempt = f.interrupted(&first, "before");
    let mut store = Store::open_existing(&f.0.join(".awr/state.db")).unwrap();
    let project = store.project_by_root(&f.0.canonicalize().unwrap()).unwrap();
    let rev = project.project_revision;
    assert!(matches!(
        store.begin_proposal_apply(
            project.id,
            rev,
            second.parse().unwrap(),
            second_plan.clone(),
            "writer",
            "Other proposal"
        ),
        Err(Error::MutationConflict(_))
    ));
    assert!(matches!(
        store.finish_proposal_apply(
            project.id,
            rev,
            attempt.proposal_id,
            attempt.event_id,
            "writer",
            "Cannot finalize old bytes"
        ),
        Err(Error::SourceConflict(_))
    ));
    assert!(matches!(
        store.finish_proposal_apply(
            project.id,
            rev,
            attempt.proposal_id,
            Id::new(),
            "writer",
            "Wrong attempt"
        ),
        Err(Error::InvalidTransition(_))
    ));
    assert!(matches!(
        store.fail_proposal_apply(
            project.id,
            0,
            attempt.proposal_id,
            attempt.event_id,
            false,
            "writer",
            "Stale cancellation"
        ),
        Err(Error::RevisionConflict { .. })
    ));
    assert_eq!(store.project(project.id).unwrap().project_revision, rev);
    store
        .fail_proposal_apply(
            project.id,
            rev,
            attempt.proposal_id,
            attempt.event_id,
            false,
            "writer",
            "Stopped before writing",
        )
        .unwrap();
    let rev = store.project(project.id).unwrap().project_revision;
    let mut reused = second_plan.clone();
    reused.id = attempt.plan.id;
    assert!(matches!(
        store.begin_proposal_apply(
            project.id,
            rev,
            second.parse().unwrap(),
            reused,
            "writer",
            "Reject duplicate write plan"
        ),
        Err(Error::InvalidInput(_))
    ));
    let mut wrong_base = second_plan.clone();
    wrong_base.before_fingerprint = wrong_base.after_fingerprint.clone();
    assert!(
        store
            .begin_proposal_apply(
                project.id,
                rev,
                second.parse().unwrap(),
                wrong_base,
                "writer",
                "Wrong baseline"
            )
            .is_err()
    );
    store
        .begin_proposal_apply(
            project.id,
            rev,
            second.parse().unwrap(),
            second_plan,
            "writer",
            "Reservation released",
        )
        .unwrap();
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);
}

#[test]
fn event_failures_do_not_fake_application_and_written_sources_can_be_recovered() {
    for event_type in [
        "proposal.apply_started",
        "source.projected",
        "proposal.applied",
    ] {
        let f = Fixture::new();
        let id = f.approved();
        let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
        db.execute_batch(&format!("CREATE TRIGGER reject_apply_event BEFORE INSERT ON events WHEN NEW.event_type='{event_type}' BEGIN SELECT RAISE(ABORT,'injected application event failure'); END;")).unwrap();
        let result = f.review("apply", &id);
        if event_type == "proposal.apply_started" {
            f.error(&result, "Storage");
            assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);
            assert!(f.ok(&["proposal", "show", &id])["apply_attempt"].is_null());
        } else {
            f.error(&result, "MutationIncomplete");
            let report: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(report["proposal"]["status"], "approved");
            assert_eq!(report["source_write_performed"], true);
            assert_eq!(report["write_outcome"], "pending_recovery");
            let recovery = f.0.join(report["recovery_directory"].as_str().unwrap());
            assert_eq!(
                fs::read_to_string(recovery.join("before.yaml")).unwrap(),
                WORK
            );
            assert_eq!(
                fs::read(recovery.join("after.yaml")).unwrap(),
                fs::read(f.0.join("work.yaml")).unwrap()
            );
            assert!(report["apply_attempt"]["resolved_event_id"].is_null());
        }
        db.execute_batch("DROP TRIGGER reject_apply_event").unwrap();
        let recovered = f.reviewed(
            if event_type == "proposal.apply_started" {
                "apply"
            } else {
                "recover"
            },
            &id,
        );
        assert_eq!(recovered["proposal"]["status"], "applied");
        assert_eq!(
            f.ok(&["work", "show", "W"])["work"]["next_action"],
            "Read the revised report"
        );
    }
}

#[test]
fn a_held_writer_lock_or_changed_source_cannot_be_overwritten() {
    let f = Fixture::new();
    let id = f.approved();
    let source = f.ok(&["proposal", "show", &id])["proposal"]["source_id"]
        .as_str()
        .unwrap()
        .to_owned();
    fs::create_dir_all(f.0.join(".awr/mutations")).unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(f.0.join(format!(".awr/mutations/{source}.lock")))
        .unwrap();
    lock.try_lock().unwrap();
    let revision = f.revision();
    f.error(&f.review("apply", &id), "MutationConflict");
    assert_eq!(f.revision(), revision);
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);
    drop(lock);
    fs::write(
        f.0.join("work.yaml"),
        format!("{WORK}# Preserve this new note\n"),
    )
    .unwrap();
    let before = fs::read(f.0.join("work.yaml")).unwrap();
    let result = f.review("apply", &id);
    f.error(&result, "SourceConflict");
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["proposal"]["status"], "conflict");
    assert_eq!(report["source_write_performed"], false);
    assert!(report["apply_attempt"].is_null());
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
}

#[test]
fn guarded_fields_invalid_values_and_read_only_files_have_explicit_outcomes() {
    for (patch, code, status) in [
        (r#"{"status":"completed"}"#, "proposal_required", "approved"),
        (r#"{"next_action":123}"#, "InvalidInput", "failed"),
        (r#"{"next_action":"New"}"#, "SourceUnavailable", "failed"),
    ] {
        let f = Fixture::new();
        let value = f.ok(&[
            "proposal",
            "create",
            "--kind",
            "work",
            "--target",
            "W",
            "--intent",
            "Review a field change",
            "--patch",
            patch,
            "--expected-revision",
            &f.revision(),
        ]);
        let id = value["proposal"]["id"].as_str().unwrap();
        f.reviewed("submit", id);
        f.reviewed("approve", id);
        let file = f.0.join("work.yaml");
        let original = fs::metadata(&file).unwrap().permissions();
        if code == "SourceUnavailable" {
            let mut permissions = original.clone();
            permissions.set_readonly(true);
            fs::set_permissions(&file, permissions).unwrap();
        }
        let result = f.review("apply", id);
        f.error(&result, code);
        fs::set_permissions(&file, original).unwrap();
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["proposal"]["status"], status);
        assert_eq!(report["source_write_performed"], false);
        assert!(report["apply_attempt"].is_null());
        assert_eq!(fs::read_to_string(file).unwrap(), WORK);
    }
}

#[test]
fn yaml_goals_plans_and_source_evidence_apply_with_verified_projection_receipts() {
    let f = Fixture::new();
    let work=WORK.replace("  acceptance: [Keep source authority]", "  acceptance: [Keep source authority]\n  evidence:\n  - locator: reports/draft.txt\n    summary: Source reference only");
    fs::write(f.0.join("work.yaml"),format!("{work}goals:\n- id: G\n  title: Goal\n  status: active\nmilestones:\n- id: M1\n  name: Delivery\n  status: active\n")).unwrap();
    f.ok(&["source", "reindex"]);
    for (kind, key, patch) in [
        ("goal", "G", r#"{"summary":"A clear project goal"}"#),
        ("plan", "M1", r#"{"name":"Report delivery"}"#),
        (
            "evidence",
            "W/evidence/reports/draft.txt",
            r#"{"summary":"A revised source reference"}"#,
        ),
    ] {
        let created = f.ok(&[
            "proposal",
            "create",
            "--kind",
            kind,
            "--target",
            key,
            "--intent",
            "Clarify a source record",
            "--patch",
            patch,
            "--expected-revision",
            &f.revision(),
        ]);
        let id = created["proposal"]["id"].as_str().unwrap();
        f.reviewed("submit", id);
        f.reviewed("approve", id);
        let applied = f.reviewed("apply", id);
        assert_eq!(applied["proposal"]["status"], "applied");
        assert_eq!(applied["source_write_performed"], true);
        assert_eq!(applied["source_refresh_performed"], true);
        assert_eq!(applied["proposal"]["patch"], created["proposal"]["patch"]);
        assert_eq!(
            applied["event"]["payload"]["target_after_hash"],
            applied["apply_attempt"]["plan"]["target_after_hash"]
        );
        let store = Store::open_readonly(&f.0.join(".awr/state.db")).unwrap();
        let project = store
            .project_by_root(&f.0.canonicalize().unwrap())
            .unwrap()
            .id;
        let kind_value = match kind {
            "goal" => EntityKind::Goal,
            "plan" => EntityKind::Plan,
            _ => EntityKind::Evidence,
        };
        let target = store.mutation_target(project, kind_value, key).unwrap();
        let object = json!({"object":target.item,"source":target.source});
        assert_eq!(
            object["object"]["id"],
            created["proposal"]["patch"]["target"]["meta"]["id"]
        );
        assert_eq!(
            object["source"]["fingerprint"],
            applied["apply_attempt"]["plan"]["after_fingerprint"]
        );
        if kind == "evidence" {
            assert_eq!(object["object"]["level"], "unknown");
            assert!(object["object"]["verified_at"].is_null());
        }
    }
}
