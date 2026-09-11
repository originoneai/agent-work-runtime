use awr_core::Id;
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let p = Self(std::env::temp_dir().join(format!("awr-public-notes-{}", Id::new())));
        fs::create_dir(&p.0).unwrap();
        fs::write(p.0.join("mapping.toml"), "[project]\nname='Public notes'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        p
    }
    fn source(&self, summary: &str) -> String {
        let text = format!(
            "goals:\n- id: G\n  title: Publish a guide\n  status: active\nwork_items:\n- id: W\n  title: Review a guide\n  status: ready\n  goal: G\n  next_action: Review the guide\n  acceptance: [A useful guide]\n  summary: {}\n",
            serde_json::to_string(summary).unwrap()
        );
        fs::write(self.0.join("work.yaml"), &text).unwrap();
        text
    }
    fn run(&self, args: &[&str]) -> (bool, Value, String) {
        let p = Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(["--project", self.0.to_str().unwrap(), "--json"])
            .args(args)
            .output()
            .unwrap();
        (
            p.status.success(),
            serde_json::from_slice(&p.stdout).unwrap_or(Value::Null),
            String::from_utf8(p.stderr).unwrap(),
        )
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn public_command_and_permission_notes_survive_intake_query_and_reindex() {
    let p = Project::new();
    let first = "PROJECT_ROOT=/public/guide EXPECTED_ITEMS=42 cargo test --offline";
    let source = p.source(first);
    let (ok, preview, error) = p.run(&["init", "--manifest", "mapping.toml"]);
    assert!(ok, "{error}");
    assert_eq!(preview["preview"]["source_issues"], serde_json::json!([]));
    assert!(!p.0.join(".awr").exists());
    let (ok, _, error) = p.run(&[
        "init",
        "--manifest",
        "mapping.toml",
        "--accept",
        "--expected-preview",
        preview["preview"]["fingerprint"].as_str().unwrap(),
    ]);
    assert!(ok, "{error}");
    let (ok, work, error) = p.run(&["work", "show", "W"]);
    assert!(ok, "{error}");
    assert_eq!(work["work"]["summary"], first);
    assert_eq!(fs::read_to_string(p.0.join("work.yaml")).unwrap(), source);
    let second = "Completed native authorization: 用户明确确认；原文和历史保持不变。";
    p.source(second);
    let (ok, work2, error) = p.run(&["work", "show", "W"]);
    assert!(ok, "{error}");
    assert_eq!(work2["work"]["id"], work["work"]["id"]);
    assert_eq!(work2["work"]["summary"], second);
}

#[test]
fn ambiguous_fields_and_dumps_return_categories_without_echo_and_can_recover() {
    let p = Project::new();
    p.source("Public guide");
    assert!(p.run(&["init", "--manifest", "mapping.toml", "--accept"]).0);
    let old = p.run(&["work", "show", "W"]).1["work"]["id"].clone();
    for (text, category) in [
        ("authorization: synthetic-private-value", "labelled_value"),
        ("export HOME=/synthetic-private-value", "environment_dump"),
    ] {
        let source = p.source(text);
        let (ok, result, error) = p.run(&["status"]);
        assert!(!ok);
        assert_eq!(result["source_issues"][0]["code"], "RuleViolation");
        assert_eq!(result["source_issues"][0]["details"]["category"], category);
        assert!(!format!("{result}{error}").contains("synthetic-private-value"));
        assert_eq!(fs::read_to_string(p.0.join("work.yaml")).unwrap(), source);
    }
    p.source("Expected count GUIDE_ITEMS=42; review the guide.");
    let (ok, work, error) = p.run(&["work", "show", "W"]);
    assert!(ok, "{error}");
    assert_eq!(work["work"]["id"], old);
}
