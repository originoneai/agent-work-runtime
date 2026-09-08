use awr_core::*;
use awr_store::Store;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

const WORK: &str = "work_items:\n- id: W\n  title: Deliver merged work\n  status: in_progress\n  next_action: Review the branch result\n  acceptance: [Retain delivery provenance]\n- id: NEXT\n  title: Follow up the remaining input\n  status: ready\n  next_action: Read the carried observation\n  acceptance: [Deliver the follow-up]\n";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let f = Self(std::env::temp_dir().join(format!("awr-close-{}", Id::new())));
        fs::create_dir_all(&f.0).unwrap();
        fs::write(f.0.join("work.yaml"), WORK).unwrap();
        fs::write(
            f.0.join("rules.md"),
            "# Authority {severity=hard scope=project value=*}\n\nPreserve current source facts.\n",
        )
        .unwrap();
        fs::write(
            f.0.join("goal.md"),
            "# Deliver durable work\n\nRetain completed work and its history.\n",
        )
        .unwrap();
        fs::write(f.0.join("sources.toml"),"[project]\nname='Branch closure'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='goal.md'\nadapter='markdown-heading-v1'\n[sources.options]\nstatus='active'\n").unwrap();
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
        Self::success(self.run(args))
    }
    fn success(r: Output) -> Value {
        assert!(
            r.status.success(),
            "{} {}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn error(r: Output, code: &str) {
        assert!(!r.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&r.stderr).unwrap()["code"],
            code,
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
    }
    fn rev(&self) -> String {
        self.ok(&["branch", "list"])["project_revision"].to_string()
    }
    fn branch(&self, name: &str) -> Value {
        self.ok(&[
            "branch",
            "create",
            name,
            "--actor",
            "executor",
            "--reason",
            "Develop the requested branch",
            "--expected-revision",
            &self.rev(),
        ])
    }
    fn switch(&self, name: &str) {
        self.ok(&[
            "branch",
            "switch",
            name,
            "--actor",
            "executor",
            "--reason",
            "Continue the branch",
            "--expected-revision",
            &self.rev(),
        ]);
    }
    fn start(&self) -> Value {
        self.ok(&[
            "session",
            "start",
            "--work",
            "W",
            "--agent",
            "executor",
            "--provider",
            "fixture",
            "--model",
            "local",
            "--claim",
            "--expected-revision",
            &self.rev(),
        ])
    }
    fn end(&self, sid: &str) {
        self.ok(&[
            "session",
            "end",
            "--session",
            sid,
            "--expected-revision",
            &self.rev(),
        ]);
    }
    fn checkpoint(&self, sid: &str, loops: &[&str]) -> Value {
        let hash = "a".repeat(64);
        let rev = self.rev();
        let mut args = vec![
            "session",
            "checkpoint",
            "--session",
            sid,
            "--context-hash",
            &hash,
            "--digest",
            "Recorded the branch findings",
            "--next-action",
            "Review the retained observations",
            "--expected-revision",
            &rev,
        ];
        for text in loops {
            args.extend(["--open-loop", text]);
        }
        self.ok(&args)
    }
    fn source_input(&self) -> Value {
        let path = self.0.join("merge-report.txt");
        fs::write(
            &path,
            "Source result reconciled; retain this concrete merge report.\n",
        )
        .unwrap();
        let snapshot = awr_source::Locator::File(path)
            .read(&self.0, 1048576)
            .unwrap();
        json!({"version":1,"outcome":"merged","summary":"Source results are reconciled","merge":{"kind":"source","locator":"merge-report.txt","sha256":snapshot.fingerprint.strip_prefix("sha256:").unwrap()},"open_loops":[]})
    }
    fn close_at(&self, name: &str, input: &Value, rev: &str) -> Output {
        let path = self.0.join("close-input.json");
        fs::write(&path, serde_json::to_vec(input).unwrap()).unwrap();
        self.run(&[
            "branch",
            "close",
            name,
            "--input",
            path.to_str().unwrap(),
            "--actor",
            "executor",
            "--reason",
            "Finish the reviewed branch lifecycle",
            "--expected-revision",
            rev,
        ])
    }
    fn close(&self, name: &str, input: &Value) -> Output {
        self.close_at(name, input, &self.rev())
    }
    fn plan(&self, name: &str) -> Value {
        self.ok(&["branch", "show", name, "--close-plan"])
    }
    fn store(&self) -> (Store, Id) {
        let s = Store::open_existing(&self.0.join(".awr/state.db")).unwrap();
        let p = s.project_by_root(&self.0).unwrap().id;
        (s, p)
    }
    fn git(&self, args: &[&str]) -> String {
        let r = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        String::from_utf8(r.stdout).unwrap().trim().into()
    }
    fn init_git(&self) {
        self.git(&["init", "-b", "main"]);
        self.git(&["config", "user.name", "Fixture"]);
        self.git(&["config", "user.email", "fixture@example.invalid"]);
        self.git(&["config", "commit.gpgsign", "false"]);
        self.git(&["add", "work.yaml", "rules.md", "goal.md", "sources.toml"]);
        self.git(&["commit", "-m", "Initial fixture"]);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn stale_checkpoint_pointer_never_hides_a_saved_open_loop() {
    let f = Fixture::new();
    f.branch("pointer");
    f.switch("pointer");
    let session = f.start();
    let sid = session["session"]["id"].as_str().unwrap();
    let old = f.checkpoint(sid, &[]);
    let latest = f.checkpoint(sid, &["Saved observation must not disappear"]);
    f.end(sid);
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    db.execute(
        "UPDATE sessions SET last_checkpoint_id=?1 WHERE id=?2",
        [old["checkpoint"]["id"].as_str().unwrap(), sid],
    )
    .unwrap();
    let input = f.source_input();
    Fixture::error(f.close("pointer", &input), "SourceConflict");
    db.execute(
        "UPDATE sessions SET last_checkpoint_id=?1 WHERE id=?2",
        [latest["checkpoint"]["id"].as_str().unwrap(), sid],
    )
    .unwrap();
    assert_eq!(
        f.plan("pointer")["open_loops"][0]["text"],
        "Saved observation must not disappear"
    );
}

#[test]
fn outstanding_source_proposals_must_be_settled_and_are_not_silently_discarded() {
    let f = Fixture::new();
    f.branch("proposal-work");
    f.switch("proposal-work");
    let started = f.start();
    let sid = started["session"]["id"].as_str().unwrap();
    let (mut store, p) = f.store();
    let target = store.mutation_target(p, EntityKind::WorkItem, "W").unwrap();
    let work: WorkItem = serde_json::from_value(target.item).unwrap();
    let input = WorkActionInput {
        action: WorkAction::Progress,
        reason: "Review the source progress proposal".into(),
        next_action: Some("Inspect the drafted follow-up".into()),
        summary: None,
        blocker: None,
    };
    let (binding, changes) = input.plan(&work).unwrap();
    let revision = store.project(p).unwrap().project_revision;
    let (proposal, _) = store
        .create_proposal(
            p,
            revision,
            MutationDraft {
                source_id: target.source.id,
                base_fingerprint: target.source.fingerprint,
                mutation_type: WorkAction::Progress.mutation_type().into(),
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
                created_by_session: Some(sid.parse().unwrap()),
            },
        )
        .unwrap();
    drop(store);
    f.end(sid);
    assert_eq!(
        f.plan("proposal-work")["pending_proposals"],
        json!([proposal.id])
    );
    let close = f.source_input();
    Fixture::error(f.close("proposal-work", &close), "InvalidTransition");
    let (mut store, p) = f.store();
    let revision = store.project(p).unwrap().project_revision;
    store
        .review_proposal(
            p,
            revision,
            proposal.id,
            ProposalAction::Reject,
            "executor",
            "Superseded proposal explicitly reviewed and rejected",
        )
        .unwrap();
    drop(store);
    let before = fs::read(f.0.join("work.yaml")).unwrap();
    Fixture::success(f.close("proposal-work", &close));
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
    let (store, p) = f.store();
    assert_eq!(
        store.proposal(p, proposal.id).unwrap().status,
        ProposalStatus::Rejected
    );
}

#[test]
fn explicit_work_branch_destination_receives_the_summary_and_never_moves_other_runtime() {
    let f = Fixture::new();
    let target = f.branch("target");
    f.branch("source");
    f.switch("target");
    let session = f.start();
    let sid = session["session"]["id"].as_str().unwrap();
    f.switch("source");
    let input = f.source_input();
    let path = f.0.join("close-input.json");
    fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let close = |into: &str| {
        f.run(&[
            "branch",
            "close",
            "source",
            "--into",
            into,
            "--input",
            path.to_str().unwrap(),
            "--actor",
            "executor",
            "--reason",
            "Record delivery into the target work branch",
            "--expected-revision",
            &f.rev(),
        ])
    };
    Fixture::error(close("source"), "InvalidInput");
    Fixture::error(close("absent"), "NotFound");
    let result = Fixture::success(close("target"));
    assert_eq!(
        result["receipt"]["target_branch_id"],
        target["branch"]["id"]
    );
    assert_eq!(
        f.ok(&["branch", "show"])["branch_id"],
        target["branch"]["id"]
    );
    let retained = f.ok(&["session", "show", sid]);
    assert_eq!(retained["session"], session["session"]);
    assert_eq!(retained["claims"], json!([session["claim"].clone()]));
    let (store, p) = f.store();
    let event = store
        .event(
            p,
            result["receipt"]["summary_event_id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        event.branch_id,
        Some(target["branch"]["id"].as_str().unwrap().parse().unwrap())
    );
}

#[test]
fn external_git_merge_reindexes_then_closes_with_current_versions_and_retained_main_work() {
    let f = Fixture::new();
    f.init_git();
    let main = f.start();
    let ms = main["session"]["id"].as_str().unwrap();
    let created = f.branch("delivery");
    let bid = created["branch"]["id"].as_str().unwrap();
    f.switch("delivery");
    f.git(&["checkout", "-b", "feature"]);
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace(
            "Review the branch result",
            "Use the externally merged source action",
        ),
    )
    .unwrap();
    f.git(&["add", "work.yaml"]);
    f.git(&["commit", "-m", "Branch source result"]);
    let tip = f.git(&["rev-parse", "HEAD"]);
    f.git(&["checkout", "main"]);
    f.git(&["merge", "--no-ff", "feature", "-m", "Merge reviewed result"]);
    let head = f.git(&["rev-parse", "HEAD"]);
    let input = json!({"version":1,"outcome":"merged","summary":"External Git branch merged and sources reviewed","merge":{"kind":"git","source_ref":"feature","target_ref":"main"},"open_loops":[]});
    let old = f.rev();
    Fixture::error(f.close_at("delivery", &input, &old), "RevisionConflict");
    assert_ne!(old, f.rev());
    assert_eq!(
        f.ok(&["branch", "show", "delivery"])["record"]["branch"]["status"],
        "active"
    );
    let expected = f.rev();
    let before = fs::read(f.0.join("work.yaml")).unwrap();
    let result = Fixture::success(f.close_at("delivery", &input, &expected));
    assert_eq!(result["branch"]["status"], "merged");
    assert_eq!(result["branch"]["revision"], 2);
    assert_eq!(
        result["receipt"]["source_project_revision"].to_string(),
        expected
    );
    assert_eq!(result["receipt"]["merge"]["source"]["commit_sha"], tip);
    assert_eq!(result["receipt"]["merge"]["target"]["commit_sha"], head);
    assert!(result["receipt"]["current_branch_id"].is_null());
    assert_eq!(result["receipt"]["previous_branch_id"], bid);
    assert!(f.ok(&["branch", "show"])["branch_id"].is_null());
    assert_eq!(f.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(f.git(&["symbolic-ref", "HEAD"]), "refs/heads/main");
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
    let retained = f.ok(&["session", "show", ms]);
    assert_eq!(retained["session"], main["session"]);
    assert_eq!(retained["claims"], json!([main["claim"].clone()]));
    let (store, p) = f.store();
    let work = store.work_item(p, "W").unwrap();
    assert_eq!(work.item.status, WorkStatus::InProgress);
    assert!(work.item.next_action.contains("externally merged"));
    let receipt = &result["receipt"];
    let mut current = store
        .sources(p)
        .unwrap()
        .into_iter()
        .map(BranchSourceVersion::from)
        .collect::<Vec<_>>();
    current.sort_by_key(|s| s.source_id);
    assert_eq!(
        serde_json::to_value(current).unwrap(),
        receipt["source_versions"]
    );
    let summary = store
        .event(
            p,
            receipt["summary_event_id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(summary.event_type, "branch.merged");
    assert!(summary.branch_id.is_none());
    assert_eq!(summary.payload["closure"], *receipt);
    let shown = f.ok(&["branch", "show", "delivery"]);
    assert_eq!(
        shown["record"]["closure"]["event_id"],
        result["event"]["id"]
    );
    Fixture::error(f.close("delivery", &input), "InvalidTransition");
}

#[test]
fn every_latest_checkpoint_loop_requires_an_exact_resolution_or_traceable_carry_forward() {
    let f = Fixture::new();
    f.branch("review");
    f.switch("review");
    let session = f.start();
    let sid = session["session"]["id"].as_str().unwrap();
    let old = f.checkpoint(sid, &["Superseded loop"]);
    let cp = f.checkpoint(
        sid,
        &[
            "Reviewed input is resolved",
            "Follow up the missing business input",
        ],
    );
    f.end(sid);
    let plan = f.plan("review");
    assert_eq!(plan["open_loops"].as_array().unwrap().len(), 2);
    assert!(!plan.to_string().contains("Superseded loop"));
    assert!(plan["blockers"].as_array().unwrap().is_empty());
    let mut input = f.source_input();
    Fixture::error(f.close("review", &input), "InvalidInput");
    input["open_loops"] = json!([
        {"checkpoint_id":cp["checkpoint"]["id"],"index":0,"text":"Reviewed input is resolved","resolution":{"outcome":"resolved","reason":"Reviewed the recorded source result","references":["merge-report.txt"]}},
        {"checkpoint_id":cp["checkpoint"]["id"],"index":1,"text":"Follow up the missing business input","resolution":{"outcome":"carry_forward","reason":"Continue in the follow-up task","work_item_key":"NEXT"}}
    ]);
    let mut wrong = input.clone();
    wrong["open_loops"][0]["text"] = json!("Changed the observation");
    Fixture::error(f.close("review", &wrong), "SourceConflict");
    let mut duplicate = input.clone();
    duplicate["open_loops"]
        .as_array_mut()
        .unwrap()
        .push(input["open_loops"][0].clone());
    Fixture::error(f.close("review", &duplicate), "InvalidInput");
    let mut previous = input.clone();
    previous["open_loops"].as_array_mut().unwrap().push(json!({"checkpoint_id":old["checkpoint"]["id"],"index":0,"text":"Superseded loop","resolution":{"outcome":"resolved","reason":"Wrong window","references":["merge-report.txt"]}}));
    Fixture::error(f.close("review", &previous), "InvalidInput");
    let mut missing_target = input.clone();
    missing_target["open_loops"][1]["resolution"]["work_item_key"] = json!("ABSENT");
    Fixture::error(f.close("review", &missing_target), "NotFound");
    let before = fs::read(f.0.join("work.yaml")).unwrap();
    let result = Fixture::success(f.close("review", &input));
    assert_eq!(result["receipt"]["open_loops"].as_array().unwrap().len(), 2);
    let carried = &result["receipt"]["open_loops"][1];
    assert_eq!(carried["carried_to"]["external_key"], "NEXT");
    assert!(carried["carry_event_id"].is_string());
    let (store, p) = f.store();
    let event = store
        .event(
            p,
            carried["carry_event_id"].as_str().unwrap().parse().unwrap(),
        )
        .unwrap();
    assert_eq!(event.event_type, "branch.loop_carried");
    assert!(event.branch_id.is_none());
    assert_eq!(
        event.work_item_id,
        Some(store.work_item(p, "NEXT").unwrap().item.meta.id)
    );
    assert_eq!(
        store
            .checkpoint(p, cp["checkpoint"]["id"].as_str().unwrap().parse().unwrap())
            .unwrap()
            .open_loops,
        vec![
            "Reviewed input is resolved",
            "Follow up the missing business input"
        ]
    );
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), before);
    let context = f.ok(&["context", "compile", "--work", "NEXT"]);
    assert!(
        context
            .to_string()
            .contains("Follow up the missing business input")
    );
}

#[test]
fn active_sessions_claims_and_incomplete_saves_require_explicit_cleanup_before_closure() {
    let f = Fixture::new();
    let branch = f.branch("busy");
    f.switch("busy");
    let session = f.start();
    let sid = session["session"]["id"].as_str().unwrap();
    let input = f.source_input();
    let before = f.rev();
    Fixture::error(f.close("busy", &input), "InvalidTransition");
    assert_eq!(before, f.rev());
    let (mut store, p) = f.store();
    let attempt = store
        .begin_checkpoint_save(
            p,
            before.parse().unwrap(),
            sid.parse().unwrap(),
            CheckpointDraft {
                context_hash: "a".repeat(64),
                digest: "Unfinished persistence".into(),
                next_action: "Resolve the save".into(),
                open_loops: vec![],
                changed_entities: vec![],
            },
        )
        .unwrap();
    drop(store);
    f.end(sid);
    let plan = f.plan("busy");
    assert_eq!(plan["pending_checkpoint_attempts"], json!([attempt.id]));
    Fixture::error(f.close("busy", &input), "InvalidTransition");
    f.ok(&[
        "doctor",
        "repair",
        "abandon-checkpoint",
        &attempt.id.to_string(),
        "--reason",
        "Preserve the interrupted save and close its attempt",
        "--expected-revision",
        &f.rev(),
    ]);
    // Retained stale active-status claims still block, even when their TTL elapsed.
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    db.execute(
        "UPDATE claims SET status='active',released_at=NULL,expires_at=0 WHERE id=?1",
        [session["claim"]["id"].as_str().unwrap()],
    )
    .unwrap();
    assert_eq!(
        f.plan("busy")["unsettled_claims"].as_array().unwrap().len(),
        1
    );
    Fixture::error(f.close("busy", &input), "InvalidTransition");
    f.ok(&[
        "doctor",
        "repair",
        "expire-claim",
        session["claim"]["id"].as_str().unwrap(),
        "--reason",
        "Observed the elapsed lease",
        "--expected-revision",
        &f.rev(),
    ]);
    let result = Fixture::success(f.close("busy", &input));
    assert_eq!(result["branch"]["id"], branch["branch"]["id"]);
    assert!(f.ok(&["session", "show", sid])["session"]["status"] == "ended");
}

#[test]
fn git_ancestry_head_and_dirty_tree_failures_never_record_a_merge_or_close() {
    let f = Fixture::new();
    f.init_git();
    f.branch("unmerged");
    f.git(&["checkout", "-b", "feature"]);
    fs::write(f.0.join("result.txt"), "Feature result\n").unwrap();
    f.git(&["add", "result.txt"]);
    f.git(&["commit", "-m", "Unmerged branch work"]);
    f.git(&["checkout", "main"]);
    let mut input = json!({"version":1,"outcome":"merged","summary":"Await actual merge","merge":{"kind":"git","source_ref":"feature","target_ref":"main"},"open_loops":[]});
    let before = f.rev();
    Fixture::error(f.close("unmerged", &input), "SourceConflict");
    input["merge"]["source_ref"] = json!("main");
    input["merge"]["target_ref"] = json!("feature");
    Fixture::error(f.close("unmerged", &input), "SourceConflict");
    input["merge"]["target_ref"] = json!("main");
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace("Review the branch result", "Dirty tracked source"),
    )
    .unwrap();
    Fixture::error(f.close("unmerged", &input), "SourceConflict");
    assert_eq!(before, f.rev());
    assert_eq!(
        f.ok(&["branch", "show", "unmerged"])["record"]["branch"]["status"],
        "active"
    );
    input["merge"]["source_ref"] = json!("absent-ref");
    Fixture::error(f.close_at("unmerged", &input, "0"), "RevisionConflict");
}

#[test]
fn source_report_hash_missing_sources_and_main_closure_are_explicit_failures() {
    let f = Fixture::new();
    f.branch("source-only");
    let mut input = f.source_input();
    let before = f.rev();
    input["merge"]["sha256"] = json!("0".repeat(64));
    Fixture::error(f.close("source-only", &input), "SourceConflict");
    assert_eq!(before, f.rev());
    input = f.source_input();
    Fixture::error(f.close("main", &input), "InvalidInput");
    fs::remove_file(f.0.join("rules.md")).unwrap();
    Fixture::error(f.close("source-only", &input), "SourceStale");
    assert_eq!(
        f.ok(&["branch", "show", "source-only"])["record"]["branch"]["status"],
        "active"
    );
    let (store, p) = f.store();
    assert!(
        store
            .events_since(p, 0, 1000)
            .unwrap()
            .iter()
            .all(|e| e.event_type != "branch.merged" && e.event_type != "branch.closed")
    );
}

#[test]
fn summary_and_closure_are_atomic_and_cannot_be_forged_by_raw_events() {
    let f = Fixture::new();
    let created = f.branch("atomic");
    f.switch("atomic");
    let input = f.source_input();
    let before = f.rev();
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    for kind in ["branch.merged", "branch.closed"] {
        db.execute_batch(&format!("CREATE TRIGGER fail_close BEFORE INSERT ON events WHEN NEW.event_type='{kind}' BEGIN SELECT RAISE(ABORT,'injected close failure'); END;")).unwrap();
        assert!(!f.close("atomic", &input).status.success());
        assert_eq!(f.rev(), before);
        let shown = f.ok(&["branch", "show", "atomic"]);
        assert_eq!(shown["record"]["branch"]["status"], "active");
        assert_eq!(shown["current_branch_id"], created["branch"]["id"]);
        assert!(shown["record"].get("closure").is_none());
        db.execute_batch("DROP TRIGGER fail_close").unwrap();
    }
    let (mut store, p) = f.store();
    for kind in [
        "branch.merged",
        "branch.closed",
        "branch.loop_carried",
        "branch.abandoned",
    ] {
        assert!(
            store
                .append_event(
                    p,
                    before.parse().unwrap(),
                    EventDraft::new(kind, "Forged lifecycle receipt")
                )
                .is_err()
        );
    }
    drop(store);
    Fixture::success(f.close("atomic", &input));
}

#[test]
fn abandonment_and_retained_closed_history_do_not_depend_on_git_or_move_other_branches() {
    let f = Fixture::new();
    f.branch("abandoned");
    let other = f.branch("other");
    f.switch("other");
    let input = json!({"version":1,"outcome":"abandoned","summary":"The experiment was intentionally stopped; its history is retained","merge":null,"open_loops":[]});
    let result = Fixture::success(f.close("abandoned", &input));
    assert_eq!(result["branch"]["status"], "abandoned");
    assert_eq!(
        result["receipt"]["current_branch_id"],
        other["branch"]["id"]
    );
    assert!(result["receipt"]["merge"].is_null());
    fs::remove_file(f.0.join("work.yaml")).unwrap();
    fs::remove_file(f.0.join(".awr/project.toml")).unwrap();
    let shown = f.ok(&["branch", "show", "abandoned"]);
    assert_eq!(
        shown["record"]["closure"]["event_id"],
        result["event"]["id"]
    );
    assert_eq!(
        f.ok(&["branch", "list", "--status", "abandoned"])["total"],
        1
    );
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    db.execute(
        "UPDATE branches SET status='active' WHERE name='abandoned'",
        [],
    )
    .unwrap();
    Fixture::error(f.run(&["branch", "show", "abandoned"]), "SourceConflict");
}
