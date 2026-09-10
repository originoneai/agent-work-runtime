//! Native host-save fixtures, including a real interrupted final receipt.
use awr_core::Id;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
};
struct Host(PathBuf);
impl Host {
    fn new(text: &str) -> Self {
        let h = Self(std::env::temp_dir().join(format!("awr Markdown 台账 {}", Id::new())));
        fs::create_dir(&h.0).unwrap();
        h.write("work.md", text);
        h.write(
            "GOAL.md",
            "# Reading {#reading}\n\nKeep useful reading notes.\n",
        );
        h.write("mapping.toml","[project]\nname='Markdown work'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.md'\nadapter='markdown-ledger-v1'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='GOAL.md'\nadapter='markdown-heading-v1'\n[sources.options]\nstatus='active'\nkey_prefix='g'\n");
        h.ok(&["init", "--manifest", "mapping.toml", "--accept"]);
        h
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.0.join(path), text).unwrap()
    }
    fn text(&self, path: &str) -> String {
        fs::read_to_string(self.0.join(path)).unwrap()
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .env_clear()
            .args(["--project", self.0.to_str().unwrap(), "--json"])
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
    fn envelope(&self, key: &str, origin: &str, change: Value) {
        self.write("save.json",&json!({"version":1,"request_key":key,"actor":{"host":"fixture-desktop","subject":"fixture-user","origin":origin},"reason":"Save the explicit edit shown in the fixture host","change":change}).to_string())
    }
    fn fields(&self, key: &str, origin: &str, work: &str, fields: Value) {
        let w = self.ok(&["work", "show", work]);
        self.envelope(key,origin,json!({"operation":"fields","kind":"work_item","target":work,"source_fingerprint":w["source_ref"]["source_fingerprint"],"fields":fields}))
    }
    fn activate(&self, key: &str, work: &str) {
        let w = self.ok(&["work", "show", work]);
        self.envelope(key,"human",json!({"operation":"activate_draft","work":work,"source_fingerprint":w["source_ref"]["source_fingerprint"]}))
    }
    fn save(&self) -> Value {
        self.ok(&[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn sql(&self, sql: &str) {
        rusqlite::Connection::open(self.0.join(".awr/state.db"))
            .unwrap()
            .execute_batch(sql)
            .unwrap()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const TABLE: &str = "# Reading ledger\n\n| ID | 标题 | 状态 | 目标 | 验收 | 下一步 | unknown |\n| --- | --- | --- | --- | --- | --- | --- |\n| W | Read A \\| B | `planned` | g#reading | Useful note | Write notes | `keep \\| code` |\n| U | Same title | planned | g#reading | Useful note | Read more | keep |\n\n```md\n| W | Fenced example must remain unchanged |\n```\n";
const LIST: &str = "# Reading\n\n- [ ] Read notes <!-- awr:id=\"W\" --> <!-- awr:goal=\"g#reading\" --> <!-- awr:acceptance=[\"Useful note\"] --> <!-- awr:next_action=\"Read it\" -->\n- [ ] Another task <!-- awr:id=\"U\" -->\n\nKeep this prose.\n";

#[test]
fn table_field_edits_preserve_every_other_byte_and_stable_ids() {
    for newline in ["\n", "\r\n"] {
        let before = TABLE.replace('\n', newline);
        let h = Host::new(&before);
        let id = h.ok(&["work", "show", "W"])["work"]["id"].clone();
        h.fields("title", "human", "W", json!({"title":"Review C | D"}));
        h.save();
        assert_eq!(
            h.text("work.md"),
            before.replace("Read A \\| B", "Review C \\| D")
        );
        assert_eq!(h.ok(&["work", "show", "W"])["work"]["id"], id);
        h.fields(
            "criteria",
            "human",
            "W",
            json!({"acceptance":["Useful note","Correct references"]}),
        );
        h.save();
        assert_eq!(
            h.ok(&["work", "show", "W"])["acceptance"],
            json!(["Useful note", "Correct references"])
        );
    }
}

#[test]
fn checklist_identity_and_individual_metadata_edits_preserve_prose() {
    let h = Host::new(LIST);
    let id = h.ok(&["work", "show", "W"])["work"]["id"].clone();
    h.fields(
        "rename",
        "human",
        "W",
        json!({"title":"Read and annotate notes"}),
    );
    h.save();
    assert_eq!(
        h.text("work.md"),
        LIST.replace("Read notes", "Read and annotate notes")
    );
    let before = h.text("work.md");
    h.fields("next", "human", "W", json!({"next_action":"Review it"}));
    h.save();
    assert_eq!(
        h.text("work.md"),
        before.replace("\"Read it\"", "\"Review it\"")
    );
    assert_eq!(h.ok(&["work", "show", "W"])["work"]["id"], id);
    let legacy = Host::new("- [ ] Unnumbered old task\n");
    let rows = legacy.ok(&["object", "list", "work"]);
    let key = rows["items"][0]["external_key"].as_str().unwrap();
    legacy.fields("unsafe", "human", key, json!({"title":"Changed"}));
    assert!(
        !legacy
            .run(&["host", "preview", "--input", "save.json"])
            .status
            .success()
    );
    assert_eq!(legacy.text("work.md"), "- [ ] Unnumbered old task\n");
}

#[test]
fn creation_retries_keep_one_draft_and_original_rows() {
    for text in [TABLE, LIST] {
        let h = Host::new(text);
        h.write(
            "new.json",
            &json!({"version":1,"request_key":"new","title":"New work | review"}).to_string(),
        );
        let p = h.ok(&["work", "create", "--input", "new.json"]);
        let r = h.ok(&[
            "work",
            "create",
            "--input",
            "new.json",
            "--accept",
            "--expected-preview",
            p["preview"]["fingerprint"].as_str().unwrap(),
            "--expected-revision",
            &h.revision(),
        ]);
        let source = h.text("work.md");
        let revision = h.revision();
        let replay = h.ok(&[
            "work",
            "create",
            "--input",
            "new.json",
            "--accept",
            "--expected-preview",
            p["preview"]["fingerprint"].as_str().unwrap(),
            "--expected-revision",
            &revision,
        ]);
        assert_eq!(h.text("work.md"), source);
        assert_eq!(r["work_id"], replay["work_id"]);
        let key = r["external_key"].as_str().unwrap();
        let w = h.ok(&["work", "show", key]);
        assert_eq!(w["work"]["status"], "draft");
        assert_eq!(w["work"]["ready"], false);
        h.fields("fill-new","human",key,json!({"goal":"g#reading","acceptance":["Useful note"],"next_action":"Read the new article"}));
        h.save();
        h.activate("activate-new", key);
        h.save();
        assert_eq!(h.ok(&["work", "show", key])["work"]["ready"], true);
    }
}

#[test]
fn lifecycle_guards_activate_and_progress_real_markdown_records() {
    let h = Host::new(LIST);
    h.fields("forged", "human", "W", json!({"status":"completed"}));
    h.error(
        &["host", "preview", "--input", "save.json"],
        "MutationUnsupported",
    );
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
    h.ok(&[
        "work",
        "progress",
        "W",
        "--session",
        s["session"]["id"].as_str().unwrap(),
        "--reason",
        "Read the article",
        "--next-action",
        "Write the useful note",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(
        h.ok(&["work", "show", "W"])["work"]["status"],
        "in_progress"
    );
    assert!(
        h.text("work.md")
            .contains("<!-- awr:status=\"in_progress\" -->")
    );
    h.ok(&[
        "work",
        "cancel",
        "W",
        "--session",
        s["session"]["id"].as_str().unwrap(),
        "--reason",
        "Reading deferred by owner",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(h.ok(&["work", "show", "W"])["work"]["status"], "cancelled");
    assert!(h.text("work.md").contains("- [ ] Read notes"));
}

#[test]
fn lost_receipts_recover_once_and_external_changes_remain_untouched() {
    let h = Host::new(TABLE);
    h.fields(
        "recover",
        "human",
        "W",
        json!({"next_action":"Write the revised note"}),
    );
    h.sql("CREATE TRIGGER fail_receipt BEFORE INSERT ON events WHEN NEW.event_type='proposal.applied' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;");
    let out = h.run(&[
        "host",
        "save",
        "--input",
        "save.json",
        "--expected-revision",
        &h.revision(),
    ]);
    assert!(!out.status.success());
    let partial: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(partial["status"], "pending_recovery");
    let source = h.text("work.md");
    h.sql("DROP TRIGGER fail_receipt;");
    h.ok(&[
        "host",
        "recover",
        "--key",
        "recover",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(h.text("work.md"), source);
    h.fields(
        "conflict",
        "human",
        "W",
        json!({"next_action":"An old suggestion"}),
    );
    let p = h.ok(&["host", "preview", "--input", "save.json"]);
    h.write(
        "work.md",
        &source.replace("Write the revised note", "New user content"),
    );
    let actual = h.text("work.md");
    assert!(
        !h.run(&[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-preview",
            p["preview"]["fingerprint"].as_str().unwrap(),
            "--expected-revision",
            &h.revision()
        ])
        .status
        .success()
    );
    assert_eq!(h.text("work.md"), actual);
}

#[test]
fn complex_or_conflicting_markdown_is_not_written() {
    let h = Host::new("| ID | title | status |\n| --- | --- | --- |\n| W | Title | planned |\n");
    h.write("work.md","| ID | title | status |\n| --- | --- | --- |\n| W | Title | planned |\n| W | Duplicate | planned |\n");
    let before = h.text("work.md");
    assert!(!h.run(&["source", "reindex"]).status.success());
    assert_eq!(h.text("work.md"), before);
    h.write(
        "work.md",
        "- [x] Conflict <!-- awr:id=\"W\" --> <!-- awr:status=\"planned\" -->\n",
    );
    assert!(!h.run(&["source", "reindex"]).status.success());
    let h = Host::new("- [ ] A title\n  continued here <!-- awr:id=\"W\" -->\n");
    assert_eq!(h.ok(&["object", "list", "work"])["total"], 1);
}

#[test]
fn ordinary_markdown_confirmation_uses_the_same_guarded_policy_and_receipt() {
    for text in [TABLE, LIST] {
        let h = Host::new(text);
        let policy = awr_core::OrdinaryWorkPolicy {
            version: 1,
            policy_id: "reading".into(),
            authorized_by: "fixture-user".into(),
            authorized_at: 1,
            reason: "Confirm selected reading work".into(),
            work_items: vec!["W".into()],
        };
        let mut config: toml::Value = toml::from_str(&h.text(".awr/project.toml")).unwrap();
        config["sources"][0]["options"] =
            toml::Value::try_from(json!({"ordinary_work_policy":policy})).unwrap();
        h.write(".awr/project.toml", &toml::to_string(&config).unwrap());
        let w = h.ok(&["work", "show", "W"]);
        h.envelope("confirm","human",json!({"operation":"confirm_ordinary","work":"W","source_fingerprint":w["source_ref"]["source_fingerprint"],"policy_fingerprint":policy.fingerprint().unwrap(),"kind":"user_confirmation","basis":"The reading notes answer the question","confirmed_at":awr_core::now_millis().unwrap(),"artifacts":[]}));
        h.save();
        let result = h.ok(&["intake", "inspect"]);
        assert_eq!(result["organization"]["user_confirmed_completed"], 1);
        assert_eq!(result["organization"]["verified_completed"], 0);
        if text == LIST {
            assert!(h.text("work.md").contains("- [x] Read notes"));
        }
    }
}

#[test]
fn markdown_engineering_completion_requires_actual_bound_reports() {
    use sha2::{Digest, Sha256};
    for text in [TABLE, LIST] {
        let h = Host::new(text);
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
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
        let session = s["session"]["id"].as_str().unwrap();
        h.ok(&[
            "work",
            "progress",
            "W",
            "--session",
            session,
            "--reason",
            "Prepare the note",
            "--next-action",
            "Review the note",
            "--expected-revision",
            &h.revision(),
        ]);
        h.write("complete.json",&json!({"version":1,"source_sha":sha,"acceptance":[{"criterion":"Useful note","evidence":["E"]}]}).to_string());
        assert!(
            !h.run(&[
                "work",
                "complete",
                "W",
                "--session",
                session,
                "--reason",
                "Review report",
                "--input",
                h.0.join("complete.json").to_str().unwrap(),
                "--expected-revision",
                &h.revision()
            ])
            .status
            .success()
        );
        let at = awr_core::now_millis().unwrap();
        let report=json!({"version":1,"work_item":"W","source_sha":sha,"command":"review fixture note","scope":["W"],"verified_at":at,"checks":[{"name":"note","passed":true,"details":"Reviewed the useful note in this synthetic fixture","criteria":["Useful note"]}]}).to_string();
        h.write("report.json", &report);
        h.write("evidence.json",&json!({"external_key":"E","work_item_key":"W","evidence_type":"completion_report","level":"locally_verified","summary":"Checked fixture note","locator":"report.json","sha256":format!("{:x}",Sha256::digest(report.as_bytes())),"source_sha":sha,"command":"review fixture note","scope":["W"],"verified_at":at}).to_string());
        h.ok(&[
            "evidence",
            "add",
            "--input",
            "evidence.json",
            "--expected-revision",
            &h.revision(),
        ]);
        h.ok(&[
            "work",
            "complete",
            "W",
            "--session",
            session,
            "--reason",
            "Every criterion has a checked report",
            "--input",
            h.0.join("complete.json").to_str().unwrap(),
            "--expected-revision",
            &h.revision(),
        ]);
        assert_eq!(h.ok(&["work", "show", "W"])["work"]["status"], "completed");
        assert_eq!(
            h.ok(&["intake", "inspect", "--source-sha", sha])["organization"]["verified_completed"],
            1
        );
    }
}
