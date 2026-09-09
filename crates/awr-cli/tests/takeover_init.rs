use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("awr 接入 project {}", awr_core::Id::new()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, body: &str) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(["--project", self.0.to_str().unwrap(), "--json"])
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn empty_and_code_only_projects_get_a_real_work_entry_without_invented_history() {
    for code in [false, true] {
        let p = Project::new();
        if code {
            p.write("src/main.rs", "fn main() {}\n");
        }
        let preview = p.ok(&["init", "--goal", "Deliver a useful document portal"]);
        assert_eq!(preview["status"], "preview");
        assert!(!p.0.join(".awr").exists());
        assert_eq!(
            preview["draft"]["generated_files"]
                .as_object()
                .unwrap()
                .len(),
            4
        );
        let installed = p.ok(&[
            "init",
            "--accept",
            "--goal",
            "Deliver a useful document portal",
        ]);
        assert_eq!(installed["index"]["indexed"], 4);
        let work = p.ok(&["work", "show", "INTAKE-001"]);
        assert!(work.to_string().contains("核实项目目标"));
        let before = fs::read(p.0.join(".awr/intake/work-ledger.yaml")).unwrap();
        p.ok(&["init", "--accept"]);
        assert_eq!(
            before,
            fs::read(p.0.join(".awr/intake/work-ledger.yaml")).unwrap()
        );
        if code {
            assert_eq!(
                fs::read_to_string(p.0.join("src/main.rs")).unwrap(),
                "fn main() {}\n"
            );
        }
    }
}
#[test]
fn a_plan_yields_reviewable_work_and_does_not_claim_existing_implementation() {
    let p = Project::new();
    let plan = "# Publish documents {#publish status=active}\n\nDeliver a search page.\n\n## Search UI {#search status=planned}\n\nReaders can find documents.\n";
    p.write("PLAN.md", plan);
    p.ok(&["init", "--accept"]);
    let ledger = fs::read_to_string(p.0.join(".awr/intake/work-ledger.yaml")).unwrap();
    assert!(ledger.contains("Publish documents"));
    assert!(ledger.contains("Search UI"));
    assert!(!ledger.contains("status: completed"));
    assert_eq!(fs::read_to_string(p.0.join("PLAN.md")).unwrap(), plan);
}
#[test]
fn existing_markdown_table_and_checkbox_states_are_preserved() {
    let p = Project::new();
    let body = "# Existing tasks\n\n| 编号 | 任务 | 状态 | 下一步 |\n| --- | --- | --- | --- |\n| DONE-1 | Old delivery | 已完成 | Review receipt |\n| OPEN-1 | Current delivery | 进行中 | Finish search |\n\n- [ ] Confirm remaining scope\n";
    p.write("台账.md", body);
    p.ok(&["init", "--accept"]);
    assert!(!p.0.join(".awr/intake/work-ledger.yaml").exists());
    assert!(
        p.ok(&["work", "show", "DONE-1"])
            .to_string()
            .contains("completed")
    );
    assert!(
        p.ok(&["work", "show", "OPEN-1"])
            .to_string()
            .contains("in_progress")
    );
    assert_eq!(fs::read_to_string(p.0.join("台账.md")).unwrap(), body);
}
#[test]
fn reviewed_draft_rejects_changed_inventory_and_cannot_write_arbitrary_paths() {
    let p = Project::new();
    p.write("README.md", "# Document portal\n");
    let draft = std::env::temp_dir().join(format!("awr-draft-{}.json", awr_core::Id::new()));
    p.ok(&["init", "--write-draft", draft.to_str().unwrap()]);
    p.write("README.md", "# Changed portal\n");
    let stale = p.run(&["init", "--from-draft", draft.to_str().unwrap(), "--accept"]);
    assert!(!stale.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&stale.stderr).unwrap()["code"],
        "SourceConflict"
    );
    assert!(!p.0.join(".awr").exists());
    fs::remove_file(&draft).unwrap();
    p.ok(&["init", "--write-draft", draft.to_str().unwrap()]);
    let mut value: Value = serde_json::from_slice(&fs::read(&draft).unwrap()).unwrap();
    value["generated_files"]["README.md"] = Value::String("overwrite".into());
    fs::write(&draft, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(
        !p.run(&["init", "--from-draft", draft.to_str().unwrap(), "--accept"])
            .status
            .success()
    );
    assert_eq!(
        fs::read_to_string(p.0.join("README.md")).unwrap(),
        "# Changed portal\n"
    );
    fs::remove_file(draft).unwrap();
}
#[test]
fn reviewed_draft_can_supply_concrete_new_work() {
    let p = Project::new();
    let draft = std::env::temp_dir().join(format!("awr-draft-{}.json", awr_core::Id::new()));
    p.ok(&["init", "--write-draft", draft.to_str().unwrap()]);
    let mut value: Value = serde_json::from_slice(&fs::read(&draft).unwrap()).unwrap();
    value["generated_files"][".awr/intake/work-ledger.yaml"]=Value::String("work_items:\n- id: SEARCH-001\n  title: Implement approved document search\n  status: ready\n  acceptance: [Return the approved documents for a query]\n  next_action: Implement the search handler\n".into());
    fs::write(&draft, serde_json::to_vec(&value).unwrap()).unwrap();
    p.ok(&["init", "--from-draft", draft.to_str().unwrap(), "--accept"]);
    assert!(
        p.ok(&["work", "show", "SEARCH-001"])
            .to_string()
            .contains("Implement approved")
    );
    fs::remove_file(draft).unwrap();
}
#[cfg(unix)]
#[test]
fn inventory_does_not_follow_symlinks() {
    let p = Project::new();
    std::os::unix::fs::symlink(Path::new("/"), p.0.join("outside")).unwrap();
    let preview = p.ok(&["init"]);
    assert!(preview["draft"]["inventory"].as_array().unwrap().is_empty());
}
