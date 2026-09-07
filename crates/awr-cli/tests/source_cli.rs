use awr_core::{EntityKind, Id};
use awr_store::Store;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-cli-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        for (path, body) in [
            (
                "project.toml",
                include_str!("../../../examples/basic/project.toml"),
            ),
            ("GOALS.md", include_str!("../../../examples/basic/GOALS.md")),
            ("PLAN.md", include_str!("../../../examples/basic/PLAN.md")),
            ("RULES.md", include_str!("../../../examples/basic/RULES.md")),
            (
                "work-ledger.yaml",
                include_str!("../../../examples/basic/work-ledger.yaml"),
            ),
            (".gitignore", "# Preserve existing ignore rules\n*.tmp\n"),
        ] {
            fs::write(root.join(path), body).unwrap();
        }
        Self(root)
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
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn explicit_mapping_init_list_scan_and_reindex() {
    let fixture = Fixture::new();
    let source = fixture.0.join("work-ledger.yaml");
    let original = fs::read(&source).unwrap();
    let preview = fixture.ok(&["init", "--manifest", "project.toml"]);
    assert_eq!(preview["status"], "preview");
    assert!(!fixture.0.join(".awr").exists());
    let initialized = fixture.ok(&["init", "--manifest", "project.toml", "--accept"]);
    assert_eq!(initialized["configuration_created"], true);
    assert_eq!(initialized["index"]["indexed"], 4);
    let config = fs::read(fixture.0.join(".awr/project.toml")).unwrap();
    let ignore = fs::read_to_string(fixture.0.join(".gitignore")).unwrap();
    assert!(ignore.starts_with("# Preserve existing ignore rules\n*.tmp\n"));
    for entry in [
        ".awr/state.db",
        ".awr/state.db-*",
        ".awr/artifacts/",
        ".awr/cache/",
    ] {
        assert!(ignore.lines().any(|line| line == entry));
    }
    assert_eq!(fs::read(&source).unwrap(), original);
    let listed = fixture.ok(&["source", "list"]);
    assert_eq!(listed["sources"].as_array().unwrap().len(), 4);
    assert_eq!(listed["freshness_basis"], "last_scan_or_index");
    let repeated = fixture.ok(&["source", "reindex"]);
    assert_eq!(repeated["indexed"], 0);
    assert_eq!(repeated["unchanged"], 4);
    assert_eq!(repeated["project_revision"], listed["project_revision"]);
    let reused = fixture.ok(&["init", "--accept"]);
    assert_eq!(reused["configuration_created"], false);
    assert_eq!(
        fs::read(fixture.0.join(".awr/project.toml")).unwrap(),
        config
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join(".gitignore")).unwrap(),
        ignore
    );
    fs::write(
        &source,
        String::from_utf8(original)
            .unwrap()
            .replace("status: ready", "status: completed"),
    )
    .unwrap();
    let scanned = fixture.ok(&["source", "scan"]);
    assert_eq!(scanned["indexed"], 0);
    assert_eq!(scanned["pending"], 1);
    let store = Store::open_readonly(&fixture.0.join(".awr/state.db")).unwrap();
    let project = store.project_by_root(&fixture.0).unwrap();
    let ledger = store
        .sources(project.id)
        .unwrap()
        .into_iter()
        .find(|s| s.domain == "ledger")
        .unwrap();
    assert_eq!(
        store
            .source_projection_payloads(&ledger, EntityKind::WorkItem)
            .unwrap()[0]["status"],
        "ready"
    );
    drop(store);
    let reindexed = fixture.ok(&["source", "reindex"]);
    assert_eq!(reindexed["indexed"], 1);
    assert_eq!(reindexed["pending"], 0);
    let store = Store::open_readonly(&fixture.0.join(".awr/state.db")).unwrap();
    assert_eq!(
        store
            .source_projection_payloads(&ledger, EntityKind::WorkItem)
            .unwrap()[0]["status"],
        "completed"
    );
    drop(store);
    assert_eq!(fixture.ok(&["doctor"])["ok"], true);
}

#[test]
fn ambiguous_and_conflicting_authority_mappings_do_not_overwrite() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("docs")).unwrap();
    fs::write(fixture.0.join("docs/GOALS.md"), "# Another goal\n").unwrap();
    let preview = fixture.ok(&["init"]);
    assert_eq!(preview["ambiguous_domains"], serde_json::json!(["goal"]));
    assert!(preview["authority_mapping"].is_null());
    assert!(!fixture.run(&["init", "--accept"]).status.success());
    assert!(!fixture.0.join(".awr").exists());
    fixture.ok(&["init", "--manifest", "project.toml", "--accept"]);
    let config = fs::read(fixture.0.join(".awr/project.toml")).unwrap();
    let alternative = fs::read_to_string(fixture.0.join("project.toml"))
        .unwrap()
        .replace("AWR example", "Conflicting project");
    fs::write(fixture.0.join("alternative.toml"), alternative).unwrap();
    let conflict = fixture.run(&["init", "--manifest", "alternative.toml", "--accept"]);
    assert!(!conflict.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&conflict.stderr).unwrap()["code"],
        "SourceConflict"
    );
    assert_eq!(
        fs::read(fixture.0.join(".awr/project.toml")).unwrap(),
        config
    );
    fs::remove_file(fixture.0.join("work-ledger.yaml")).unwrap();
    let failed = fixture.run(&["source", "reindex"]);
    assert!(!failed.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&failed.stdout).unwrap()["ok"],
        false
    );
}
