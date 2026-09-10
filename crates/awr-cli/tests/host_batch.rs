//! Native same-source batch, archive and interrupted-index fixtures.
use awr_core::Id;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
};
struct Host {
    root: PathBuf,
    file: &'static str,
}
impl Host {
    fn new(markdown: bool) -> Self {
        let h = Self {
            root: std::env::temp_dir().join(format!("awr 批量 {}", Id::new())),
            file: if markdown { "work.md" } else { "work.yaml" },
        };
        fs::create_dir(&h.root).unwrap();
        h.write(h.file,if markdown{"# Tasks\n\n| id | title | status | next_action |\n| --- | --- | --- | --- |\n| W | Read article | planned | Write note |\n| D | Draft task | draft | |\n\nKeep this paragraph.\n"}else{"# Keep this comment\nwork_items:\n- id: W\n  title: Read article\n  status: planned\n  next_action: Write note\n  custom: preserve\n- id: D\n  title: Draft task\n  status: draft\n"});
        h.write("GOAL.md", "# Reading {#reading}\n\nRetain useful ideas.\n");
        h.write("mapping.toml",&format!("[project]\nname='Batch fixture'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='{}'\nadapter='{}'\n[[sources]]\ndomain='goal'\nrole='supporting'\npath='GOAL.md'\nadapter='markdown-heading-v1'\n",h.file,if markdown{"markdown-ledger-v1"}else{"yaml-ledger-v1"}));
        h.ok(&["init", "--manifest", "mapping.toml", "--accept"]);
        h
    }
    fn write(&self, p: &str, s: &str) {
        fs::write(self.root.join(p), s).unwrap()
    }
    fn text(&self, p: &str) -> String {
        fs::read_to_string(self.root.join(p)).unwrap()
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .env_clear()
            .args(["--project", self.root.to_str().unwrap(), "--json"])
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "{args:?}\n{}\n{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    }
    fn error(&self, args: &[&str], code: &str) -> Output {
        let o = self.run(args);
        assert!(!o.status.success(), "{args:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&o.stderr).unwrap()["code"],
            code,
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        o
    }
    fn revision(&self) -> String {
        self.ok(&["session", "list"])["project_revision"].to_string()
    }
    fn request(&self, key: &str, operations: Value) {
        let sources = self.ok(&["source", "list"]);
        let source = sources["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["domain"] == "ledger")
            .unwrap();
        self.write("batch.json",&json!({"version":1,"request_key":key,"actor":{"host":"fixture","subject":"editor","origin":"ai_accepted"},"reason":"Apply the reviewed reading plan edits","change":{"kind":"ledger","source_id":source["id"],"source_fingerprint":source["fingerprint"],"operations":operations}}).to_string())
    }
    fn preview(&self) -> Value {
        self.ok(&["batch", "change", "--input", "batch.json"])
    }
    fn args(&self, p: &Value) -> Vec<String> {
        vec![
            "batch".into(),
            "change".into(),
            "--input".into(),
            "batch.json".into(),
            "--accept".into(),
            "--expected-preview".into(),
            p["preview"]["fingerprint"].as_str().unwrap().into(),
            "--expected-revision".into(),
            p["project_revision"].to_string(),
        ]
    }
    fn accept(&self, p: &Value) -> Value {
        self.ok(&self.args(p).iter().map(String::as_str).collect::<Vec<_>>())
    }
    fn sql(&self, s: &str) {
        rusqlite::Connection::open(self.root.join(".awr/state.db"))
            .unwrap()
            .execute_batch(s)
            .unwrap()
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn all_operations_are_preflighted_before_writing_and_exact_preview_is_required() {
    for md in [false, true] {
        let h = Host::new(md);
        let before = h.text(h.file);
        let rev = h.revision();
        h.request("invalid-second",json!([{"operation":"fields","target":"W","fields":{"title":"New title"}},{"operation":"fields","target":"D","fields":{"status":"completed"}}]));
        h.error(
            &["batch", "change", "--input", "batch.json"],
            "MutationUnsupported",
        );
        assert_eq!(h.text(h.file), before);
        assert_eq!(h.revision(), rev);
        h.request("valid",json!([{"operation":"fields","target":"W","fields":{"title":"New title"}},{"operation":"fields","target":"D","fields":{"next_action":"Review draft"}}]));
        let p = h.preview();
        h.error(
            &[
                "batch",
                "change",
                "--input",
                "batch.json",
                "--accept",
                "--expected-revision",
                &h.revision(),
            ],
            "SourceConflict",
        );
        assert_eq!(h.text(h.file), before);
        let r = h.accept(&p);
        assert_eq!(r["status"], "completed");
        assert_eq!(r["actor"]["origin"], "ai_accepted");
        assert!(h.text(h.file).contains("New title"));
        assert!(h.text(h.file).contains(if md {
            "Keep this paragraph."
        } else {
            "custom: preserve"
        }));
        let rev = h.revision();
        assert_eq!(h.accept(&p)["already_recorded"], true);
        assert_eq!(h.revision(), rev);
        assert_eq!(h.ok(&["session", "list"])["sessions"], json!([]));
    }
}
#[test]
fn imports_keep_explicit_ids_remain_drafts_and_never_overwrite_duplicates() {
    for md in [false, true] {
        let h = Host::new(md);
        let ops = json!([{"operation":"import","external_key":"N","title":"New reading","fields":{"next_action":"Review scope","acceptance":["Note is available"]},"duplicate":"fail"}]);
        h.request("import", ops.clone());
        let r = h.accept(&h.preview());
        let work = h.ok(&["work", "show", "N"]);
        assert_eq!(work["work"]["status"], "draft");
        assert_eq!(
            h.ok(&["batch", "status", "--key", "import"])["applied_files"],
            r["applied_files"]
        );
        h.request("duplicate", ops.clone());
        let before = h.text(h.file);
        h.error(
            &["batch", "change", "--input", "batch.json"],
            "SourceConflict",
        );
        assert_eq!(h.text(h.file), before);
        let mut skip = ops.clone();
        skip[0]["duplicate"] = json!("skip_exact");
        h.request("skip", skip.clone());
        let p = h.preview();
        let rev = h.revision();
        let r = h.accept(&p);
        assert_eq!(r["status"], "no_change");
        assert_eq!(r["outcomes"][0]["status"], "skipped_exact");
        assert_eq!(h.revision(), rev);
        assert_eq!(
            h.ok(&["work", "show", "N"])["work"]["id"],
            work["work"]["id"]
        );
        skip[0]["fields"]["next_action"] = json!("Conflicting scope");
        h.request("conflict", skip);
        h.error(
            &["batch", "change", "--input", "batch.json"],
            "SourceConflict",
        );
    }
}
#[test]
fn archive_restore_preserves_identity_status_and_does_not_count_as_completion() {
    for md in [false, true] {
        let h = Host::new(md);
        let w = h.ok(&["work", "show", "W"]);
        h.request(
            "archive",
            json!([{"operation":"archive","target":"W","archived":true}]),
        );
        h.accept(&h.preview());
        let archived = h.ok(&["work", "show", "W"]);
        assert_eq!(archived["work"]["id"], w["work"]["id"]);
        assert_eq!(archived["work"]["status"], w["work"]["status"]);
        assert_eq!(archived["work"]["archived"], true);
        let ready = h.ok(&["ready"]).to_string();
        assert!(!ready.contains(w["work"]["id"].as_str().unwrap()));
        let status = h.ok(&["status"]);
        assert_eq!(status["organization"]["source_archived"], 1);
        assert_eq!(status["organization"]["source_completed"], 0);
        h.request(
            "restore",
            json!([{"operation":"archive","target":"W","archived":false}]),
        );
        h.accept(&h.preview());
        let restored = h.ok(&["work", "show", "W"]);
        assert_eq!(restored["work"]["id"], w["work"]["id"]);
        assert_eq!(restored["work"]["archived"], false);
        assert_eq!(restored["work"]["status"], "planned");
    }
}
#[test]
fn dependency_graph_is_checked_for_the_whole_batch() {
    let h = Host::new(false);
    h.request(
        "dependency",
        json!([{"operation":"fields","target":"D","fields":{"depends_on":["W"]}}]),
    );
    h.accept(&h.preview());
    let before = h.text(h.file);
    h.request(
        "blocked",
        json!([{"operation":"archive","target":"W","archived":true}]),
    );
    h.error(
        &["batch", "change", "--input", "batch.json"],
        "RuleViolation",
    );
    assert_eq!(h.text(h.file), before);
    h.request("archive-scope",json!([{"operation":"archive","target":"W","archived":true},{"operation":"archive","target":"D","archived":true}]));
    h.accept(&h.preview());
    h.request(
        "invalid-restore",
        json!([{"operation":"archive","target":"D","archived":false}]),
    );
    h.error(
        &["batch", "change", "--input", "batch.json"],
        "RuleViolation",
    );
    h.request("restore-scope",json!([{"operation":"archive","target":"W","archived":false},{"operation":"archive","target":"D","archived":false}]));
    h.accept(&h.preview());
}
#[test]
fn real_index_failure_leaves_a_recoverable_receipt_and_blocks_other_source_writers() {
    let h = Host::new(false);
    h.request(
        "interrupted",
        json!([{"operation":"fields","target":"W","fields":{"next_action":"Review saved note"}}]),
    );
    let p = h.preview();
    h.sql("CREATE TRIGGER fail_batch_index BEFORE UPDATE OF fingerprint ON sources WHEN NEW.fingerprint<>OLD.fingerprint BEGIN SELECT RAISE(ABORT,'fixture index failure'); END;");
    let o = h.run(&h.args(&p).iter().map(String::as_str).collect::<Vec<_>>());
    assert!(!o.status.success());
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["status"], "pending_recovery");
    assert_eq!(r["source_write_performed"], true);
    assert!(h.text(h.file).contains("Review saved note"));
    h.sql("DROP TRIGGER fail_batch_index;");
    h.ok(&["source", "reindex"]);
    let work = h.ok(&["work", "show", "W"]);
    h.write("host.json",&json!({"version":1,"request_key":"other-writer","actor":{"host":"fixture","subject":"user","origin":"human"},"reason":"Another edit","change":{"operation":"fields","kind":"work_item","target":"W","source_fingerprint":work["source_ref"]["source_fingerprint"],"fields":{"title":"Another title"}}}).to_string());
    h.error(
        &[
            "host",
            "save",
            "--input",
            "host.json",
            "--expected-revision",
            &h.revision(),
        ],
        "MutationConflict",
    );
    let recovered = h.ok(&[
        "batch",
        "recover",
        "--key",
        "interrupted",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(recovered["status"], "completed");
    assert_eq!(recovered["source_write_performed"], false);
    let rev = h.revision();
    h.ok(&[
        "batch",
        "recover",
        "--key",
        "interrupted",
        "--expected-revision",
        &rev,
    ]);
    assert_eq!(h.revision(), rev);
}
#[test]
fn recovery_never_replaces_external_edits_and_foreign_project_receipts_are_rejected() {
    let h = Host::new(false);
    h.request(
        "pending",
        json!([{"operation":"fields","target":"W","fields":{"title":"Reviewed title"}}]),
    );
    let p = h.preview();
    h.sql("CREATE TRIGGER fail_batch_index BEFORE UPDATE OF fingerprint ON sources WHEN NEW.fingerprint<>OLD.fingerprint BEGIN SELECT RAISE(ABORT,'fixture failure'); END;");
    let o = h.run(&h.args(&p).iter().map(String::as_str).collect::<Vec<_>>());
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    h.sql("DROP TRIGGER fail_batch_index;");
    h.write(
        h.file,
        &format!("{}\n# External edit retained\n", h.text(h.file)),
    );
    let before = h.text(h.file);
    h.error(
        &[
            "batch",
            "recover",
            "--key",
            "pending",
            "--expected-revision",
            &h.revision(),
        ],
        "SourceConflict",
    );
    assert_eq!(h.text(h.file), before);
    let path = h
        .root
        .join(r["recovery_directory"].as_str().unwrap())
        .join("receipt.json");
    let mut receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    receipt["plan"]["project_id"] = json!(Id::new());
    fs::write(&path, receipt.to_string()).unwrap();
    h.error(&["batch", "status", "--key", "pending"], "SourceConflict");
}
#[test]
fn project_scoped_request_keys_and_changed_actor_have_separate_outcomes() {
    let a = Host::new(false);
    let b = Host::new(false);
    for h in [&a, &b] {
        h.request(
            "same-key",
            json!([{"operation":"fields","target":"W","fields":{"title":"Reviewed reading"}}]),
        );
        h.accept(&h.preview());
    }
    assert_ne!(
        a.ok(&["batch", "status", "--key", "same-key"])["project_id"],
        b.ok(&["batch", "status", "--key", "same-key"])["project_id"]
    );
    let mut v: Value = serde_json::from_str(&a.text("batch.json")).unwrap();
    v["actor"]["subject"] = json!("different-user");
    a.write("batch.json", &v.to_string());
    a.error(
        &["batch", "change", "--input", "batch.json"],
        "SourceConflict",
    );
}

#[test]
fn active_execution_prevents_archiving_until_its_claim_is_released() {
    let h = Host::new(false);
    let s = h.ok(&[
        "session",
        "start",
        "--work",
        "W",
        "--agent",
        "fixture",
        "--provider",
        "fixture",
        "--model",
        "fixture",
        "--claim",
        "--expected-revision",
        &h.revision(),
    ]);
    h.request(
        "archive-active",
        json!([{"operation":"archive","target":"W","archived":true}]),
    );
    let before = h.text(h.file);
    h.error(
        &["batch", "change", "--input", "batch.json"],
        "ClaimConflict",
    );
    assert_eq!(h.text(h.file), before);
    h.ok(&[
        "session",
        "end",
        "--session",
        s["session"]["id"].as_str().unwrap(),
        "--expected-revision",
        &h.revision(),
    ]);
    h.accept(&h.preview());
    h.error(
        &[
            "session",
            "start",
            "--work",
            "W",
            "--agent",
            "fixture",
            "--provider",
            "fixture",
            "--model",
            "fixture",
            "--claim",
            "--expected-revision",
            &h.revision(),
        ],
        "DependencyBlocked",
    );
}
