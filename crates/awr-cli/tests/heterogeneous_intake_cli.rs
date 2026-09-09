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
        let path = std::env::temp_dir().join(format!("awr heterogeneous 项目 {}", Id::new()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
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
        let o = self.run(args);
        assert!(
            o.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn existing_chinese_ledger_adr_and_public_schema_import_without_source_edits() {
    let f = Fixture::new();
    let ledger = "# 项目权威台账\n\n| ID | 工作项 | 状态 | Owner 角色 | 完成硬门槛 | 当前证据 / 下一动作 |\n|---|---|---|---|---|---|\n| W | 交付报表 | pending | Delivery | 用户收到报表 | 整理数据 |\n| D | 既有功能 | complete | Delivery | 历史报告符合要求 | 保留待验证证据 |\n";
    let adr = "# 保留项目资料\n\n- 编号：ADR-1\n- **状态：** Accepted\n- 日期：2026-09-09\n\n## 决策\n沿用原台账。\n\n## 背景\n减少接入负担。\n";
    let readme = "# 项目\n讨论 Bearer authentication。\n\n```json\n{\"token\":{\"type\":\"string\"},\"authorization\":{\"type\":\"http\",\"scheme\":\"bearer\"}}\n```\n";
    for (path, body) in [
        ("docs/ledger/phase-1-ledger.md", ledger),
        ("docs/decisions/ADR-1.md", adr),
        ("README.md", readme),
    ] {
        f.write(path, body);
    }
    let preview = f.ok(&[
        "init",
        "--status-map",
        "pending=planned",
        "--status-map",
        "complete=completed",
    ]);
    assert!(!f.0.join(".awr").exists());
    assert!(
        preview["draft"]["generated_files"]
            .get(".awr/intake/work-ledger.yaml")
            .is_none()
    );
    let initialized = f.ok(&[
        "init",
        "--status-map",
        "pending=planned",
        "--status-map",
        "complete=completed",
        "--accept",
    ]);
    assert_eq!(initialized["index"]["ok"], true);
    let w = f.ok(&["work", "show", "W"]);
    assert_eq!(w["work"]["status"], "planned");
    assert_eq!(w["work"]["raw_status"], "pending");
    assert_eq!(w["work"]["title"], "交付报表");
    assert_eq!(w["acceptance"][0], "用户收到报表");
    let d = f.ok(&["decision", "show", "ADR-1"]);
    assert!(d.to_string().contains("accepted"));
    assert!(d.to_string().contains("沿用原台账"));
    let report = f.ok(&["intake", "inspect"]);
    assert_eq!(report["organization"]["source_completed"], 1);
    assert_eq!(report["organization"]["verified_completed"], 0);
    for (path, body) in [
        ("docs/ledger/phase-1-ledger.md", ledger),
        ("docs/decisions/ADR-1.md", adr),
        ("README.md", readme),
    ] {
        assert_eq!(fs::read_to_string(f.0.join(path)).unwrap(), body);
    }
}

#[test]
fn mapped_yaml_claim_progress_and_mapping_change_obey_source_binding() {
    let f = Fixture::new();
    let text = "goals:\n- id: G\n  title: 用户需要报表\n  status: active\n  summary: 用户要求交付报表\n  success_criteria: [收到报表]\nwork_items:\n- ticket: W\n  name: 交付报表\n  phase: Ready\n  objective: G\n  done_when: [收到报表]\n  next: 整理数据\n- ticket: WAIT\n  name: 后续迭代\n  phase: Pending\n  objective: G\n  done_when: [收到报表]\n  next: 确认下一轮范围\n";
    f.write("work-ledger.yaml", text);
    f.ok(&[
        "init",
        "--accept",
        "--field-map",
        "id=ticket",
        "--field-map",
        "title=name",
        "--field-map",
        "status=phase",
        "--field-map",
        "goal=objective",
        "--field-map",
        "acceptance=done_when",
        "--field-map",
        "next_action=next",
        "--status-map",
        "Pending=planned",
        "--status-map",
        "Doing=in_progress",
        "--status-map",
        "Done=completed",
    ]);
    assert_eq!(
        fs::read_to_string(f.0.join("work-ledger.yaml")).unwrap(),
        text
    );
    assert_eq!(f.ok(&["doctor"])["ok"], true);
    let revision = f.ok(&["status"])["project_revision"].to_string();
    let started = f.ok(&[
        "session",
        "start",
        "--work",
        "W",
        "--agent",
        "mapping-worker",
        "--provider",
        "generic",
        "--model",
        "local",
        "--claim",
        "--expected-revision",
        &revision,
    ]);
    let session = started["session"]["id"].as_str().unwrap();
    let revision = f.ok(&["status"])["project_revision"].to_string();
    f.ok(&[
        "work",
        "progress",
        "W",
        "--session",
        session,
        "--expected-revision",
        &revision,
        "--reason",
        "已开始整理数据",
        "--next-action",
        "复核报表",
    ]);
    let changed = fs::read_to_string(f.0.join("work-ledger.yaml")).unwrap();
    let source: Value =
        serde_json::to_value(serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&changed).unwrap())
            .unwrap();
    assert_eq!(source["work_items"][0]["phase"], "Doing");
    assert_eq!(source["work_items"][0]["next"], "复核报表");
    assert!(source["work_items"][0].get("status").is_none());
    assert_eq!(source["work_items"][1]["phase"], "Pending");
    let before = f.ok(&["work", "show", "W"]);
    assert_eq!(before["work"]["status"], "in_progress");
    let path = f.0.join(".awr/project.toml");
    let manifest = fs::read_to_string(&path).unwrap();
    assert!(manifest.contains("Doing = \"in_progress\""));
    fs::write(
        path,
        manifest.replace("Doing = \"in_progress\"", "Doing = \"blocked\""),
    )
    .unwrap();
    let diagnosis = f.run(&["doctor"]);
    assert!(!diagnosis.status.success());
    let diagnosis: Value = serde_json::from_slice(&diagnosis.stdout).unwrap();
    assert_eq!(diagnosis["read_only"], true);
    assert_eq!(diagnosis["source_refresh_performed"], false);
    assert!(
        diagnosis["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| finding["code"] == "source_configuration_changed")
    );
    let stale = f.run(&[
        "work",
        "progress",
        "W",
        "--session",
        session,
        "--expected-revision",
        &before["project_revision"].to_string(),
        "--reason",
        "尝试旧映射",
        "--next-action",
        "不能写入",
    ]);
    assert!(!stale.status.success());
    assert_eq!(
        fs::read_to_string(f.0.join("work-ledger.yaml")).unwrap(),
        changed
    );
    let after = f.ok(&["work", "show", "W"]);
    assert_eq!(after["work"]["status"], "blocked");
    assert_eq!(f.ok(&["doctor"])["ok"], true);
    assert!(
        after["project_revision"].as_u64().unwrap() > before["project_revision"].as_u64().unwrap()
    );
}

#[test]
fn invalid_mapping_and_embedded_credentials_fail_before_initialization() {
    let f = Fixture::new();
    f.write(
        "work-ledger.yaml",
        "work_items: [{id: W, title: Report, status: pending}]\n",
    );
    for args in [
        vec!["init", "--accept", "--status-map", "completed=ready"],
        vec!["init", "--field-map", "status=evidence"],
        vec![
            "init",
            "--status-map",
            "pending=planned",
            "--status-map",
            "pending=ready",
        ],
    ] {
        assert!(!f.run(&args).status.success());
        assert!(!f.0.join(".awr").exists());
    }
    // CLI arguments and documentation use the same value-sensitive boundary.
    let output = f.run(&[
        "init",
        "--goal",
        r#"{"authorization":{"type":"http","scheme":"bearer","value":"fixture-value"}}"#,
    ]);
    assert!(!output.status.success());
    assert!(!f.0.join(".awr").exists());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-value"));
}
