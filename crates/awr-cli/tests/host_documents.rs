//! Native document fixtures. Journal rewinds model persisted crash boundaries explicitly.
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
        let h = Self(std::env::temp_dir().join(format!("awr 文档 {}", Id::new())));
        fs::create_dir(&h.0).unwrap();
        fs::create_dir(h.0.join("drafts")).unwrap();
        h.write("GOAL.md", text);
        h.write("work.yaml", "work_items: []\n");
        h.write("mapping.toml","[project]\nname='Documents'\ncontext_profile='minimal'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='GOAL.md'\nadapter='markdown-heading-v1'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='decisions'\nrole='primary'\npath='drafts'\nadapter='markdown-directory-v1'\n");
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
    fn error(&self, args: &[&str], code: &str) {
        let o = self.run(args);
        assert!(!o.status.success(), "{args:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&o.stderr).unwrap()["code"],
            code,
            "{}",
            String::from_utf8_lossy(&o.stderr)
        )
    }
    fn revision(&self) -> String {
        self.ok(&["session", "list"])["project_revision"].to_string()
    }
    fn request(&self, key: &str, edit: Value) {
        let sources = self.ok(&["object", "list", "source"]);
        let source = sources["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["domain"] == "goal")
            .unwrap()
            .clone();
        self.write("edit.json",&json!({"version":1,"request_key":key,"change":{"operation":"edit","source_id":source["id"],"source_fingerprint":source["fingerprint"],"edit":edit}}).to_string());
    }
    fn draft(&self, key: &str, path: &str, body: &str) {
        self.write("edit.json",&json!({"version":1,"request_key":key,"change":{"operation":"create_draft","path":path,"title":"Review approach","body":body}}).to_string())
    }
    fn preview(&self) -> Value {
        self.ok(&["document", "change", "--input", "edit.json"])
    }
    fn args<'a>(&'a self, p: &'a Value) -> Vec<String> {
        vec![
            "document".into(),
            "change".into(),
            "--input".into(),
            "edit.json".into(),
            "--accept".into(),
            "--expected-preview".into(),
            p["preview"]["fingerprint"].as_str().unwrap().into(),
            "--expected-revision".into(),
            p["project_revision"].to_string(),
        ]
    }
    fn accept(&self, p: &Value) -> Value {
        let a = self.args(p);
        self.ok(&a.iter().map(String::as_str).collect::<Vec<_>>())
    }
    fn stored(&self, r: &Value) -> (PathBuf, Value) {
        let p = self
            .0
            .join(r["recovery_directory"].as_str().unwrap())
            .join("receipt.json");
        let v = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        (p, v)
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn exact_fragment_keeps_crlf_code_and_other_sections_and_replays_without_new_identity() {
    let before = "# Goal {#goal}\r\n\r\n原文说明。\r\n\r\n```text\r\n# embedded instruction\r\n```\r\n\r\n# Notes {#notes}\r\n\r\nKeep exactly.\r\n";
    let h = Host::new(before);
    let prior = h.ok(&["object", "list", "goal"]);
    h.request(
        "edit-once",
        json!({"kind":"fragment","before":"原文说明。","after":"更新说明。"}),
    );
    let p = h.preview();
    assert_eq!(h.text("GOAL.md"), before);
    let saved = h.accept(&p);
    assert_eq!(saved["status"], "completed");
    assert_eq!(
        h.text("GOAL.md"),
        before.replace("原文说明。", "更新说明。")
    );
    let rev = h.revision();
    let replay = h.accept(&p);
    assert_eq!(replay["source_id"], saved["source_id"]);
    assert_eq!(h.revision(), rev);
    let next = h.ok(&["object", "list", "goal"]);
    let ids = |v: &Value| {
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&prior), ids(&next));
    assert_eq!(h.ok(&["session", "list"])["sessions"], json!([]));
    assert_eq!(
        h.ok(&["document", "status", "--key", "edit-once"])["current_source"],
        "after"
    );
}
#[test]
fn whole_body_edit_retains_explicit_keys_and_lifecycle_but_accepts_changed_prose() {
    let h = Host::new("# Goal {#goal status=active}\n\nBefore\n");
    h.request("whole",json!({"kind":"replace","text":"# Better goal {#goal status=active}\n\nNew success criteria:\n- Ready to read.\n"}));
    h.accept(&h.preview());
    assert!(h.text("GOAL.md").contains("New success criteria"));
    h.request(
        "status-forgery",
        json!({"kind":"replace","text":"# Better goal {#goal status=completed}\n\nDone\n"}),
    );
    h.error(
        &["document", "change", "--input", "edit.json"],
        "RuleViolation",
    );
    h.request(
        "key-change",
        json!({"kind":"replace","text":"# Other key {#different status=active}\n\nBody\n"}),
    );
    h.error(
        &["document", "change", "--input", "edit.json"],
        "RuleViolation",
    );
}
#[test]
fn stale_preview_ambiguous_fragment_and_no_change_have_explicit_outcomes() {
    let before = "# Goal {#goal}\n\nSame Same\n";
    let h = Host::new(before);
    h.request(
        "ambiguous",
        json!({"kind":"fragment","before":"Same","after":"Other"}),
    );
    h.error(
        &["document", "change", "--input", "edit.json"],
        "SourceConflict",
    );
    h.request(
        "stale",
        json!({"kind":"replace","text":before.replace("Same Same","New")}),
    );
    let p = h.preview();
    h.write("GOAL.md", &format!("{before}\nExternal\n"));
    let args = h.args(&p);
    h.error(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        "RevisionConflict",
    );
    assert!(h.text("GOAL.md").contains("External"));
    h.request(
        "no-change",
        json!({"kind":"replace","text":h.text("GOAL.md")}),
    );
    let p = h.preview();
    let rev = h.revision();
    let r = h.accept(&p);
    assert_eq!(r["status"], "no_change");
    assert_eq!(r["source_write_performed"], false);
    assert_eq!(h.revision(), rev);
}
#[test]
fn new_draft_has_stable_source_identity_and_never_clobbers_existing_files() {
    let h = Host::new("# Goal {#goal}\n\nKeep\n");
    h.draft("draft-once", "drafts/方案.md", "A considered approach.\n");
    let p = h.preview();
    assert!(!h.0.join("drafts/方案.md").exists());
    let saved = h.accept(&p);
    let content = h.text("drafts/方案.md");
    assert!(content.contains("status: proposed"));
    assert_eq!(h.accept(&p)["source_id"], saved["source_id"]);
    h.draft("draft-once", "drafts/方案.md", "Changed payload");
    h.error(
        &["document", "change", "--input", "edit.json"],
        "SourceConflict",
    );
    h.draft("second", "drafts/方案.md", "Different");
    h.error(
        &["document", "change", "--input", "edit.json"],
        "SourceConflict",
    );
    assert_eq!(h.text("drafts/方案.md"), content);
    h.draft("outside", "outside.md", "Outside");
    h.error(
        &["document", "change", "--input", "edit.json"],
        "SourceConflict",
    );
    h.draft("escape", "../escape.md", "Outside");
    h.error(
        &["document", "change", "--input", "edit.json"],
        "RuleViolation",
    );
    h.draft(
        "claimed-accepted",
        "drafts/invalid.md",
        "- Status: Accepted\n\nBody\n",
    );
    h.error(
        &["document", "change", "--input", "edit.json"],
        "SourceConflict",
    );
    assert_eq!(h.ok(&["object", "list", "decision"])["total"], 1);
}
#[test]
fn conflicting_declarations_aliases_and_ledger_edits_do_not_write() {
    let before = "---\nstatus: active\n---\n# Goal {#goal status=active}\n\nBefore\n";
    let h = Host::new(before);
    h.request(
        "contradiction",
        json!({"kind":"replace","text":before.replace("status: active","status: completed")}),
    );
    h.error(
        &["document", "change", "--input", "edit.json"],
        "SourceConflict",
    );
    assert_eq!(h.text("GOAL.md"), before);
    let sources = h.ok(&["object", "list", "source"]);
    let source = sources["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["domain"] == "ledger")
        .unwrap();
    h.write("edit.json", &json!({"version":1,"request_key":"ledger-bypass","change":{"operation":"edit","source_id":source["id"],"source_fingerprint":source["fingerprint"],"edit":{"kind":"replace","text":"work_items: []\n"}}}).to_string());
    h.error(
        &["document", "change", "--input", "edit.json"],
        "MutationUnsupported",
    );
    h.request(
        "alias",
        json!({"kind":"fragment","before":"Before","after":"After"}),
    );
    h.write(".awr/project.toml",&(h.text(".awr/project.toml")+"\n[[sources]]\ndomain='plan'\nrole='supporting'\npath='GOAL.md'\nadapter='markdown-heading-v1'\n"));
    h.error(
        &["document", "change", "--input", "edit.json"],
        "SourceConflict",
    );
    assert_eq!(h.text("GOAL.md"), before);
}
#[test]
fn persisted_recovery_retains_third_party_bytes_and_rejects_foreign_receipts() {
    let before = "# Goal {#goal}\n\nBefore\n";
    let h = Host::new(before);
    h.request(
        "recover",
        json!({"kind":"fragment","before":"Before","after":"After"}),
    );
    let p = h.preview();
    let saved = h.accept(&p);
    let after = h.text("GOAL.md");
    let (path, mut receipt) = h.stored(&saved);
    receipt["phase"] = json!("applied");
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let r = h.ok(&[
        "document",
        "recover",
        "--key",
        "recover",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(r["source_write_performed"], false);
    assert_eq!(r["source_id"], saved["source_id"]);
    receipt["phase"] = json!("prepared");
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    h.write("GOAL.md", before);
    let recovered = h.ok(&[
        "document",
        "recover",
        "--key",
        "recover",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(recovered["source_write_performed"], true);
    assert_eq!(h.text("GOAL.md"), after);
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    h.write("GOAL.md", "External newer document\n");
    h.error(
        &[
            "document",
            "recover",
            "--key",
            "recover",
            "--expected-revision",
            &h.revision(),
        ],
        "SourceConflict",
    );
    assert_eq!(h.text("GOAL.md"), "External newer document\n");
    h.write("GOAL.md", &after);
    receipt["plan"]["root"] = json!("/foreign/project");
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    h.error(
        &["document", "status", "--key", "recover"],
        "SourceConflict",
    );
}

#[test]
fn architecture_kind_and_readonly_or_concurrent_destinations_are_explicit() {
    let h = Host::new("# Architecture {#architecture}\n\nExecution structure.\n");
    let manifest =
        h.text(".awr/project.toml")
            .replacen("domain = \"goal\"", "domain = \"plan\"", 1);
    // The accepted manifest retains TOML formatting; parse it instead of depending on whitespace.
    let mut config: toml::Value = toml::from_str(&manifest).unwrap();
    config["sources"][0]["domain"] = toml::Value::String("plan".into());
    config["sources"][0]["options"] = toml::Value::Table(toml::Table::from_iter([(
        "kind".into(),
        toml::Value::String("architecture".into()),
    )]));
    h.write(".awr/project.toml", &toml::to_string(&config).unwrap());
    let plans = h.ok(&["object", "list", "plan"]);
    assert_eq!(plans["items"][0]["kind"], "architecture");
    h.draft("concurrent", "drafts/contended.md", "Reviewed draft\n");
    let p = h.preview();
    h.write("drafts/contended.md", "# Existing\n\nConcurrent contents\n");
    let args = h.args(&p);
    let out = h.run(&args.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(!out.status.success());
    assert_eq!(
        h.text("drafts/contended.md"),
        "# Existing\n\nConcurrent contents\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let s = h.ok(&["object", "list", "source"]);
        let source = s["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["domain"] == "plan")
            .unwrap();
        h.write("edit.json",&json!({"version":1,"request_key":"readonly","change":{"operation":"edit","source_id":source["id"],"source_fingerprint":source["fingerprint"],"edit":{"kind":"fragment","before":"Execution structure.","after":"Changed."}}}).to_string());
        let p = h.preview();
        fs::set_permissions(h.0.join("GOAL.md"), fs::Permissions::from_mode(0o444)).unwrap();
        let args = h.args(&p);
        h.error(
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            "RuleViolation",
        );
        assert!(h.text("GOAL.md").contains("Execution structure."));
        fs::set_permissions(h.0.join("GOAL.md"), fs::Permissions::from_mode(0o600)).unwrap();
    }
}
