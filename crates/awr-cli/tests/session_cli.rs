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
        let root = std::env::temp_dir().join(format!("awr-session-cli-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("work-ledger.yaml"),"work_items:\n- id: W\n  title: Implement runtime\n  owner: source-team\n  status: in_progress\n  next_action: Wire session\n- id: OTHER\n  title: Other work\n  status: ready\n").unwrap();
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
    fn error(&self, args: &[&str], code: &str) {
        let r = self.run(args);
        assert!(!r.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&r.stderr).unwrap()["code"],
            code
        );
    }
    fn revision(&self) -> String {
        self.ok(&["session", "list"])["project_revision"].to_string()
    }
    fn start(&self, agent: &str, claim: bool) -> Value {
        let rev = self.ok(&["status"])["project_revision"].to_string();
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
            "test",
            "--expected-revision",
            &rev,
        ];
        if claim {
            args.push("--claim");
        }
        self.ok(&args)
    }
    fn checkpoint(&self, id: &str) -> Value {
        self.ok(&[
            "session",
            "checkpoint",
            "--session",
            id,
            "--context-hash",
            &"a".repeat(64),
            "--digest",
            "Implemented session plumbing",
            "--next-action",
            "Connect context compiler",
            "--open-loop",
            "Keep unfinished artifact review",
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
fn cli_handoff_transfers_claim_and_preserves_both_histories_and_open_loops() {
    let f = Fixture::new();
    let source = fs::read(f.0.join("work-ledger.yaml")).unwrap();
    let first = f.start("executor", true);
    let sender = first["session"]["id"].as_str().unwrap();
    let old_claim = first["claim"]["id"].as_str().unwrap();
    let second = f.start("successor", false);
    let receiver = second["session"]["id"].as_str().unwrap();
    f.error(
        &[
            "session",
            "end",
            "--work",
            "W",
            "--expected-revision",
            &f.revision(),
        ],
        "InvalidInput",
    );
    f.error(
        &[
            "work",
            "handoff",
            "W",
            "--session",
            sender,
            "--to-session",
            receiver,
            "--expected-revision",
            &f.revision(),
        ],
        "ContextIncomplete",
    );
    let checkpoint = f.checkpoint(sender);
    let before = f.revision();
    f.error(
        &[
            "work",
            "handoff",
            "W",
            "--session",
            sender,
            "--to-session",
            sender,
            "--expected-revision",
            &before,
        ],
        "InvalidInput",
    );
    assert_eq!(f.revision(), before);
    f.error(
        &[
            "work",
            "handoff",
            "W",
            "--session",
            sender,
            "--to-session",
            receiver,
            "--expected-revision",
            "0",
        ],
        "RevisionConflict",
    );
    assert_eq!(f.revision(), before);
    let handed = f.ok(&[
        "work",
        "handoff",
        "W",
        "--session",
        sender,
        "--to-session",
        receiver,
        "--expected-revision",
        &before,
    ]);
    assert_eq!(handed["handoff"]["from_session"]["status"], "incomplete");
    assert_eq!(
        handed["handoff"]["transferred_claim"]["agent_id"],
        "successor"
    );
    assert_ne!(handed["handoff"]["transferred_claim"]["id"], old_claim);
    let shown = f.ok(&["session", "show", receiver]);
    assert!(shown["checkpoint"].is_null());
    assert_eq!(
        shown["inherited_checkpoint"]["id"],
        checkpoint["checkpoint"]["id"]
    );
    assert_eq!(
        shown["inherited_checkpoint"]["open_loops"][0],
        "Keep unfinished artifact review"
    );
    assert_eq!(shown["context_requires_refresh"], true);
    assert_eq!(
        f.ok(&["session", "show", sender])["claims"][0]["status"],
        "released"
    );
    // Both receipts share one revision. Cursor pagination must not lose the second receipt.
    let after = handed["project_revision"].as_u64().unwrap() - 1;
    let page = f.ok(&[
        "work",
        "history",
        "W",
        "--after-revision",
        &after.to_string(),
        "--limit",
        "1",
    ]);
    let cursor = page["next_cursor"].to_string();
    assert!(page["next_cursor"].is_object());
    assert!(page["events"][0].get("payload").is_none());
    let next = f.ok(&["work", "history", "W", "--cursor", &cursor, "--limit", "1"]);
    assert_ne!(page["events"][0]["id"], next["events"][0]["id"]);
    assert_eq!(
        page["events"][0]["project_revision"],
        next["events"][0]["project_revision"]
    );
    for id in [sender, receiver] {
        assert!(
            !f.ok(&[
                "work",
                "history",
                "W",
                "--session",
                id,
                "--after-revision",
                &after.to_string()
            ])["events"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    f.ok(&[
        "session",
        "end",
        "--session",
        receiver,
        "--expected-revision",
        &f.revision(),
    ]);
    assert!(
        f.ok(&["session", "list", "--active"])["sessions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(fs::read(f.0.join("work-ledger.yaml")).unwrap(), source);
    assert_eq!(f.ok(&["doctor"])["ok"], true);
}

#[test]
fn cleanup_and_retained_history_work_without_authoritative_sources() {
    let f = Fixture::new();
    let started = f.start("worker", false);
    let id = started["session"]["id"].as_str().unwrap();
    let acquired = f.ok(&[
        "work",
        "claim",
        "W",
        "--agent",
        "worker",
        "--expected-revision",
        &f.revision(),
    ]);
    let claim = acquired["claim"]["id"].as_str().unwrap();
    f.ok(&[
        "work",
        "release",
        "W",
        "--session",
        id,
        "--claim",
        claim,
        "--expected-revision",
        &f.revision(),
    ]);
    f.ok(&[
        "work",
        "claim",
        "W",
        "--session",
        id,
        "--expected-revision",
        &f.revision(),
    ]);
    fs::remove_file(f.0.join("work-ledger.yaml")).unwrap();
    let ended = f.ok(&[
        "session",
        "end",
        "--session",
        id,
        "--outcome",
        "interrupted",
        "--expected-revision",
        &f.revision(),
    ]);
    assert_eq!(ended["source_refresh_performed"], false);
    assert_eq!(ended["closed_claim_ids"].as_array().unwrap().len(), 1);
    assert_eq!(
        f.ok(&["session", "show", id])["claims"][1]["status"],
        "released"
    );
    assert!(
        !f.ok(&["work", "history", "W"])["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unassigned_handoff_releases_work_and_keeps_checkpoint_for_later_pickup() {
    let f = Fixture::new();
    let started = f.start("worker", true);
    let id = started["session"]["id"].as_str().unwrap();
    f.checkpoint(id);
    let ended = f.ok(&[
        "work",
        "handoff",
        "W",
        "--session",
        id,
        "--expected-revision",
        &f.revision(),
    ]);
    assert!(ended["handoff"]["to_session"].is_null());
    assert!(ended["handoff"]["transferred_claim"].is_null());
    assert_eq!(
        f.ok(&["session", "show", id])["claims"][0]["status"],
        "released"
    );
    f.start("later-worker", true);
}
