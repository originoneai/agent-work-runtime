use awr_core::*;
use awr_store::Store;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

const WORK: &str = "work_items:\n- id: W\n  title: Deliver the report\n  status: ready\n  owner: business-coordinator\n  next_action: Draft the report\n  depends_on: [D]\n  acceptance: [Review the report]\n- id: OTHER\n  status: ready\n  next_action: Separate work\n";
const DEPS: &str = "work_items:\n- id: D\n  status: completed\n  next_action: Delivered\n";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let f = Self(std::env::temp_dir().join(format!("awr-work-action-{}", Id::new())));
        fs::create_dir(&f.0).unwrap();
        fs::write(f.0.join("work.yaml"), WORK).unwrap();
        fs::write(f.0.join("deps.yaml"), DEPS).unwrap();
        fs::write(f.0.join("sources.toml"),"[project]\nname='Work actions'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='ledger'\nrole='supporting'\npath='deps.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
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
    fn error(&self, r: &Output, code: &str) {
        assert!(
            !r.status.success(),
            "{}",
            String::from_utf8_lossy(&r.stdout)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&r.stderr).unwrap()["code"],
            code,
            "{}",
            String::from_utf8_lossy(&r.stdout)
        );
    }
    fn revision(&self) -> String {
        self.ok(&["proposal", "list"])["project_revision"].to_string()
    }
    fn session(&self, key: &str, agent: &str, claim: bool) -> Value {
        let revision = self.revision();
        let mut args = vec![
            "session",
            "start",
            "--work",
            key,
            "--agent",
            agent,
            "--provider",
            "fixture",
            "--model",
            "local",
            "--expected-revision",
            &revision,
        ];
        if claim {
            args.push("--claim");
        }
        self.ok(&args)
    }
    fn action(&self, action: &str, session: &str, extra: &[&str]) -> Output {
        let revision = self.revision();
        let mut args = vec![
            "work",
            action,
            "W",
            "--session",
            session,
            "--reason",
            "Update work after reviewing current facts",
            "--expected-revision",
            &revision,
        ];
        args.extend_from_slice(extra);
        self.run(&args)
    }
    fn acted(&self, action: &str, session: &str, extra: &[&str]) -> Value {
        let r = self.action(action, session, extra);
        assert!(
            r.status.success(),
            "{action}: {} {}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn work(&self) -> Value {
        self.ok(&["object", "show", "work", "W", "--cached", "--full"])["object"].clone()
    }
    fn claim(&self, session: &str, id: &str) -> Value {
        self.ok(&["session", "show", session])["claims"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()
            .clone()
    }
    fn store(&self) -> (Store, Id) {
        let s = Store::open_existing(&self.0.join(".awr/state.db")).unwrap();
        let id = s
            .project_by_root(&self.0.canonicalize().unwrap())
            .unwrap()
            .id;
        (s, id)
    }
    fn draft(&self, session: Option<Id>, action: WorkAction) -> MutationDraft {
        let (s, project) = self.store();
        let target = s
            .mutation_target(project, EntityKind::WorkItem, "W")
            .unwrap();
        let work: WorkItem = serde_json::from_value(target.item).unwrap();
        let input = WorkActionInput {
            action,
            reason: "Review a deterministic source state change".into(),
            next_action: Some("Review the report".into()),
            summary: None,
            blocker: None,
        };
        let (binding, changes) = input.plan(&work).unwrap();
        MutationDraft {
            source_id: target.source.id,
            base_fingerprint: target.source.fingerprint,
            mutation_type: action.mutation_type().into(),
            patch: MutationPatch {
                version: 1,
                target: MutationTarget {
                    kind: EntityKind::WorkItem,
                    meta: work.meta,
                },
                source_config: target.source.config,
                intent: input.reason,
                changes,
                work_action: Some(binding),
            },
            created_by_session: session,
        }
    }
    fn approved(&self, session: Id) -> Id {
        let draft = self.draft(Some(session), WorkAction::Progress);
        let (mut store, p) = self.store();
        let r = store.project(p).unwrap().project_revision;
        let (proposal, e) = store.create_proposal(p, r, draft).unwrap();
        let (_, e) = store
            .review_proposal(
                p,
                e.project_revision,
                proposal.id,
                ProposalAction::Submit,
                "worker-a",
                "Review source action",
            )
            .unwrap();
        store
            .review_proposal(
                p,
                e.project_revision,
                proposal.id,
                ProposalAction::Approve,
                "worker-a",
                "Approve source action",
            )
            .unwrap();
        proposal.id
    }
    fn proposal_action(&self, action: &str, id: Id) -> Output {
        self.run(&[
            "proposal",
            action,
            &id.to_string(),
            "--actor",
            "worker-a",
            "--reason",
            "Resume the reviewed source action",
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
fn source_lifecycle_records_verified_actions_and_keeps_business_owner_separate() {
    let f = Fixture::new();
    let started = f.session("W", "worker-a", true);
    let sid = started["session"]["id"].as_str().unwrap();
    let claim = started["claim"]["id"].as_str().unwrap();
    assert_eq!(f.work()["status"], "ready");
    let progress = f.acted(
        "progress",
        sid,
        &[
            "--next-action",
            "Review the draft",
            "--summary",
            "Draft produced",
        ],
    );
    assert_eq!(progress["proposal"]["status"], "applied");
    assert_eq!(progress["event"]["event_type"], "work.progressed");
    assert_eq!(progress["source_write_performed"], true);
    assert_eq!(f.work()["status"], "in_progress");
    assert_eq!(f.work()["summary"], "Draft produced");
    assert_eq!(f.work()["owner"], "business-coordinator");
    let blocked = f.acted("block", sid, &["--blocker", "Waiting for the source data"]);
    assert_eq!(blocked["event"]["event_type"], "work.blocked");
    assert_eq!(f.work()["blocker"], "Waiting for the source data");
    assert_eq!(f.claim(sid, claim)["status"], "active");
    let revision = f.revision();
    f.error(
        &f.action("progress", sid, &["--next-action", "Continue prematurely"]),
        "InvalidTransition",
    );
    assert_eq!(f.revision(), revision);
    let unblocked = f.acted(
        "unblock",
        sid,
        &["--next-action", "Review the restored source data"],
    );
    assert_eq!(unblocked["event"]["event_type"], "work.unblocked");
    assert!(f.work()["blocker"].is_null());
    let cancelled = f.acted("cancel", sid, &[]);
    assert_eq!(cancelled["event"]["event_type"], "work.cancelled");
    assert_eq!(
        cancelled["event"]["payload"]["released_claim_ids"],
        json!([claim])
    );
    assert_eq!(f.claim(sid, claim)["status"], "released");
    assert_eq!(f.work()["status"], "cancelled");
    f.error(
        &f.action("progress", sid, &["--next-action", "Bypass reopening"]),
        "InvalidTransition",
    );
    let reopened = f.acted("reopen", sid, &["--next-action", "Replan the report"]);
    assert_eq!(reopened["event"]["event_type"], "work.reopened");
    assert_eq!(
        reopened["event"]["payload"]["work_action"]["from"],
        "cancelled"
    );
    assert_eq!(reopened["event"]["payload"]["work_action"]["to"], "planned");
    assert!(
        !reopened["event"]["payload"]["action_reason"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(f.work()["status"], "planned");
    assert_eq!(f.work()["owner"], "business-coordinator");
    f.ok(&[
        "work",
        "claim",
        "W",
        "--session",
        sid,
        "--expected-revision",
        &f.revision(),
    ]);
    f.acted(
        "progress",
        sid,
        &["--next-action", "Execute the revised plan"],
    );
    assert_eq!(f.work()["status"], "in_progress");
}

#[test]
fn completed_work_requires_explicit_reopen_reason_and_revision() {
    let f = Fixture::new();
    fs::write(
        f.0.join("work.yaml"),
        WORK.replacen("status: ready", "status: completed", 1),
    )
    .unwrap();
    f.ok(&["source", "reindex"]);
    let started = f.session("W", "worker-a", false);
    let sid = started["session"]["id"].as_str().unwrap();
    let before = fs::read(f.0.join("work.yaml")).unwrap();
    let revision = f.revision();
    f.error(
        &f.action(
            "progress",
            sid,
            &["--next-action", "Continue without reopening"],
        ),
        "InvalidTransition",
    );
    f.error(
        &f.run(&[
            "work",
            "reopen",
            "W",
            "--session",
            sid,
            "--reason",
            " ",
            "--next-action",
            "Review a new requirement",
            "--expected-revision",
            &revision,
        ]),
        "InvalidInput",
    );
    f.error(
        &f.run(&[
            "work",
            "reopen",
            "W",
            "--session",
            sid,
            "--reason",
            "New requirement",
            "--next-action",
            "Review it",
            "--expected-revision",
            "0",
        ]),
        "RevisionConflict",
    );
    assert_eq!(f.revision(), revision);
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
    let report = f.acted(
        "reopen",
        sid,
        &["--next-action", "Assess the changed requirement"],
    );
    assert_eq!(
        report["event"]["payload"]["work_action"]["from"],
        "completed"
    );
    assert_eq!(f.work()["status"], "planned");
    f.error(
        &f.action("reopen", sid, &["--next-action", "Reopen twice"]),
        "InvalidTransition",
    );
}

#[test]
fn claims_sessions_and_expiry_are_checked_before_creating_source_proposals() {
    let f = Fixture::new();
    let a = f.session("W", "worker-a", false);
    let sid = a["session"]["id"].as_str().unwrap();
    let revision = f.revision();
    f.error(
        &f.action("progress", sid, &["--next-action", "No active claim"]),
        "ClaimConflict",
    );
    assert_eq!(f.revision(), revision);
    let own = f.ok(&[
        "work",
        "claim",
        "W",
        "--session",
        sid,
        "--expected-revision",
        &f.revision(),
    ]);
    let other = f.session("OTHER", "worker-other", true);
    f.error(
        &f.action(
            "progress",
            other["session"]["id"].as_str().unwrap(),
            &["--next-action", "Wrong work"],
        ),
        "InvalidInput",
    );
    let foreign = f.session("W", "worker-b", false);
    let foreign = foreign["session"]["id"].as_str().unwrap();
    f.error(&f.action("cancel", foreign, &[]), "ClaimConflict");
    f.ok(&[
        "work",
        "release",
        "W",
        "--session",
        sid,
        "--claim",
        own["claim"]["id"].as_str().unwrap(),
        "--expected-revision",
        &f.revision(),
    ]);
    f.error(
        &f.action("progress", sid, &["--next-action", "Released claim"]),
        "ClaimConflict",
    );
    let own = f.ok(&[
        "work",
        "claim",
        "W",
        "--session",
        sid,
        "--expected-revision",
        &f.revision(),
    ]);
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    db.execute(
        "UPDATE claims SET expires_at=0 WHERE id=?1",
        [own["claim"]["id"].as_str().unwrap()],
    )
    .unwrap();
    f.error(
        &f.action("progress", sid, &["--next-action", "Expired claim"]),
        "ClaimConflict",
    );
    assert_eq!(f.work()["status"], "ready");
    assert_eq!(f.ok(&["proposal", "list"])["proposals"], json!([]));
}

#[test]
fn actual_dependency_drift_blocks_progress_but_does_not_prevent_cancellation() {
    let f = Fixture::new();
    let started = f.session("W", "worker-a", true);
    let sid = started["session"]["id"].as_str().unwrap();
    let before = fs::read(f.0.join("work.yaml")).unwrap();
    let rev = f.revision();
    fs::write(f.0.join("deps.yaml"), DEPS.replace("completed", "blocked")).unwrap();
    f.error(
        &f.action(
            "progress",
            sid,
            &["--next-action", "Continue with stale prerequisites"],
        ),
        "SourceConflict",
    );
    assert_eq!(f.revision(), rev);
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
    f.ok(&["source", "reindex"]);
    f.error(
        &f.action(
            "progress",
            sid,
            &["--next-action", "Continue with blocked prerequisites"],
        ),
        "DependencyBlocked",
    );
    f.acted("cancel", sid, &[]);
    assert_eq!(f.work()["status"], "cancelled");
    f.acted(
        "reopen",
        sid,
        &["--next-action", "Replan after dependency repair"],
    );
    assert_eq!(f.work()["status"], "planned");
}

#[test]
fn unblocking_can_recover_work_after_its_previous_claim_was_released() {
    let f = Fixture::new();
    fs::write(
        f.0.join("work.yaml"),
        WORK.replacen(
            "status: ready",
            "status: blocked\n  blocker: Waiting for data",
            1,
        ),
    )
    .unwrap();
    f.ok(&["source", "reindex"]);
    let started = f.session("W", "worker-a", false);
    let sid = started["session"]["id"].as_str().unwrap();
    f.acted(
        "unblock",
        sid,
        &["--next-action", "Check the newly supplied data"],
    );
    assert_eq!(f.work()["status"], "in_progress");
    assert!(f.work()["blocker"].is_null());
    f.error(
        &f.action(
            "progress",
            sid,
            &["--next-action", "Continue before claiming"],
        ),
        "ClaimConflict",
    );
    f.ok(&[
        "work",
        "claim",
        "W",
        "--session",
        sid,
        "--expected-revision",
        &f.revision(),
    ]);
    f.acted("progress", sid, &["--next-action", "Produce the report"]);
}

#[test]
fn domain_labels_cannot_forge_status_ownership_or_verification_changes() {
    let f = Fixture::new();
    let session = f.session("W", "worker-a", true);
    let sid: Id = session["session"]["id"].as_str().unwrap().parse().unwrap();
    for change in [
        json!({"status":"completed","next_action":"Fake completion"}),
        json!({"status":"in_progress","next_action":"Change owner","owner":"worker-a"}),
        json!({"status":"in_progress","next_action":"Promote evidence","verification":{"evidence_level":"released"}}),
    ] {
        let mut draft = f.draft(Some(sid), WorkAction::Progress);
        draft.patch.changes = change;
        let (mut store, p) = f.store();
        let rev = store.project(p).unwrap().project_revision;
        assert!(matches!(
            store.create_proposal(p, rev, draft),
            Err(Error::InvalidInput(_))
        ));
        assert_eq!(store.project(p).unwrap().project_revision, rev);
    }
    let mut mismatch = f.draft(Some(sid), WorkAction::Progress);
    mismatch.mutation_type = "update_fields".into();
    let no_session = f.draft(None, WorkAction::Progress);
    let (mut store, p) = f.store();
    let rev = store.project(p).unwrap().project_revision;
    assert!(matches!(
        store.create_proposal(p, rev, mismatch),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        store.create_proposal(p, rev, no_session),
        Err(Error::InvalidInput(_))
    ));
    for kind in [
        "work.progressed",
        "work.blocked",
        "work.unblocked",
        "work.cancelled",
        "work.reopened",
        "work.completed",
    ] {
        assert!(matches!(
            store.append_event(p, rev, EventDraft::new(kind, "Forged receipt")),
            Err(Error::InvalidInput(_))
        ));
    }
    assert_eq!(store.project(p).unwrap().project_revision, rev);
    assert_eq!(f.work()["status"], "ready");
}

#[test]
fn cancellation_receipt_failure_rolls_back_claim_release_and_recovery_finalizes_once() {
    let f = Fixture::new();
    let s = f.session("W", "worker-a", true);
    let sid = s["session"]["id"].as_str().unwrap();
    let claim = s["claim"]["id"].as_str().unwrap();
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_work_event BEFORE INSERT ON events WHEN NEW.event_type='work.cancelled' BEGIN SELECT RAISE(ABORT,'injected work receipt failure'); END;").unwrap();
    let failed = f.action("cancel", sid, &[]);
    f.error(&failed, "MutationIncomplete");
    let failed: Value = serde_json::from_slice(&failed.stdout).unwrap();
    let id: Id = failed["proposal"]["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(failed["proposal"]["status"], "approved");
    assert_eq!(failed["source_write_performed"], true);
    assert_eq!(f.work()["status"], "cancelled");
    assert_eq!(f.claim(sid, claim)["status"], "active");
    db.execute_batch("DROP TRIGGER reject_work_event").unwrap();
    let recovered = f.proposal_action("recover", id);
    assert!(
        recovered.status.success(),
        "{} {}",
        String::from_utf8_lossy(&recovered.stdout),
        String::from_utf8_lossy(&recovered.stderr)
    );
    let recovered: Value = serde_json::from_slice(&recovered.stdout).unwrap();
    assert_eq!(recovered["proposal"]["status"], "applied");
    assert_eq!(recovered["event"]["event_type"], "work.cancelled");
    assert_eq!(recovered["source_write_performed"], false);
    assert_eq!(
        recovered["apply_attempt"]["resolved_event_id"],
        recovered["event"]["id"]
    );
    assert_eq!(f.claim(sid, claim)["status"], "released");
    f.error(&f.proposal_action("recover", id), "InvalidTransition");
    assert_eq!(
        f.ok(&["work", "history", "W", "--event-type", "work.cancelled"])["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn applying_reviewed_domain_proposals_rechecks_claims_and_actual_dependency_bytes() {
    for drift in [false, true] {
        let f = Fixture::new();
        let s = f.session("W", "worker-a", true);
        let sid = s["session"]["id"].as_str().unwrap();
        let id = f.approved(sid.parse().unwrap());
        let before = fs::read(f.0.join("work.yaml")).unwrap();
        if drift {
            fs::write(f.0.join("deps.yaml"), DEPS.replace("completed", "blocked")).unwrap();
        } else {
            f.ok(&[
                "work",
                "release",
                "W",
                "--session",
                sid,
                "--claim",
                s["claim"]["id"].as_str().unwrap(),
                "--expected-revision",
                &f.revision(),
            ]);
        }
        f.error(
            &f.proposal_action("apply", id),
            if drift {
                "SourceConflict"
            } else {
                "ClaimConflict"
            },
        );
        assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
        assert_eq!(
            f.ok(&["proposal", "show", &id.to_string()])["proposal"]["status"],
            if drift { "conflict" } else { "approved" }
        );
        if !drift {
            f.ok(&[
                "work",
                "claim",
                "W",
                "--session",
                sid,
                "--expected-revision",
                &f.revision(),
            ]);
            let r = f.proposal_action("apply", id);
            assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
            assert_eq!(f.work()["status"], "in_progress");
        }
    }
}

#[test]
fn checkpoint_handoff_transfers_execution_while_source_ownership_is_preserved() {
    let f = Fixture::new();
    let a = f.session("W", "worker-a", true);
    let a = a["session"]["id"].as_str().unwrap();
    f.acted("progress", a, &["--next-action", "Review the report draft"]);
    let b = f.session("W", "worker-b", false);
    let b = b["session"]["id"].as_str().unwrap();
    f.ok(&[
        "session",
        "checkpoint",
        "--session",
        a,
        "--context-hash",
        &"a".repeat(64),
        "--digest",
        "Draft is ready for review",
        "--next-action",
        "Review the draft",
        "--expected-revision",
        &f.revision(),
    ]);
    let before = fs::read(f.0.join("work.yaml")).unwrap();
    let handed = f.ok(&[
        "work",
        "handoff",
        "W",
        "--session",
        a,
        "--to-session",
        b,
        "--expected-revision",
        &f.revision(),
    ]);
    assert_eq!(handed["handoff"]["transferred_claim"]["session_id"], b);
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
    let result = f.acted("progress", b, &["--next-action", "Address review comments"]);
    assert_eq!(result["event"]["payload"]["actor"], "worker-b");
    assert_eq!(f.work()["owner"], "business-coordinator");
    f.error(
        &f.action("block", a, &["--blocker", "Old executor must not mutate"]),
        "InvalidTransition",
    );
}

#[test]
fn an_intermediate_review_failure_retains_a_resumable_domain_proposal() {
    let f = Fixture::new();
    let s = f.session("W", "worker-a", true);
    let sid = s["session"]["id"].as_str().unwrap();
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_review BEFORE INSERT ON events WHEN NEW.event_type='proposal.approved' BEGIN SELECT RAISE(ABORT,'injected review failure'); END;").unwrap();
    let result = f.action(
        "progress",
        sid,
        &["--next-action", "Review the retained draft"],
    );
    f.error(&result, "WorkActionIncomplete");
    let error: Value = serde_json::from_slice(&result.stderr).unwrap();
    let id: Id = error["details"]["proposal_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(error["details"]["stage"], "approve");
    assert_eq!(
        f.ok(&["proposal", "show", &id.to_string()])["proposal"]["status"],
        "ready"
    );
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);
    db.execute_batch("DROP TRIGGER reject_review").unwrap();
    let approved = f.proposal_action("approve", id);
    assert!(
        approved.status.success(),
        "{}",
        String::from_utf8_lossy(&approved.stderr)
    );
    let applied = f.proposal_action("apply", id);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied: Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(applied["proposal"]["status"], "applied");
    assert_eq!(applied["event"]["event_type"], "work.progressed");
    assert_eq!(f.work()["next_action"], "Review the retained draft");
}
