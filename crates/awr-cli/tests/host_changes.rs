use awr_core::Id;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    process::{Command, Output},
};
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let p = Self(std::env::temp_dir().join(format!("awr 增量消费 {}", Id::new())));
        fs::create_dir_all(p.0.join("decisions")).unwrap();
        p.write("work.yaml","goals:\n- id: G\n  title: Keep a useful guide\n  status: active\n  success_criteria: [A useful guide]\nwork_items:\n- id: W\n  title: Draft guide\n  status: ready\n  goal: G\n  acceptance: [A useful guide]\n  next_action: Write the outline\n");
        for key in ["D1", "D2"] {
            p.decision(key, "Keep the original format.");
        }
        p.write("mapping.toml","[project]\nname='Changes'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='decisions'\nadapter='markdown-directory-v1'\n");
        p.ok(&["init", "--manifest", "mapping.toml", "--accept"]);
        p
    }
    fn write(&self, path: &str, body: &str) {
        fs::write(self.0.join(path), body).unwrap();
    }
    fn decision(&self, key: &str, body: &str) {
        self.write(
            &format!("decisions/{key}.md"),
            &format!(
                "# {key}: Guide format\n\nID: {key}\nStatus: accepted\n\n## Decision\n\n{body}\n"
            ),
        );
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(["--project", self.0.to_str().unwrap(), "--json"])
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let r = self.run(args);
        assert!(
            r.status.success(),
            "{args:?}\n{}\n{}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn failed(&self, args: &[&str]) -> Value {
        let r = self.run(args);
        assert!(!r.status.success(), "{args:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&r.stderr).unwrap()["code"],
            "SourceStale"
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn revision(&self) -> String {
        self.ok(&["source", "list"])["project_revision"].to_string()
    }
    fn events(&self, after: &str, through: &str) -> Vec<Value> {
        let mut page = self.ok(&[
            "source",
            "changes",
            "--after-revision",
            after,
            "--through-revision",
            through,
            "--limit",
            "1",
        ]);
        let mut events = vec![];
        let mut ids = BTreeSet::new();
        loop {
            for event in page["changes"].as_array().unwrap() {
                assert!(ids.insert(event["event_id"].to_string()));
                events.push(event.clone());
            }
            assert_eq!(page["read_only"], true);
            assert_eq!(page["consumer_checkpoint_updated"], false);
            if !page["has_more"].as_bool().unwrap() {
                assert_eq!(
                    page["next_after_revision_when_processed"].to_string(),
                    through
                );
                break;
            }
            assert!(page["next_after_revision_when_processed"].is_null());
            page = self.ok(&[
                "source",
                "changes",
                "--cursor",
                &page["next_cursor"].to_string(),
                "--through-revision",
                through,
                "--limit",
                "1",
            ]);
        }
        events
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn no_change_refreshes_have_empty_event_windows_and_one_document_reports_only_its_facts() {
    let p = Project::new();
    let baseline = p.revision();
    for command in ["scan", "reindex", "reindex"] {
        let result = p.ok(&["source", command]);
        assert_eq!(result["project_revision"].to_string(), baseline);
        assert_eq!(
            result["change_window"]["after_revision"],
            result["change_window"]["through_revision"]
        );
        assert_eq!(result["projection_complete"], true);
    }
    assert!(p.events(&baseline, &baseline).is_empty());
    let before = p.ok(&["decision", "show", "D1", "--full"]);
    p.decision("D1", "Use revised examples in the same guide.");
    let changed = p.ok(&["source", "reindex"]);
    assert_eq!(changed["indexed"], 1);
    assert_eq!(changed["retired"], 0);
    let through = changed["change_window"]["through_revision"].to_string();
    // A later runtime event must not leak into the immutable source window or force a new page.
    p.ok(&[
        "event",
        "append",
        "--type",
        "host.note",
        "--summary",
        "Host observed the index operation",
        "--expected-revision",
        &through,
    ]);
    let after = p.revision();
    let events = p.events(&baseline, &through);
    assert_eq!(p.revision(), after);
    assert!(!events.is_empty());
    assert!(
        events
            .iter()
            .all(|e| e["source_id"] == before["source_ref"]["source_id"])
    );
    let projected = events
        .iter()
        .find(|e| e["event_type"] == "source.projected")
        .unwrap();
    assert_eq!(projected["content_changed"], true);
    assert_eq!(projected["projection_change_count"], 1);
    assert_eq!(projected["changes_included"], false);
    let full = p.ok(&[
        "event",
        "show",
        projected["event_id"].as_str().unwrap(),
        "--full",
    ]);
    assert_eq!(full["event"]["payload"]["changes"][0]["action"], "updated");
    assert_eq!(
        full["event"]["payload"]["changes"][0]["id"],
        before["decision"]["id"]
    );
    let latest = p.ok(&["decision", "show", "D1", "--full"]);
    assert_eq!(latest["decision"]["id"], before["decision"]["id"]);
    assert!(p.events(&through, &after).is_empty());
}

#[test]
fn failed_refresh_retains_pending_sources_and_restarting_from_old_success_does_not_skip_changes() {
    let p = Project::new();
    let baseline = p.revision();
    p.decision("D1", "Updated valid decision.");
    p.write(
        "decisions/D2.md",
        "---\nstatus: [broken\n---\n# Broken metadata\n",
    );
    let result = p.failed(&["source", "reindex"]);
    assert_eq!(result["indexed"], 1);
    assert_eq!(result["retired"], 0);
    assert_eq!(result["projection_complete"], false);
    assert!(!result["issues"].as_array().unwrap().is_empty());
    let feed = p.failed(&["source", "changes", "--after-revision", &baseline]);
    assert_eq!(feed["pending_source_total"], 1);
    assert!(feed["next_after_revision_when_processed"].is_null());
    assert!(
        feed["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["content_changed"] == true)
    );
    let before_retry = p.revision();
    p.failed(&["source", "reindex"]);
    assert_eq!(p.revision(), before_retry);
    let retry = p.failed(&["source", "changes", "--after-revision", &baseline]);
    assert_eq!(retry["changes"], feed["changes"]);
    p.decision("D2", "Fixed metadata and changed the decision.");
    p.ok(&["source", "reindex"]);
    let through = p.revision();
    let events = p.events(&baseline, &through);
    let changed = events
        .iter()
        .filter(|e| e["event_type"] == "source.projected" && e["content_changed"] == true)
        .map(|e| e["source_id"].to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(changed.len(), 2);
    let cursor = p.ok(&[
        "source",
        "changes",
        "--after-revision",
        &baseline,
        "--limit",
        "1",
    ])["next_cursor"]
        .to_string();
    let other = Project::new();
    let r = other.run(&[
        "source",
        "changes",
        "--cursor",
        &cursor,
        "--through-revision",
        &other.revision(),
    ]);
    assert!(!r.status.success());
}

#[test]
fn deleted_children_retire_but_unavailable_directories_do_not_and_history_survives() {
    let p = Project::new();
    let baseline = p.revision();
    let original = p.ok(&["decision", "show", "D1", "--full"]);
    let source = original["source_ref"]["source_id"].as_str().unwrap();
    fs::rename(p.0.join("decisions"), p.0.join("temporarily-offline")).unwrap();
    let unavailable = p.failed(&["source", "reindex"]);
    assert_eq!(unavailable["retired"], 0);
    let failed = p.failed(&["source", "changes", "--after-revision", &baseline]);
    assert_eq!(failed["pending_source_total"], 2);
    assert!(
        failed["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["event_type"] != "source.retired")
    );
    fs::rename(p.0.join("temporarily-offline"), p.0.join("decisions")).unwrap();
    p.ok(&["source", "reindex"]);
    assert_eq!(
        p.ok(&["decision", "show", "D1", "--full"])["decision"]["id"],
        original["decision"]["id"]
    );
    let before_delete = p.revision();
    fs::remove_file(p.0.join("decisions/D1.md")).unwrap();
    let removed = p.ok(&["source", "reindex"]);
    assert_eq!(removed["retired"], 1);
    let events = p.events(&before_delete, &p.revision());
    let retired = events
        .iter()
        .find(|e| e["event_type"] == "source.retired")
        .unwrap();
    assert_eq!(retired["source_id"], source);
    assert_eq!(retired["after"]["active"], false);
    assert_eq!(p.ok(&["source", "show", source])["active"], false);
    assert!(
        !p.ok(&["source", "history", source])["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    p.decision("D1", "Keep the original format.");
    p.ok(&["source", "reindex"]);
    assert_eq!(
        p.ok(&["decision", "show", "D1", "--full"])["decision"]["id"],
        original["decision"]["id"]
    );
}

#[cfg(unix)]
#[test]
fn unreadable_file_is_a_failure_not_a_retirement() {
    use std::os::unix::fs::PermissionsExt;
    let p = Project::new();
    let path = p.0.join("decisions/D1.md");
    let permissions = fs::metadata(&path).unwrap().permissions();
    let baseline = p.revision();
    fs::set_permissions(&path, fs::Permissions::from_mode(0)).unwrap();
    // Privileged runners can still read chmod(0); only assert actual access denial where enforced.
    if fs::read(&path).is_ok() {
        fs::set_permissions(&path, permissions).unwrap();
        return;
    }
    let result = p.failed(&["source", "reindex"]);
    fs::set_permissions(&path, permissions).unwrap();
    assert_eq!(result["retired"], 0);
    let feed = p.failed(&["source", "changes", "--after-revision", &baseline]);
    assert_eq!(feed["pending_source_total"], 1);
    assert!(
        feed["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["event_type"] != "source.retired")
    );
    p.ok(&["source", "reindex"]);
}
