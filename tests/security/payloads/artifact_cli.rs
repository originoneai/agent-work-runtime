use awr_core::Id;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-artifact-cli-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        fs::write(
            root.join("work.yaml"),
            "work_items:\n- id: W\n  title: Review report\n  status: ready\n",
        )
        .unwrap();
        fs::write(root.join("project.toml"), "[project]\nname='Artifact CLI'\nexternal_key='artifact-cli'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        let fixture = Self(root);
        fixture.ok(&["init", "--manifest", "project.toml", "--accept"]);
        fixture
    }
    fn run(&self, args: &[&str], json: bool) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_awr"));
        cmd.arg("--project").arg(&self.0);
        if json {
            cmd.arg("--json");
        }
        cmd.args(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let result = self.run(args, true);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    }
    fn reject(&self, args: &[&str]) {
        let result = self.run(args, true);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&result.stderr).unwrap()["code"],
            "InvalidInput"
        );
    }
    fn revision(&self) -> String {
        self.ok(&["session", "list"])["project_revision"].to_string()
    }
    fn event(&self) -> Value {
        self.ok(&[
            "event",
            "append",
            "--type",
            "report.produced",
            "--summary",
            "Produced report",
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn add_args<'a>(event: &'a str, revision: &'a str, cap: &'a str) -> Vec<&'a str> {
        vec![
            "artifact",
            "add",
            "report.bin",
            "--type",
            "report",
            "--mime",
            "application/octet-stream",
            "--source-event",
            event,
            "--expected-revision",
            revision,
            "--max-bytes",
            cap,
        ]
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_import_cannot_raise_the_runtime_limit_or_leave_a_failed_copy() {
    let f = Fixture::new();
    fs::write(f.0.join("report.bin"), b"report").unwrap();
    let event = f.event();
    let eid = event["event"]["id"].as_str().unwrap();
    let revision = f.revision();
    for cap in ["0", "67108865", "18446744073709551615", "5"] {
        f.reject(&Fixture::add_args(eid, &revision, cap));
        assert_eq!(f.revision(), revision);
        assert!(!f.0.join(".awr/artifacts").exists());
    }
    let stored = f.ok(&Fixture::add_args(eid, &revision, "6"));
    assert_eq!(stored["artifact"]["size"], 6);
    let locator = stored["artifact"]["locator"].as_str().unwrap();
    assert_eq!(fs::read(f.0.join(locator)).unwrap(), b"report");
    assert_eq!(
        stored["project_revision"].as_u64().unwrap(),
        revision.parse::<u64>().unwrap() + 1
    );
}

#[test]
fn cli_read_checks_its_limit_before_emitting_raw_or_json_content() {
    let f = Fixture::new();
    let bytes = [0, 255, 42];
    fs::write(f.0.join("report.bin"), bytes).unwrap();
    let event = f.event();
    let stored = f.ok(&Fixture::add_args(
        event["event"]["id"].as_str().unwrap(),
        &f.revision(),
        "3",
    ));
    let id = stored["artifact"]["id"].as_str().unwrap();
    let revision = f.revision();
    for cap in ["0", "2", "16777217", "18446744073709551615"] {
        f.reject(&["artifact", "cat", id, "--max-bytes", cap]);
        let raw = f.run(&["artifact", "cat", id, "--max-bytes", cap], false);
        assert!(!raw.status.success() && raw.stdout.is_empty());
        assert_eq!(f.revision(), revision);
    }
    let raw = f.run(&["artifact", "cat", id, "--max-bytes", "3"], false);
    assert!(raw.status.success());
    assert_eq!(raw.stdout, bytes);
    assert_eq!(
        f.ok(&["artifact", "show", id])["artifact"],
        stored["artifact"]
    );
    assert_eq!(f.revision(), revision);
}
