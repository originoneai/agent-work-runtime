use awr_core::Id;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
struct Project(PathBuf);
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_awr"))
        .args(["--project", root.to_str().unwrap(), "--json"])
        .args(args)
        .output()
        .unwrap()
}
fn ok(root: &Path, args: &[&str]) -> Value {
    let r = run(root, args);
    assert!(
        r.status.success(),
        "{args:?}\n{}\n{}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    serde_json::from_slice(&r.stdout).unwrap()
}
const WORK: &str = "custom_contract: {target: 17, accepted: 3} # retain this exact contract\ncurrent:\n  stage: 'P1' # original style\n  focus: W\n  area: [W]\n  next: Read notes\n  extra: {anything: untouched}\ngoals:\n- id: G\n  title: A useful guide\n  status: active\nmilestones:\n- id: P1\n  title: Draft\n  status: in_progress\n- id: P2\n  title: Publish\n  status: planned\nwork_items:\n- id: W\n  title: Write guide\n  status: in_progress\n  milestone: P1\n  goal: G\n  acceptance: [Useful guide]\n  next_action: Draft examples\n- id: DONE\n  title: Old guide\n  status: completed\n  milestone: P1\n- id: LATER\n  title: Publish guide\n  status: planned\n  milestone: P2\n";
impl Project {
    fn new() -> Self {
        let p = Self(std::env::temp_dir().join(format!("awr-organization-{}", Id::new())));
        fs::create_dir(&p.0).unwrap();
        fs::write(p.0.join("map.toml"),"[project]\nname='Guide'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        fs::write(p.0.join("work.yaml"), WORK).unwrap();
        ok(&p.0, &["init", "--manifest", "map.toml", "--accept"]);
        p
    }
    fn request(&self) -> Value {
        let w = ok(&self.0, &["work", "show", "W"]);
        json!({"version":1,"request_key":"focus-review","actor":{"origin":"ai_accepted","host":"fixture","subject":"writer"},"reason":"Review current delivery","source_id":w["source_ref"]["source_id"],"source_fingerprint":w["source_ref"]["source_fingerprint"],"mapping":{"phase":"/current/stage","scope":"/current/area","focus":"/current/focus","next_action":"/current/next"},"values":{"phase":"P1","scope":["W","LATER"],"focus":"W","next_action":"Review the worked examples"}})
    }
    fn input(&self, r: &Value) {
        fs::write(self.0.join("request.json"), serde_json::to_vec(r).unwrap()).unwrap();
    }
    fn preview(&self, r: &Value) -> Value {
        self.input(r);
        ok(
            &self.0,
            &["organization", "preview", "--input", "request.json"],
        )
    }
    fn apply(&self, p: &Value) -> Value {
        ok(
            &self.0,
            &[
                "organization",
                "change",
                "--input",
                "request.json",
                "--expected-preview",
                p["preview"]["fingerprint"].as_str().unwrap(),
                "--expected-revision",
                &p["preview"]["project_revision"].to_string(),
            ],
        )
    }
}
#[test]
fn explicit_metadata_preview_is_readonly_and_preserves_contracts_identity_and_style() {
    let p = Project::new();
    let r = p.request();
    let work = ok(&p.0, &["work", "show", "W"]);
    let before = fs::read(p.0.join("work.yaml")).unwrap();
    let db = fs::read(p.0.join(".awr/state.db")).unwrap();
    let plan = p.preview(&r);
    assert_eq!(fs::read(p.0.join("work.yaml")).unwrap(), before);
    assert_eq!(fs::read(p.0.join(".awr/state.db")).unwrap(), db);
    assert_eq!(plan["preview"]["entity_changes"], 0);
    let result = p.apply(&plan);
    fs::write(
        p.0.join("fields.json"),
        serde_json::to_vec(&r["mapping"]).unwrap(),
    )
    .unwrap();
    let read = ok(
        &p.0,
        &[
            "organization",
            "show",
            "--source",
            r["source_id"].as_str().unwrap(),
            "--mapping",
            "fields.json",
        ],
    );
    assert_eq!(read["fields"], plan["preview"]["after"]);
    assert_eq!(result["phase"], "completed");
    let after = fs::read_to_string(p.0.join("work.yaml")).unwrap();
    assert!(after.starts_with(WORK.lines().next().unwrap()));
    assert!(after.contains("stage: 'P1' # original style"));
    assert!(after.contains("extra: {anything: untouched}"));
    assert!(after.ends_with(&WORK[WORK.find("goals:").unwrap()..]));
    let new = ok(&p.0, &["work", "show", "W"]);
    assert_eq!(new["work"]["id"], work["work"]["id"]);
    assert_eq!(new["acceptance"], work["acceptance"]);
    let replay = p.apply(&plan);
    assert_eq!(replay["source_write_performed"], false);
    assert_eq!(replay["project_revision"], result["project_revision"]);
}
#[test]
fn references_stale_sources_and_completion_bypasses_are_rejected() {
    let p = Project::new();
    let original = p.request();
    for (field, value) in [
        ("focus", json!("DONE")),
        ("phase", json!("P2")),
        ("scope", json!(["LATER"])),
        ("focus", json!("ABSENT")),
    ] {
        let mut r = original.clone();
        r["values"][field] = value;
        p.input(&r);
        assert!(
            !run(
                &p.0,
                &["organization", "preview", "--input", "request.json"]
            )
            .status
            .success()
        );
    }
    let mut r = original.clone();
    r["mapping"]["focus"] = json!("/work_items/0/status");
    p.input(&r);
    assert!(
        !run(
            &p.0,
            &["organization", "preview", "--input", "request.json"]
        )
        .status
        .success()
    );
    let plan = p.preview(&original);
    fs::write(p.0.join("work.yaml"), format!("{WORK}# user edit\n")).unwrap();
    let changed = fs::read(p.0.join("work.yaml")).unwrap();
    assert!(
        !run(
            &p.0,
            &[
                "organization",
                "change",
                "--input",
                "request.json",
                "--expected-preview",
                plan["preview"]["fingerprint"].as_str().unwrap(),
                "--expected-revision",
                &plan["preview"]["project_revision"].to_string()
            ]
        )
        .status
        .success()
    );
    assert_eq!(fs::read(p.0.join("work.yaml")).unwrap(), changed);
}
#[test]
fn interrupted_metadata_write_recovers_only_matching_bytes() {
    let p = Project::new();
    let request = p.request();
    let preview = p.preview(&request);
    let result = p.apply(&preview);
    let path = p.0.join(result["receipt"].as_str().unwrap());
    let mut receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    receipt["phase"] = json!("prepared");
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    let before = fs::read(p.0.join("work.yaml")).unwrap();
    let revision = result["project_revision"].to_string();
    let recovered = ok(
        &p.0,
        &[
            "organization",
            "recover",
            "--key",
            "focus-review",
            "--expected-revision",
            &revision,
        ],
    );
    assert_eq!(recovered["phase"], "completed");
    assert_eq!(fs::read(p.0.join("work.yaml")).unwrap(), before);
    receipt["phase"] = json!("prepared");
    fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    fs::write(
        p.0.join("work.yaml"),
        format!(
            "{}# external followup\n",
            String::from_utf8(before).unwrap()
        ),
    )
    .unwrap();
    let current = fs::read(p.0.join("work.yaml")).unwrap();
    assert!(
        !run(
            &p.0,
            &[
                "organization",
                "recover",
                "--key",
                "focus-review",
                "--expected-revision",
                &recovered["project_revision"].to_string()
            ]
        )
        .status
        .success()
    );
    assert_eq!(fs::read(p.0.join("work.yaml")).unwrap(), current);
}
