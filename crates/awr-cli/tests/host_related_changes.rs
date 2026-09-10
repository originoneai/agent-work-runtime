//! Native related-change fixtures. Explicit journal rewinds model persisted process-crash boundaries.
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
        let h = Self(std::env::temp_dir().join(format!("awr 采纳 {}", Id::new())));
        fs::create_dir(&h.0).unwrap();
        fs::create_dir(h.0.join("decisions")).unwrap();
        h.write("GOAL.md", "# Goal {#g status=active}\n\nInitial goal.\n");
        h.write("PLAN.md", "# Plan {#p status=active}\n\nInitial plan.\n");
        h.write("work.yaml","work_items:\n- id: W\n  title: Read article\n  status: planned\n  next_action: Read initial plan\n");
        h.write("decisions/old.md","---\nid: OLD\ntitle: Earlier approach\nstatus: accepted\ncustom: retain\n---\n# Earlier approach\n\nKeep the original decision and its reasons.\n");
        h.write("decisions/new.md","---\nid: NEW\ntitle: Revised approach\nstatus: proposed\n---\n# Revised approach\n\nUse the reviewed approach.\n");
        h.write("mapping.toml","[project]\nname='Related changes'\ncontext_profile='minimal'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='GOAL.md'\nadapter='markdown-heading-v1'\n[[sources]]\ndomain='plan'\nrole='primary'\npath='PLAN.md'\nadapter='markdown-heading-v1'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='decisions'\nrole='primary'\npath='decisions'\nadapter='markdown-directory-v1'\n");
        h.ok(&["init", "--manifest", "mapping.toml", "--accept"]);
        h
    }
    fn write(&self, p: &str, s: &str) {
        fs::write(self.0.join(p), s).unwrap()
    }
    fn text(&self, p: &str) -> String {
        fs::read_to_string(self.0.join(p)).unwrap()
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
    fn source(&self, path: &str) -> Value {
        self.ok(&["source", "list"])["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["locator"].as_str().unwrap().ends_with(path))
            .unwrap()
            .clone()
    }
    fn reference(&self, path: &str, key: &str) -> Value {
        let s = self.source(path);
        json!({"source_id":s["id"],"external_key":key,"source_fingerprint":s["fingerprint"]})
    }
    fn edit(&self, path: &str, before: &str, after: &str) -> Value {
        let s = self.source(path);
        json!({"operation":"document","source_id":s["id"],"source_fingerprint":s["fingerprint"],"edit":{"kind":"fragment","before":before,"after":after}})
    }
    fn adopt(&self) -> Value {
        json!({"operation":"adopt","candidate":self.reference("decisions/new.md","NEW"),"supersedes":self.reference("decisions/old.md","OLD")})
    }
    fn request(&self, key: &str, changes: Value) {
        self.write("batch.json",&json!({"version":1,"request_key":key,"actor":{"host":"fixture","subject":"reviewer","origin":"human"},"reason":"Adopt the reviewed project approach","change":{"kind":"related","changes":changes}}).to_string())
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
    fn decision(&self, key: &str) -> Value {
        self.ok(&["decision", "show", key, "--full"])
    }
    fn stored(&self, r: &Value) -> (PathBuf, Value) {
        let path = self
            .0
            .join(r["recovery_directory"].as_str().unwrap())
            .join("receipt.json");
        let v = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        (path, v)
    }
    fn sql(&self, s: &str) {
        rusqlite::Connection::open(self.0.join(".awr/state.db"))
            .unwrap()
            .execute_batch(s)
            .unwrap()
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn adoption_binds_both_versions_retains_old_content_and_is_not_timestamp_selection() {
    let h = Host::new();
    let old = h.text("decisions/old.md");
    assert_eq!(h.decision("NEW")["decision"]["status"], "proposed");
    h.request("adopt", json!([h.adopt()]));
    let p = h.preview();
    assert_eq!(h.text("decisions/old.md"), old);
    let r = h.accept(&p);
    assert_eq!(r["status"], "completed");
    assert_eq!(r["filesystem_atomic"], false);
    assert_eq!(r["file_total"], 2);
    let new = h.decision("NEW");
    assert_eq!(new["decision"]["status"], "accepted");
    assert_eq!(
        new["decision"]["adoption"]["supersedes"]["external_key"],
        "OLD"
    );
    let old = h.decision("OLD");
    assert_eq!(old["decision"]["status"], "superseded");
    assert_eq!(old["decision"]["superseded_by"]["external_key"], "NEW");
    assert!(
        h.text("decisions/old.md")
            .contains("Keep the original decision and its reasons.")
    );
    assert!(h.text("decisions/old.md").contains("custom: retain"));
    let rev = h.revision();
    assert_eq!(h.accept(&p)["already_recorded"], true);
    assert_eq!(h.revision(), rev);
}
#[test]
fn related_goal_plan_ledger_and_adoption_share_one_review() {
    let h = Host::new();
    let s = h.source("work.yaml");
    h.request("linked",json!([h.edit("GOAL.md","Initial goal.","Updated goal criteria."),h.edit("PLAN.md","Initial plan.","Updated architecture."),{"operation":"ledger","source_id":s["id"],"source_fingerprint":s["fingerprint"],"operations":[{"operation":"fields","target":"W","fields":{"next_action":"Use the reviewed approach"}}]},h.adopt()]));
    let p = h.preview();
    assert_eq!(p["preview"]["files"].as_array().unwrap().len(), 5);
    let r = h.accept(&p);
    assert_eq!(r["applied_files"], json!([0, 1, 2, 3, 4]));
    assert!(h.text("GOAL.md").contains("Updated goal criteria."));
    assert!(h.text("PLAN.md").contains("Updated architecture."));
    assert!(h.text("work.yaml").contains("Use the reviewed approach"));
}
#[test]
fn all_targets_preflight_before_any_write_and_stale_approval_is_rejected() {
    let h = Host::new();
    let old = h.text("GOAL.md");
    h.request(
        "bad-last",
        json!([
            h.edit("GOAL.md", "Initial goal.", "New goal."),
            h.edit("PLAN.md", "missing fragment", "Invalid")
        ]),
    );
    h.error(
        &["batch", "change", "--input", "batch.json"],
        "SourceConflict",
    );
    assert_eq!(h.text("GOAL.md"), old);
    h.request("stale", json!([h.adopt()]));
    let p = h.preview();
    h.write(
        "decisions/new.md",
        &format!("{}\nExternal content.\n", h.text("decisions/new.md")),
    );
    h.error(
        &h.args(&p).iter().map(String::as_str).collect::<Vec<_>>(),
        "RevisionConflict",
    );
    assert!(h.text("decisions/old.md").contains("status: accepted"));
    assert!(h.text("decisions/new.md").contains("External content."));
}
#[test]
fn external_content_change_invalidates_adoption_until_that_exact_version_is_reviewed() {
    let h = Host::new();
    h.request("first", json!([h.adopt()]));
    h.accept(&h.preview());
    let old = h.text("decisions/new.md");
    h.write(
        "decisions/new.md",
        &old.replace(
            "Use the reviewed approach.",
            "A materially changed approach.",
        ),
    );
    h.ok(&["source", "reindex"]);
    assert_eq!(h.decision("NEW")["decision"]["status"], "unknown");
    h.request("review-changed",json!([{"operation":"adopt","candidate":h.reference("decisions/new.md","NEW"),"supersedes":null}]));
    h.accept(&h.preview());
    assert_eq!(h.decision("NEW")["decision"]["status"], "accepted");
}
#[test]
fn partial_file_crash_boundary_is_explicit_and_recovery_resumes_remaining_file_once() {
    let h = Host::new();
    h.request(
        "two",
        json!([
            h.edit("GOAL.md", "Initial goal.", "Reviewed goal."),
            h.edit("PLAN.md", "Initial plan.", "Reviewed plan.")
        ]),
    );
    let r = h.accept(&h.preview());
    let (path, mut receipt) = h.stored(&r);
    let base = path.parent().unwrap();
    let plan_path = receipt["plan"]["files"][1]["path"].as_str().unwrap();
    fs::write(plan_path, fs::read(base.join("1.before")).unwrap()).unwrap();
    receipt["phase"] = json!("prepared");
    receipt["applied_files"] = json!([0]);
    fs::write(&path, receipt.to_string()).unwrap();
    let s = h.ok(&["batch", "status", "--key", "two"]);
    assert_eq!(s["status"], "pending_recovery");
    assert_eq!(s["partial_apply"], true);
    assert_eq!(s["current_sources"], json!(["after", "before"]));
    let goal = h.text("GOAL.md");
    let r = h.ok(&[
        "batch",
        "recover",
        "--key",
        "two",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(r["status"], "completed");
    assert_eq!(h.text("GOAL.md"), goal);
    assert!(h.text("PLAN.md").contains("Reviewed plan."));
    let rev = h.revision();
    h.ok(&[
        "batch",
        "recover",
        "--key",
        "two",
        "--expected-revision",
        &rev,
    ]);
    assert_eq!(h.revision(), rev);
}
#[test]
fn partial_recovery_preflights_all_files_and_preserves_external_content() {
    let h = Host::new();
    h.request(
        "partial",
        json!([
            h.edit("GOAL.md", "Initial goal.", "New goal."),
            h.edit("PLAN.md", "Initial plan.", "New plan.")
        ]),
    );
    let r = h.accept(&h.preview());
    let (path, mut receipt) = h.stored(&r);
    receipt["phase"] = json!("prepared");
    receipt["applied_files"] = json!([0]);
    fs::write(&path, receipt.to_string()).unwrap();
    h.write(
        "PLAN.md",
        "# Plan {#p status=active}\n\nNew external work.\n",
    );
    let goal = h.text("GOAL.md");
    h.error(
        &[
            "batch",
            "recover",
            "--key",
            "partial",
            "--expected-revision",
            &h.revision(),
        ],
        "SourceConflict",
    );
    assert_eq!(h.text("GOAL.md"), goal);
    assert!(h.text("PLAN.md").contains("New external work."));
}
#[test]
fn real_multi_file_index_failure_reports_written_files_and_recovers_without_rewriting() {
    let h = Host::new();
    h.request("index-failure", json!([h.adopt()]));
    let p = h.preview();
    h.sql("CREATE TRIGGER fail_index BEFORE UPDATE OF fingerprint ON sources WHEN NEW.fingerprint<>OLD.fingerprint BEGIN SELECT RAISE(ABORT,'fixture index failure'); END;");
    let o = h.run(&h.args(&p).iter().map(String::as_str).collect::<Vec<_>>());
    assert!(!o.status.success());
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["status"], "pending_recovery");
    assert_eq!(r["applied_files"], json!([0, 1]));
    h.sql("DROP TRIGGER fail_index;");
    let r = h.ok(&[
        "batch",
        "recover",
        "--key",
        "index-failure",
        "--expected-revision",
        &h.revision(),
    ]);
    assert_eq!(r["status"], "completed");
    assert_eq!(r["source_write_performed"], false);
}
#[test]
fn duplicate_targets_and_ambiguous_lifecycle_metadata_are_rejected() {
    let h = Host::new();
    h.request(
        "overlap",
        json!([
            h.edit("PLAN.md", "Initial plan.", "New plan."),
            h.edit("PLAN.md", "Initial plan.", "Another plan.")
        ]),
    );
    h.error(
        &["batch", "change", "--input", "batch.json"],
        "InvalidInput",
    );
    h.write(
        "decisions/new.md",
        "# New\n\nStatus: proposed\n\nNew body.\n",
    );
    h.ok(&["source", "reindex"]);
    h.request("unsupported",json!([{"operation":"adopt","candidate":h.reference("decisions/new.md",h.source("decisions/new.md")["locator"].as_str().unwrap()),"supersedes":null}]));
    let before = h.text("decisions/new.md");
    let o = h.run(&["batch", "change", "--input", "batch.json"]);
    assert!(!o.status.success());
    assert_eq!(h.text("decisions/new.md"), before);
}
