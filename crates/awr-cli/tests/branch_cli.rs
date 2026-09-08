use awr_core::*;
use awr_store::{BranchFilter, EventQuery, Store};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};
const WORK: &str = "work_items:\n- id: W\n  title: Deliver branch-aware work\n  status: in_progress\n  owner: business-owner\n  next_action: Continue the selected work\n  acceptance: [Preserve work and branch ownership]\n";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let f = Self(std::env::temp_dir().join(format!("awr-branch-{}", Id::new())));
        fs::create_dir_all(f.0.join("decisions")).unwrap();
        fs::write(f.0.join("work.yaml"), WORK).unwrap();
        fs::write(f.0.join("rules.md"),"# Authority {#authority severity=hard scope=project value=*}\n\nPreserve shared source facts exactly.\n").unwrap();
        fs::write(
            f.0.join("goal.md"),
            "# Deliver durable work\n\nRetain the requested work and its results.\n",
        )
        .unwrap();
        fs::write(f.0.join("decisions/adr.md"),"---\naffected_keys: [W]\n---\n\n# Store work history\n\nStatus: accepted\n\n## Decision\n\nRecord work history separately from source facts.\n").unwrap();
        fs::write(f.0.join("sources.toml"),"[project]\nname='Branch fixture'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='goal.md'\nadapter='markdown-heading-v1'\n[sources.options]\nstatus='active'\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='decisions'\nadapter='markdown-directory-v1'\n").unwrap();
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
        Self::success(&self.run(args))
    }
    fn success(r: &Output) -> Value {
        assert!(
            r.status.success(),
            "{} {}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn error(r: &Output, code: &str) {
        assert!(
            !r.status.success(),
            "{}",
            String::from_utf8_lossy(&r.stdout)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&r.stderr).unwrap()["code"],
            code,
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
    }
    fn revision(&self) -> String {
        self.ok(&["branch", "list"])["project_revision"].to_string()
    }
    fn create(&self, name: &str, extra: &[&str]) -> Output {
        let rev = self.revision();
        let mut args = vec![
            "branch",
            "create",
            name,
            "--actor",
            "developer",
            "--reason",
            "Explore the selected work",
            "--expected-revision",
            &rev,
        ];
        args.extend_from_slice(extra);
        self.run(&args)
    }
    fn switch(&self, name: &str) -> Output {
        self.run(&[
            "branch",
            "switch",
            name,
            "--actor",
            "developer",
            "--reason",
            "Continue on the selected work branch",
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn start(&self, agent: &str, claim: bool) -> Value {
        let rev = self.revision();
        let mut args = vec![
            "session",
            "start",
            "--work",
            "W",
            "--agent",
            agent,
            "--provider",
            "fixture",
            "--model",
            "local",
            "--expected-revision",
            &rev,
        ];
        if claim {
            args.push("--claim");
        }
        self.ok(&args)
    }
    fn store(&self) -> (Store, Id) {
        let s = Store::open_existing(&self.0.join(".awr/state.db")).unwrap();
        let p = s
            .project_by_root(&self.0.canonicalize().unwrap())
            .unwrap()
            .id;
        (s, p)
    }
    fn event(&self, session: &str, summary: &str) -> Event {
        let (mut s, p) = self.store();
        let mut e = EventDraft::new("work.observed", summary);
        e.importance = "critical".into();
        e.session_id = Some(session.parse().unwrap());
        let r = s.project(p).unwrap().project_revision;
        s.append_event(p, r, e).unwrap()
    }
    fn evidence(&self, key: &str, branch: Option<Option<Id>>) -> Value {
        let mut draft = json!({"external_key":key,"work_item_key":"W","evidence_type":"report","level":"implemented","summary":format!("Evidence for {key}"),"locator":"report.json","source_sha":"a".repeat(40),"scope":["W"]});
        if let Some(branch) = branch {
            draft["branch_id"] = json!(branch);
        }
        let path = self.0.join("evidence.json");
        fs::write(&path, serde_json::to_vec(&draft).unwrap()).unwrap();
        self.ok(&[
            "evidence",
            "add",
            "--input",
            path.to_str().unwrap(),
            "--expected-revision",
            &self.revision(),
        ])
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
        self.git(&["add", "work.yaml"]);
        self.git(&["commit", "-m", "Fixture source"]);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn creating_branch_records_observed_git_commit_without_changing_checkout_sources_or_selection() {
    let f = Fixture::new();
    f.init_git();
    let sha = f.git(&["rev-parse", "HEAD"]);
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace("Continue the selected work", "Uncommitted user work"),
    )
    .unwrap();
    let source = fs::read(f.0.join("work.yaml")).unwrap();
    let git_before = f.git(&["status", "--porcelain"]);
    let rev = f.revision().parse::<u64>().unwrap();
    let created = Fixture::success(&f.create("experiment", &["--git-ref", "main"]));
    assert_eq!(created["branch"]["fork_project_revision"], rev);
    assert_eq!(created["branch"]["parent_branch_id"], Value::Null);
    assert_eq!(created["branch"]["git_ref"], "main");
    assert_eq!(created["git_binding"]["commit_sha"], sha);
    assert_eq!(created["git_binding"]["resolved_ref"], "refs/heads/main");
    assert_eq!(created["current_branch_id"], Value::Null);
    assert_eq!(created["event"]["event_type"], "branch.created");
    assert_eq!(created["event"]["branch_id"], created["branch"]["id"]);
    assert_eq!(created["project_revision"], rev + 1);
    assert_eq!(f.git(&["status", "--porcelain"]), git_before);
    assert_eq!(f.git(&["symbolic-ref", "HEAD"]), "refs/heads/main");
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), source);
    let shown = f.ok(&["branch", "show", "experiment"]);
    assert_eq!(shown["record"]["git_binding"], created["git_binding"]);
    assert_eq!(shown["record"]["creation_event_id"], created["event"]["id"]);
    f.git(&["add", "work.yaml"]);
    f.git(&["commit", "-m", "Move Git ref after observation"]);
    assert_ne!(f.git(&["rev-parse", "HEAD"]), sha);
    assert_eq!(
        f.ok(&["branch", "show", "experiment"])["record"]["git_binding"]["commit_sha"],
        sha
    );
}

#[test]
fn branching_from_current_or_explicit_main_preserves_lineage_and_bounded_read_queries() {
    let f = Fixture::new();
    let a = Fixture::success(&f.create("a", &[]));
    assert!(a["git_binding"].is_null());
    Fixture::success(&f.switch("a"));
    let fork = f.revision();
    let b = Fixture::success(&f.create("b", &[]));
    assert_eq!(b["branch"]["parent_branch_id"], a["branch"]["id"]);
    assert_eq!(b["branch"]["fork_project_revision"].to_string(), fork);
    let c = Fixture::success(&f.create("c", &["--parent", "main"]));
    assert!(c["branch"]["parent_branch_id"].is_null());
    let rev = f.revision();
    let first = f.ok(&["branch", "list", "--limit", "2"]);
    assert_eq!(first["branches"].as_array().unwrap().len(), 2);
    assert_eq!(first["total"], 3);
    assert_eq!(first["has_more"], true);
    let last = f.ok(&["branch", "list", "--offset", "2", "--limit", "2"]);
    assert_eq!(last["branches"][0]["name"], "c");
    assert_eq!(last["has_more"], false);
    assert_eq!(f.ok(&["branch", "show"])["branch_id"], a["branch"]["id"]);
    assert_eq!(f.revision(), rev);
    Fixture::success(&f.switch("main"));
    assert!(f.ok(&["branch", "show"])["record"].is_null());
    Fixture::error(&f.switch("main"), "InvalidTransition");
    for args in [
        &["branch", "list", "--limit", "0"][..],
        &["branch", "list", "--status", "unknown"][..],
        &["branch", "show", "missing"][..],
    ] {
        assert!(!f.run(args).status.success());
    }
}

#[test]
fn creation_checks_revision_before_git_access_and_failure_preserves_database_and_sources() {
    let f = Fixture::new();
    let source = fs::read(f.0.join("work.yaml")).unwrap();
    let r = f.revision();
    Fixture::error(
        &f.run(&[
            "branch",
            "create",
            "stale",
            "--actor",
            "developer",
            "--reason",
            "Review",
            "--git-ref",
            "missing",
            "--expected-revision",
            "0",
        ]),
        "RevisionConflict",
    );
    Fixture::error(&f.create("no-repo", &["--git-ref", "HEAD"]), "InvalidInput");
    f.init_git();
    Fixture::error(
        &f.create("bad-ref", &["--git-ref", "missing-ref"]),
        "InvalidInput",
    );
    Fixture::error(&f.create("main", &[]), "InvalidInput");
    Fixture::error(&f.create(" ", &[]), "InvalidInput");
    Fixture::error(
        &f.create("bad-parent", &["--parent", "missing"]),
        "NotFound",
    );
    assert_eq!(f.revision(), r);
    assert_eq!(f.ok(&["branch", "list"])["total"], 0);
    assert_eq!(fs::read(f.0.join("work.yaml")).unwrap(), source);
}

#[test]
fn git_binding_handles_detached_commits_tags_and_ignores_unrelated_git_environment() {
    let f = Fixture::new();
    f.init_git();
    let sha = f.git(&["rev-parse", "HEAD"]);
    f.git(&["tag", "-a", "candidate", "-m", "Candidate"]);
    let tag = Fixture::success(&f.create("tag-work", &["--git-ref", "candidate"]));
    assert_eq!(tag["git_binding"]["commit_sha"], sha);
    assert_eq!(tag["git_binding"]["resolved_ref"], "refs/tags/candidate");
    f.git(&["checkout", "--detach", "HEAD"]);
    let detached = Fixture::success(&f.create("detached-work", &["--git-ref", "HEAD"]));
    assert_eq!(detached["git_binding"]["commit_sha"], sha);
    assert!(detached["git_binding"]["resolved_ref"].is_null());
    let commit = Fixture::success(&f.create("commit-work", &["--git-ref", &sha]));
    assert_eq!(commit["git_binding"]["commit_sha"], sha);
    assert!(commit["git_binding"]["resolved_ref"].is_null());
    let r = Command::new(env!("CARGO_BIN_EXE_awr"))
        .arg("--project")
        .arg(&f.0)
        .arg("--json")
        .args([
            "branch",
            "create",
            "env-work",
            "--actor",
            "developer",
            "--reason",
            "Read project Git",
            "--git-ref",
            "HEAD",
            "--expected-revision",
            &f.revision(),
        ])
        .env("GIT_DIR", f.0.join("absent.git"))
        .output()
        .unwrap();
    assert_eq!(Fixture::success(&r)["git_binding"]["commit_sha"], sha);
}

#[test]
fn switching_keeps_existing_runtime_records_and_shared_source_claim_guards() {
    let f = Fixture::new();
    let main = f.start("main-worker", true);
    let ms = main["session"]["id"].as_str().unwrap();
    let me = f.event(ms, "MAIN_PROCESS_FACT");
    let branch = Fixture::success(&f.create("parallel", &[]));
    let bid = branch["branch"]["id"].as_str().unwrap();
    let changed = Fixture::success(&f.switch("parallel"));
    assert_eq!(changed["selection"]["retained_active_sessions"], 1);
    assert_eq!(changed["selection"]["retained_active_claims"], 1);
    let child = f.start("branch-worker", true);
    let cs = child["session"]["id"].as_str().unwrap();
    assert_eq!(child["session"]["branch_id"], bid);
    assert_eq!(child["claim"]["branch_id"], bid);
    let ce = f.event(cs, "BRANCH_PROCESS_FACT");
    assert_eq!(ce.branch_id, Some(bid.parse().unwrap()));
    assert!(f.event(ms, "ANOTHER_MAIN_FACT").branch_id.is_none());
    assert!(f.ok(&["session", "show", ms])["session"]["branch_id"].is_null());
    let blocked = f.run(&[
        "work",
        "progress",
        "W",
        "--session",
        cs,
        "--reason",
        "Apply source progress",
        "--next-action",
        "Continue shared source work",
        "--expected-revision",
        &f.revision(),
    ]);
    Fixture::error(&blocked, "ClaimConflict");
    f.ok(&[
        "work",
        "release",
        "W",
        "--session",
        ms,
        "--claim",
        main["claim"]["id"].as_str().unwrap(),
        "--expected-revision",
        &f.revision(),
    ]);
    f.ok(&[
        "work",
        "progress",
        "W",
        "--session",
        cs,
        "--reason",
        "Apply after resolving shared occupancy",
        "--next-action",
        "Shared source update",
        "--expected-revision",
        &f.revision(),
    ]);
    Fixture::success(&f.switch("main"));
    assert_eq!(
        f.ok(&["object", "show", "work", "W", "--cached", "--full"])["object"]["next_action"],
        "Shared source update"
    );
    let (s, p) = f.store();
    for (scope, yes, no) in [
        (BranchFilter::Main, me.id, ce.id),
        (BranchFilter::Branch(bid.parse().unwrap()), ce.id, me.id),
    ] {
        let events = s
            .query_events(
                p,
                &EventQuery {
                    branch: scope,
                    limit: 100,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(events.events.iter().any(|e| e.id == yes));
        assert!(!events.events.iter().any(|e| e.id == no));
    }
}

#[test]
fn context_and_evidence_keep_branch_identity_and_explicit_main_evidence_is_preserved() {
    let f = Fixture::new();
    let main = f.start("main-worker", false);
    let ms = main["session"]["id"].as_str().unwrap();
    f.event(ms, "MAIN_ONLY_EVENT");
    let base = f.ok(&[
        "context",
        "compile",
        "--session",
        ms,
        "--after-revision",
        "0",
    ]);
    assert!(base["work_context"]["identity"]["branch_id"].is_null());
    let created = Fixture::success(&f.create("context-work", &[]));
    let bid = created["branch"]["id"].as_str().unwrap();
    Fixture::success(&f.switch("context-work"));
    let child = f.start("branch-worker", false);
    let cs = child["session"]["id"].as_str().unwrap();
    f.event(cs, "BRANCH_ONLY_EVENT");
    let proof = f.evidence("branch-proof", None);
    assert_eq!(proof["evidence"]["branch_id"], bid);
    let explicit_main = f.evidence("main-proof", Some(None));
    assert!(explicit_main["evidence"]["branch_id"].is_null());
    let pack = f.ok(&[
        "context",
        "compile",
        "--session",
        cs,
        "--after-revision",
        "0",
    ]);
    assert_eq!(pack["completeness"]["complete"], true);
    assert_eq!(pack["work_context"]["identity"]["branch_id"], bid);
    assert_ne!(
        pack["work_context"]["context_hash"],
        base["work_context"]["context_hash"]
    );
    let rendered = pack["work_context"]["rendered_context"].as_str().unwrap();
    assert!(rendered.contains("BRANCH_ONLY_EVENT"));
    assert!(!rendered.contains("MAIN_ONLY_EVENT"));
    Fixture::error(
        &f.run(&["context", "compile", "--session", ms]),
        "InvalidInput",
    );
    Fixture::success(&f.switch("main"));
    let report = f.ok(&[
        "evidence",
        "show",
        "branch-proof",
        "--source-sha",
        &"a".repeat(40),
    ]);
    assert_eq!(report["currency"], "historical");
    assert_eq!(report["evidence"]["branch_id"], bid);
    let (s, p) = f.store();
    let events = s
        .query_events(
            p,
            &EventQuery {
                event_type: Some("evidence.recorded".into()),
                branch: BranchFilter::Branch(bid.parse().unwrap()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(events.events.len(), 1);
}

#[test]
fn event_failures_roll_back_branch_creation_and_current_selection_with_no_fake_receipts() {
    let f = Fixture::new();
    let r = f.revision();
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_branch_create BEFORE INSERT ON events WHEN NEW.event_type='branch.created' BEGIN SELECT RAISE(ABORT,'injected branch failure'); END;").unwrap();
    Fixture::error(&f.create("a", &[]), "Storage");
    assert_eq!(f.revision(), r);
    assert_eq!(f.ok(&["branch", "list"])["total"], 0);
    db.execute_batch("DROP TRIGGER fail_branch_create").unwrap();
    let branch = Fixture::success(&f.create("a", &[]));
    let r = f.revision();
    db.execute_batch("CREATE TRIGGER fail_branch_switch BEFORE INSERT ON events WHEN NEW.event_type='branch.switched' BEGIN SELECT RAISE(ABORT,'injected switch failure'); END;").unwrap();
    Fixture::error(&f.switch("a"), "Storage");
    assert_eq!(f.revision(), r);
    assert!(f.ok(&["branch", "show"])["branch_id"].is_null());
    let (mut s, p) = f.store();
    for kind in ["branch.created", "branch.switched"] {
        assert!(matches!(
            s.append_event(
                p,
                r.parse().unwrap(),
                EventDraft::new(kind, "Forged branch receipt")
            ),
            Err(Error::InvalidInput(_))
        ));
    }
    assert_eq!(
        s.branch(p, branch["branch"]["id"].as_str().unwrap().parse().unwrap())
            .unwrap()
            .creation_event_id
            .unwrap()
            .to_string(),
        branch["event"]["id"].as_str().unwrap()
    );
}

#[test]
fn closed_or_invalid_branches_cannot_receive_new_work_or_evidence_and_legacy_rows_remain_readable()
{
    let f = Fixture::new();
    let a = Fixture::success(&f.create("a", &[]));
    let aid = a["branch"]["id"].as_str().unwrap();
    Fixture::error(&f.create("a", &[]), "InvalidInput");
    let db = rusqlite::Connection::open(f.0.join(".awr/state.db")).unwrap();
    db.execute("UPDATE branches SET status='abandoned' WHERE id=?1", [aid])
        .unwrap();
    Fixture::error(&f.switch("a"), "NotFound");
    Fixture::error(&f.create("child", &["--parent", "a"]), "NotFound");
    assert_eq!(
        f.ok(&["branch", "list", "--status", "abandoned"])["total"],
        1
    );
    let (mut s, p) = f.store();
    let r = s.project(p).unwrap().project_revision;
    let mut event = EventDraft::new(
        "work.observed",
        "Cannot append new process events on a closed branch",
    );
    event.branch_id = Some(aid.parse().unwrap());
    assert!(matches!(
        s.append_event(p, r, event),
        Err(Error::NotFound(_))
    ));
    let invalid = Id::new();
    assert!(matches!(
        s.create_branch(
            p,
            r,
            BranchDraft {
                name: "foreign".into(),
                parent_branch_id: Some(invalid),
                git_binding: None,
                actor: "developer".into(),
                reason: "Review".into()
            }
        ),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        s.record_evidence(
            p,
            r,
            EvidenceDraft {
                external_key: "closed-proof".into(),
                work_item_key: Some("W".into()),
                evidence_type: "report".into(),
                level: EvidenceLevel::Implemented,
                summary: "Closed branch record".into(),
                locator: "report.json".into(),
                sha256: None,
                source_sha: None,
                command: None,
                scope: vec!["W".into()],
                branch_id: Some(aid.parse().unwrap()),
                verified_at: None
            }
        ),
        Err(Error::NotFound(_))
    ));
    let legacy = Id::new();
    db.execute("INSERT INTO branches(id,project_id,name,git_ref,fork_project_revision,status,revision) VALUES(?1,?2,'legacy','unverified/ref',0,'active',1)",rusqlite::params![legacy.to_string(),p.to_string()]).unwrap();
    let record = s.branch(p, legacy).unwrap();
    assert_eq!(record.branch.git_ref.as_deref(), Some("unverified/ref"));
    assert!(record.git_binding.is_none() && record.creation_event_id.is_none());
    db.execute(
        "UPDATE branches SET fork_project_revision=?1 WHERE id=?2",
        rusqlite::params![r as i64 + 100, legacy.to_string()],
    )
    .unwrap();
    Fixture::error(&f.switch("legacy"), "SourceConflict");
}

#[test]
fn name_id_ambiguity_and_cross_project_parent_are_rejected_without_fallback() {
    let f = Fixture::new();
    let a = Fixture::success(&f.create("a", &[]));
    let id = a["branch"]["id"].as_str().unwrap();
    Fixture::success(&f.create(id, &[]));
    Fixture::error(&f.switch(id), "InvalidInput");
    Fixture::success(&f.switch("a"));
    let other = Fixture::new();
    let (mut s, p) = f.store();
    let foreign = s
        .register_project(&other.0, "other-project", "Other project")
        .unwrap();
    let (other_branch, _) = s
        .create_branch(
            foreign.id,
            foreign.project_revision,
            BranchDraft {
                name: "other".into(),
                parent_branch_id: None,
                git_binding: None,
                actor: "developer".into(),
                reason: "Review".into(),
            },
        )
        .unwrap();
    let r = s.project(p).unwrap().project_revision;
    assert!(matches!(
        s.create_branch(
            p,
            r,
            BranchDraft {
                name: "foreign-parent".into(),
                parent_branch_id: Some(other_branch.id),
                git_binding: None,
                actor: "developer".into(),
                reason: "Review".into()
            }
        ),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn branch_reads_and_switches_work_without_git_or_source_access_and_do_not_reindex() {
    let f = Fixture::new();
    let created = Fixture::success(&f.create("offline", &[]));
    let source = f.0.join("work.yaml");
    fs::rename(&source, f.0.join("work-away.yaml")).unwrap();
    fs::rename(
        f.0.join(".awr/project.toml"),
        f.0.join("manifest-away.toml"),
    )
    .unwrap();
    let r = f.revision();
    let shown = f.ok(&["branch", "show", "offline"]);
    assert_eq!(shown["record"]["branch"]["id"], created["branch"]["id"]);
    assert_eq!(f.revision(), r);
    Fixture::success(&f.switch("offline"));
    assert!(!source.exists());
    Fixture::success(&f.switch("main"));
}
