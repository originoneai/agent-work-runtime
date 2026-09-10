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
    fn new() -> Self {
        let h = Self(std::env::temp_dir().join(format!("awr 一次 保存 {}", Id::new())));
        fs::create_dir(&h.0).unwrap();
        h.write("work.yaml","# keep this comment\ngoals:\n- id: G\n  title: Reading project\n  status: active\n  summary: Prepare useful reading notes.\nwork_items:\n- id: W\n  title: Read an article\n  status: planned\n  goal: G\n  acceptance: [A useful note is available]\n  next_action: Write the note\n  unknown: keep-me\n- id: D\n  title: Draft task\n  status: draft\n");
        h.write(
            "GOAL.md",
            "# Reading {#reading}\n\nKeep the useful ideas.\n",
        );
        h.write("mapping.toml","[project]\nname='Host save'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='goal'\nrole='supporting'\npath='GOAL.md'\nadapter='markdown-heading-v1'\n");
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
    fn stored(&self, r: &Value) -> (PathBuf, Value) {
        let path = self
            .0
            .join(r["recovery_directory"].as_str().unwrap())
            .join("receipt.json");
        let value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        (path, value)
    }
    fn sql(&self, sql: &str) {
        rusqlite::Connection::open(self.0.join(".awr/state.db"))
            .unwrap()
            .execute_batch(sql)
            .unwrap()
    }
}

