use awr_core::Id;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture {
    root: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new(yaml: bool, size: usize) -> Self {
        let root = std::env::temp_dir().join(format!("awr-payload-cli-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let (domain, name, adapter, prefix) = if yaml {
            (
                "ledger",
                "work.yaml",
                "yaml-ledger-v1",
                "work_items:\n- {id: W, title: Reviewed draft, status: in_progress, next_action: Read draft, acceptance: [Retain source facts]}\n#",
            )
        } else {
            (
                "goal",
                "goals.md",
                "markdown-heading-v1",
                "# Reviewed goal {status=active}\n",
            )
        };
        let source = root.join(name);
        let mut bytes = prefix.as_bytes().to_vec();
        bytes.resize(size, b'x');
        fs::write(&source, bytes).unwrap();
        fs::write(root.join("sources.toml"), format!("[project]\nname='Source limit fixture'\n[[sources]]\ndomain='{domain}'\nrole='primary'\npath='{name}'\nadapter='{adapter}'\n")).unwrap();
        let f = Self { root, source };
        f.ok(&["init", "--manifest", "sources.toml", "--accept"]);
        f
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .arg("--project")
            .arg(&self.root)
            .arg("--json")
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn revision(&self) -> String {
        self.ok(&["proposal", "list"])["project_revision"].to_string()
    }
    fn limit_error(&self, out: &Output) {
        assert!(!out.status.success());
        let error: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["code"], "InvalidInput");
        assert!(error["message"].as_str().unwrap().contains("cap"));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn source_drill_cannot_raise_the_adapter_limit_or_return_oversized_source_content() {
    for (yaml, cap) in [(false, 2 * 1024 * 1024), (true, 4 * 1024 * 1024)] {
        let f = Fixture::new(yaml, 200);
        let mut bytes = fs::read(&f.source).unwrap();
        bytes.resize(cap + 1, b'x');
        fs::write(&f.source, bytes).unwrap();
        let out = f.run(&[
            "source",
            "show",
            if yaml { "ledger" } else { "goal" },
            "--content",
            "--max-bytes",
            "16777216",
        ]);
        f.limit_error(&out);
        assert!(out.stdout.is_empty());
    }
    println!("AWR_PAYLOAD_CASE source_drill_limit");
}

#[test]
fn a_reviewed_mutation_cannot_write_a_yaml_source_over_the_adapter_limit() {
    let f = Fixture::new(true, 4 * 1024 * 1024 - 256);
    let before = fs::read(&f.source).unwrap();
    let patch = serde_json::json!({"title":"R".repeat(1024)}).to_string();
    let created = f.ok(&[
        "proposal",
        "create",
        "--kind",
        "work",
        "--target",
        "W",
        "--intent",
        "Clarify the draft title",
        "--patch",
        &patch,
        "--expected-revision",
        &f.revision(),
    ]);
    let id = created["proposal"]["id"].as_str().unwrap();
    for action in ["submit", "approve"] {
        f.ok(&[
            "proposal",
            action,
            id,
            "--actor",
            "fixture-reviewer",
            "--reason",
            "Reviewed the requested title",
            "--expected-revision",
            &f.revision(),
        ]);
    }
    let out = f.run(&[
        "proposal",
        "apply",
        id,
        "--actor",
        "fixture-writer",
        "--reason",
        "Apply the reviewed draft title",
        "--expected-revision",
        &f.revision(),
    ]);
    f.limit_error(&out);
    assert_eq!(fs::read(&f.source).unwrap(), before);
    assert_eq!(
        f.ok(&["work", "show", "W"])["work"]["title"],
        "Reviewed draft"
    );
    assert!(
        fs::read_dir(f.root.join(".awr/mutations"))
            .unwrap()
            .all(|entry| !entry.unwrap().path().is_dir())
    );
    println!("AWR_PAYLOAD_CASE source_mutation_limit");
}
