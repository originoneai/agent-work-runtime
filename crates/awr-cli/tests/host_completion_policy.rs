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
    fn policy(&self, keys: &[&str]) -> awr_core::OrdinaryWorkPolicy {
        let policy = awr_core::OrdinaryWorkPolicy {
            version: 1,
            policy_id: "reading-v1".into(),
            authorized_by: "fixture-owner".into(),
            authorized_at: 1,
            reason: "Use ordinary confirmations only for the selected reading work".into(),
            work_items: keys.iter().map(|s| s.to_string()).collect(),
        };
        let mut mapping: toml::Value = toml::from_str(&self.text(".awr/project.toml")).unwrap();
        mapping["sources"][0]["options"] =
            toml::Value::try_from(json!({"ordinary_work_policy":policy})).unwrap();
        self.write(".awr/project.toml", &toml::to_string(&mapping).unwrap());
        self.ok(&["source", "reindex"]);
        policy
    }
    fn confirm(
        &self,
        key: &str,
        work: &str,
        policy: &awr_core::OrdinaryWorkPolicy,
        origin: &str,
        kind: &str,
        artifacts: Value,
    ) {
        let w = self.ok(&["work", "show", work]);
        self.envelope(key,origin,json!({"operation":"confirm_ordinary","work":work,"source_fingerprint":w["source_ref"]["source_fingerprint"],"policy_fingerprint":policy.fingerprint().unwrap(),"kind":kind,"basis":"Read the note and confirmed the useful ideas are captured","confirmed_at":awr_core::now_millis().unwrap(),"artifacts":artifacts}));
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
    fn inspect(&self) -> Value {
        self.ok(&["intake", "inspect"])["organization"].clone()
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn ordinary_non_git_work_confirms_with_provenance_and_separate_counts() {
    let h = Host::new();
    let p = h.policy(&["W"]);
    // Remove the unrelated draft from this synthetic scope, not from runtime history.
    h.write(
        "work.yaml",
        h.text("work.yaml").split("- id: D").next().unwrap(),
    );
    let ready = h.inspect();
    assert_eq!(ready["business_execution_ready"], true);
    let ctx = h.ok(&["context", "compile", "--work", "W", "--detached"]);
    assert!(
        ctx["work_context"]["rendered_context"]
            .as_str()
            .unwrap()
            .contains("Ordinary work policy")
    );
    h.confirm(
        "confirm-reading",
        "W",
        &p,
        "human",
        "user_confirmation",
        json!([]),
    );
    let saved = h.save();
    assert_eq!(saved["status"], "completed");
    let work = h.ok(&["work", "show", "W"]);
    let receipt = &work["work"]["ordinary_completion"];
    assert_eq!(receipt["policy"], serde_json::to_value(&p).unwrap());
    assert_eq!(receipt["actor"]["origin"], "human");
    assert_eq!(receipt["kind"], "user_confirmation");
    assert!(receipt["confirmed_at"].as_i64().unwrap() > 0);
    let report = h.inspect();
    assert_eq!(report["state"], "completed_under_policy");
    assert_eq!(report["user_confirmed_completed"], 1);
    assert_eq!(report["verified_completed"], 0);
    let rev = h.revision();
    assert_eq!(h.save()["already_recorded"], true);
    assert_eq!(h.revision(), rev);
    assert_eq!(h.ok(&["session", "list"])["sessions"], json!([]));
    assert!(!h.0.join(".git").exists());
}

#[test]
fn strict_default_scope_and_user_origin_cannot_be_downgraded_by_a_work_action() {
    let h = Host::new();
    let p = awr_core::OrdinaryWorkPolicy {
        version: 1,
        policy_id: "unregistered".into(),
        authorized_by: "user".into(),
        authorized_at: 1,
        reason: "fixture".into(),
        work_items: vec!["W".into()],
    };
    let before = h.text("work.yaml");
    h.confirm("strict", "W", &p, "human", "user_confirmation", json!([]));
    h.error(
        &[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
        ],
        "RuleViolation",
    );
    assert_eq!(h.text("work.yaml"), before);
    let p = h.policy(&["D"]);
    h.confirm(
        "out-of-scope",
        "W",
        &p,
        "human",
        "user_confirmation",
        json!([]),
    );
    h.error(
        &["host", "preview", "--input", "save.json"],
        "RuleViolation",
    );
    let p = h.policy(&["W"]);
    h.confirm(
        "false-human",
        "W",
        &p,
        "delegated_agent",
        "user_confirmation",
        json!([]),
    );
    h.error(
        &["host", "preview", "--input", "save.json"],
        "RuleViolation",
    );
    let w = h.ok(&["work", "show", "W"]);
    h.envelope("forged","human",json!({"operation":"fields","kind":"work_item","target":"W","source_fingerprint":w["source_ref"]["source_fingerprint"],"fields":{"ordinary_completion":{"basis":"user agreed"},"status":"completed"}}));
    h.error(
        &["host", "preview", "--input", "save.json"],
        "MutationUnsupported",
    );
    assert_eq!(h.text("work.yaml"), before);
}

#[test]
fn actual_business_artifacts_are_checked_and_drift_invalidates_the_current_confirmation() {
    use sha2::{Digest, Sha256};
    let h = Host::new();
    let p = h.policy(&["W"]);
    h.write("note.md", "Useful notes\n");
    let artifacts =
        json!([{"locator":"note.md","sha256":format!("{:x}",Sha256::digest(b"Useful notes\n"))}]);
    h.confirm(
        "checked",
        "W",
        &p,
        "delegated_agent",
        "business_check",
        artifacts,
    );
    let preview = h.ok(&["host", "preview", "--input", "save.json"]);
    h.write("note.md", "new contents\n");
    assert!(
        !h.run(&[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
            "--expected-preview",
            preview["preview"]["fingerprint"].as_str().unwrap()
        ])
        .status
        .success()
    );
    assert!(!h.text("work.yaml").contains("ordinary_completion:"));
    h.write("note.md", "Useful notes\n");
    let preview = h.ok(&["host", "preview", "--input", "save.json"]);
    h.ok(&[
        "host",
        "save",
        "--input",
        "save.json",
        "--expected-revision",
        &h.revision(),
        "--expected-preview",
        preview["preview"]["fingerprint"].as_str().unwrap(),
    ]);
    assert_eq!(h.inspect()["business_checked_completed"], 1);
    assert_eq!(h.inspect()["verified_completed"], 0);
    h.write("note.md", "changed after confirmation\n");
    assert_eq!(h.inspect()["business_checked_completed"], 0);
    assert_eq!(
        h.ok(&["host", "status", "--key", "checked"])["historical_outcome"],
        true
    );
}

#[test]
fn policy_changes_invalidate_previews_and_do_not_promote_source_completion() {
    let h = Host::new();
    let p = h.policy(&["W"]);
    h.confirm("reviewed", "W", &p, "human", "user_confirmation", json!([]));
    let preview = h.ok(&["host", "preview", "--input", "save.json"]);
    h.policy(&["W", "D"]);
    assert!(
        !h.run(&[
            "host",
            "save",
            "--input",
            "save.json",
            "--expected-revision",
            &h.revision(),
            "--expected-preview",
            preview["preview"]["fingerprint"].as_str().unwrap()
        ])
        .status
        .success()
    );
    h.write(
        "work.yaml",
        &h.text("work.yaml")
            .replace("status: planned", "status: completed"),
    );
    let report = h.inspect();
    assert_eq!(report["source_completed"], 1);
    assert_eq!(report["user_confirmed_completed"], 0);
    assert_eq!(report["verified_completed"], 0);
}

#[test]
fn active_execution_and_unresolved_dependencies_prevent_host_confirmation() {
    let h = Host::new();
    let p = h.policy(&["W"]);
    h.write(
        "work.yaml",
        &h.text("work.yaml")
            .replace("unknown: keep-me", "unknown: keep-me\n  depends_on: [D]"),
    );
    h.confirm("blocked", "W", &p, "human", "user_confirmation", json!([]));
    assert!(
        !h.run(&["host", "preview", "--input", "save.json"])
            .status
            .success()
    );
    h.write(
        "work.yaml",
        &h.text("work.yaml").replace("  depends_on: [D]\n", ""),
    );
    h.ok(&["work", "show", "W"]);
    h.ok(&[
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
    h.confirm("occupied", "W", &p, "human", "user_confirmation", json!([]));
    h.error(
        &["host", "preview", "--input", "save.json"],
        "ClaimConflict",
    );
}

#[test]
fn reopening_clears_current_confirmation_and_keeps_the_historical_receipt() {
    let h = Host::new();
    let p = h.policy(&["W"]);
    h.confirm(
        "first-done",
        "W",
        &p,
        "human",
        "user_confirmation",
        json!([]),
    );
    h.save();
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
        "--expected-revision",
        &h.revision(),
    ]);
    h.ok(&[
        "work",
        "reopen",
        "W",
        "--session",
        s["session"]["id"].as_str().unwrap(),
        "--reason",
        "New reading questions",
        "--next-action",
        "Review the follow-up questions",
        "--expected-revision",
        &h.revision(),
    ]);
    let w = h.ok(&["work", "show", "W"]);
    assert_eq!(w["work"]["status"], "planned");
    assert_eq!(w["work"]["ordinary_completion"], Value::Null);
    assert_eq!(
        h.ok(&["host", "status", "--key", "first-done"])["historical_outcome"],
        true
    );
    assert_eq!(h.inspect()["user_confirmed_completed"], 0);
}

#[test]
fn source_receipt_tampering_and_policy_changes_cannot_reclassify_history() {
    let h = Host::new();
    let p = h.policy(&["W"]);
    h.confirm("honest", "W", &p, "human", "user_confirmation", json!([]));
    h.save();
    assert_eq!(h.inspect()["user_confirmed_completed"], 1);
    h.policy(&["W", "D"]);
    assert_eq!(h.inspect()["user_confirmed_completed"], 0);
    h.policy(&["W"]);
    assert_eq!(h.inspect()["user_confirmed_completed"], 1);
    h.write(
        "work.yaml",
        &h.text("work.yaml").replace(
            "Read the note and confirmed the useful ideas are captured",
            "Different claimed confirmation",
        ),
    );
    assert_eq!(h.inspect()["user_confirmed_completed"], 0);
    assert_eq!(h.inspect()["verified_completed"], 0);
}