#[test]
fn delegated_edits_keep_the_actual_agent_origin_and_require_exact_review() {
    let h = Host::new();
    h.fields(
        "delegated-edit",
        "delegated_agent",
        "W",
        json!({"next_action":"Review the delegated draft"}),
    );
    let mut input: Value = serde_json::from_str(&h.text("save.json")).unwrap();
    input["actor"]["subject"] = json!("fixture-delegated-agent");
    h.write("save.json", &input.to_string());
    h.error(
        &[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
        ],
        "SourceConflict",
    );
    let p = h.ok(&["host", "preview", "--input", "save.json"]);
    let saved = h.ok(&[
        "host",
        "save",
        "--input",
        "save.json",
        "--expected-revision",
        &h.revision(),
        "--expected-preview",
        p["preview"]["fingerprint"].as_str().unwrap(),
    ]);
    assert_eq!(saved["actor"]["origin"], "delegated_agent");
    assert_eq!(saved["actor"]["subject"], "fixture-delegated-agent");
    assert_eq!(h.ok(&["session", "list"])["sessions"], json!([]));
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn one_human_save_preserves_bytes_has_no_fake_agent_and_reuses_one_proposal() {
    let h = Host::new();
    let before = h.text("work.yaml");
    h.fields(
        "human-edit",
        "human",
        "W",
        json!({"title":"Read and annotate the article"}),
    );
    let revision = h.revision();
    let saved = h.save();
    assert_eq!(saved["status"], "completed");
    assert_eq!(
        h.text("work.yaml"),
        before.replace(
            "title: Read an article",
            "title: Read and annotate the article"
        )
    );
    let current = h.revision();
    let replay = h.ok(&[
        "host",
        "save",
        "--input",
        "save.json",
        "--expected-revision",
        &revision,
    ]);
    assert_eq!(replay["proposal_id"], saved["proposal_id"]);
    assert_eq!(h.revision(), current);
    assert_eq!(h.ok(&["session", "list"])["sessions"], json!([]));
    let status = h.ok(&["host", "status", "--key", "human-edit"]);
    assert_eq!(status["actor"]["subject"], "fixture-user");
    assert_eq!(status["provenance_is_authentication"], false);
    assert!(status["current_proposal"]["created_by_session"].is_null());
    assert_eq!(
        status["current_proposal"]["patch"]["host_edit"]["request_key"],
        "human-edit"
    );
    assert_eq!(h.revision(), current);
    h.fields(
        "human-edit",
        "human",
        "W",
        json!({"title":"A different edit"}),
    );
    h.error(
        &[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
        ],
        "SourceConflict",
    );
}
#[test]
fn ai_requires_the_exact_preview_and_changed_source_or_target_cannot_be_applied() {
    let h = Host::new();
    h.fields(
        "ai-edit",
        "ai_accepted",
        "W",
        json!({"next_action":"Review the saved note"}),
    );
    h.error(
        &[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
        ],
        "SourceConflict",
    );
    let p = h.ok(&["host", "preview", "--input", "save.json"]);
    let fp = p["preview"]["fingerprint"].as_str().unwrap();
    let saved = h.ok(&[
        "host",
        "save",
        "--input",
        "save.json",
        "--expected-revision",
        &p["project_revision"].to_string(),
        "--expected-preview",
        fp,
    ]);
    assert_eq!(saved["status"], "completed");
    h.fields(
        "ai-stale",
        "ai_accepted",
        "W",
        json!({"title":"Reviewed title"}),
    );
    let p = h.ok(&["host", "preview", "--input", "save.json"]);
    h.write("work.yaml", &(h.text("work.yaml") + "# external edit\n"));
    let newer = h.text("work.yaml");
    h.error(
        &[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &p["project_revision"].to_string(),
            "--expected-preview",
            p["preview"]["fingerprint"].as_str().unwrap(),
        ],
        "SourceConflict",
    );
    assert_eq!(h.text("work.yaml"), newer);
    h.fields(
        "ai-tampered",
        "ai_accepted",
        "W",
        json!({"title":"Reviewed title"}),
    );
    let preview = h.ok(&["host", "preview", "--input", "save.json"]);
    let mut request: Value = serde_json::from_str(&h.text("save.json")).unwrap();
    request["change"]["fields"]["title"] = json!("Unreviewed replacement");
    h.write("save.json", &request.to_string());
    h.error(
        &[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
            "--expected-preview",
            preview["preview"]["fingerprint"].as_str().unwrap(),
        ],
        "SourceConflict",
    );
}
#[test]
fn unchanged_saves_do_not_emit_business_events_and_human_labels_cannot_complete_work() {
    let h = Host::new();
    h.fields(
        "unchanged",
        "human",
        "W",
        json!({"title":"Read an article"}),
    );
    let revision = h.revision();
    let source = h.text("work.yaml");
    let r = h.save();
    assert_eq!(r["status"], "no_change");
    assert_eq!(r["source_write_performed"], false);
    assert_eq!(h.revision(), revision);
    assert_eq!(h.text("work.yaml"), source);
    for fields in [
        json!({"status":"completed"}),
        json!({"owner":"pretend-agent"}),
        json!({"evidence_level":"externally_verified"}),
        json!({"id":"replaced"}),
    ] {
        h.fields(
            &format!(
                "forgery-{}",
                fields.as_object().unwrap().keys().next().unwrap()
            ),
            "human",
            "W",
            fields,
        );
        let out = h.run(&[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
        ]);
        assert!(!out.status.success());
        assert_eq!(h.text("work.yaml"), source);
    }
}
#[test]
fn complete_draft_declarations_can_activate_without_invented_session_or_completion() {
    let h = Host::new();
    assert!(awr_core::is_domain_event_type("work.draft_activated"));
    let old = h.revision();
    let forged = h.run(&[
        "event",
        "append",
        "--type",
        "work.draft_activated",
        "--summary",
        "Pretend activation",
        "--expected-revision",
        &old,
    ]);
    assert!(!forged.status.success());
    assert_eq!(h.revision(), old);
    h.activate("incomplete", "D");
    let out = h.run(&[
        "host",
        "save",
        "--input",
        "save.json",
        "--expected-revision",
        &h.revision(),
    ]);
    assert!(!out.status.success());
    assert_eq!(h.ok(&["work", "show", "D"])["work"]["status"], "draft");
    h.fields("fill-draft","human","D",json!({"acceptance":["A useful note is available"],"next_action":"Prepare the note","goal":"G"}));
    h.save();
    h.fields("blocked-draft", "human", "D", json!({"depends_on":["W"]}));
    h.save();
    h.activate("blocked-activation", "D");
    h.error(
        &[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
        ],
        "DependencyBlocked",
    );
    h.fields("clear-dependency", "human", "D", json!({"depends_on":[]}));
    h.save();
    assert_eq!(h.ok(&["work", "show", "D"])["work"]["ready"], false);
    h.activate("activate-draft", "D");
    let r = h.save();
    assert_eq!(r["status"], "completed");
    let work = h.ok(&["work", "show", "D"]);
    assert_eq!(work["work"]["status"], "planned");
    assert_eq!(work["work"]["ready"], true);
    assert_eq!(h.ok(&["session", "list"])["sessions"], json!([]));
    h.fields(
        "edit-after-activation",
        "human",
        "D",
        json!({"next_action":"Review the activated draft"}),
    );
    assert_eq!(h.save()["status"], "completed");
    h.activate("repeat-transition", "D");
    h.error(
        &[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
        ],
        "InvalidTransition",
    );
}
#[test]
fn document_saves_share_exact_writers_and_record_host_provenance() {
    let h = Host::new();
    let sources = h.ok(&["object", "list", "source"]);
    let source = sources["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["domain"] == "goal")
        .unwrap();
    let change = json!({"operation":"document","change":{"operation":"edit","source_id":source["id"],"source_fingerprint":source["fingerprint"],"edit":{"kind":"fragment","before":"Keep the useful ideas.","after":"Keep the useful ideas and references."}}});
    h.envelope("document-human", "human", change);
    let r = h.save();
    assert_eq!(r["status"], "completed");
    assert!(h.text("GOAL.md").contains("and references"));
    let rev = h.revision();
    assert_eq!(h.save()["source_write_performed"], false);
    assert_eq!(h.revision(), rev);
    let status = h.ok(&["host", "status", "--key", "document-human"]);
    assert_eq!(status["current_document"]["current_source"], "after");
    assert_eq!(status["actor"]["host"], "fixture-desktop");
}
#[test]
fn interrupted_receipt_is_queryable_and_explicit_recovery_does_not_repeat_source_write() {
    let h = Host::new();
    h.fields(
        "receipt-failure",
        "human",
        "W",
        json!({"title":"Recover the note"}),
    );
    h.sql("CREATE TRIGGER fail_host_receipt BEFORE INSERT ON events WHEN NEW.event_type='proposal.applied' BEGIN SELECT RAISE(ABORT,'fixture final receipt failure'); END;");
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
    assert!(h.text("work.yaml").contains("Recover the note"));
    let status = h.ok(&["host", "status", "--key", "receipt-failure"]);
    assert_eq!(status["found"], true);
    h.sql("DROP TRIGGER fail_host_receipt;");
    let r = h.ok(&[
        "host",
        "recover",
        "--key",
        "receipt-failure",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(r["status"], "completed");
    assert_eq!(r["source_write_performed"], false);
    assert_eq!(r["proposal_id"], partial["proposal_id"]);
    let (path, mut receipt) = h.stored(&r);
    receipt["phase"] = json!("prepared");
    receipt["proposal_id"] = Value::Null;
    fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let before = h.text("work.yaml");
    let r = h.ok(&[
        "host",
        "recover",
        "--key",
        "receipt-failure",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(r["source_write_performed"], false);
    assert_eq!(h.text("work.yaml"), before);
}
#[test]
fn recovery_preserves_external_edits_and_request_keys_are_project_scoped() {
    let h = Host::new();
    let other = Host::new();
    for host in [&h, &other] {
        host.fields(
            "same-request",
            "human",
            "W",
            json!({"title":"Project-local edit"}),
        );
    }
    let a = h.save();
    let b = other.save();
    assert_ne!(a["proposal_id"], b["proposal_id"]);
    h.fields(
        "conflicted-recovery",
        "human",
        "W",
        json!({"title":"Incomplete edit"}),
    );
    h.sql("CREATE TRIGGER fail_host_receipt BEFORE INSERT ON events WHEN NEW.event_type='proposal.applied' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;");
    let out = h.run(&[
        "host",
        "save",
        "--input",
        "save.json",
        "--expected-revision",
        &h.revision(),
    ]);
    assert!(!out.status.success());
    h.sql("DROP TRIGGER fail_host_receipt;");
    h.write("work.yaml", &(h.text("work.yaml") + "# newer user edits\n"));
    let newer = h.text("work.yaml");
    let out = h.run(&[
        "host",
        "recover",
        "--key",
        "conflicted-recovery",
        "--expected-revision",
        &h.revision(),
    ]);
    assert!(!out.status.success());
    assert_eq!(h.text("work.yaml"), newer);
}
