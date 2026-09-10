use awr_core::Id;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Project(PathBuf);
impl Project {
    fn new(count: usize) -> Self {
        let p = Self(std::env::temp_dir().join(format!("awr 完整目录 {}", Id::new())));
        fs::create_dir(&p.0).unwrap();
        p.write("mapping.toml","[project]\nname='Catalog'\ncontext_profile='minimal'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[sources.options.status_map]\n'待处理'='planned'\n'`ready`'='ready'\n");
        p.write_ledger(count);
        p.ok(&["init", "--manifest", "mapping.toml", "--accept"]);
        p
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.0.join(path), text).unwrap();
    }
    fn write_ledger(&self, count: usize) {
        let mut text = String::from(
            "goals:\n- id: G\n  title: Same title\n  status: active\n  success_criteria: [A useful document]\nmilestones:\n- id: M\n  title: First phase\n  status: active\n  summary: Draft and review\nwork_items:\n",
        );
        for n in 0..count {
            let status = ["ready", "待处理", "`ready`", "unclear", "completed"][n % 5];
            text.push_str(&format!("- id: W{n:03}\n  title: Same title\n  owner: '资料编辑 张老师'\n  status: '{status}'\n  goal: G\n  milestone: M\n  depends_on: []\n  acceptance: [A useful document]\n  summary: '{}'\n", "original body ".repeat(40)));
        }
        self.write("work.yaml", &text);
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(["--project", self.0.to_str().unwrap(), "--json"])
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let r = self.run(args);
        assert!(
            r.status.success(),
            "{:?}\n{}\n{}",
            args,
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn error(&self, args: &[&str], code: &str) -> Output {
        let r = self.run(args);
        assert!(!r.status.success(), "{:?}", args);
        assert_eq!(
            serde_json::from_slice::<Value>(&r.stderr).unwrap()["code"],
            code
        );
        r
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_kind_can_be_browsed_and_work_pages_preserve_raw_values_and_sources() {
    let p = Project::new(53);
    p.write(
        "rules.md",
        "# Traceable facts {#R severity=hard scope=project value=*}\n\nKeep facts traceable.\n",
    );
    fs::create_dir(p.0.join("decisions")).unwrap();
    p.write(
        "decisions/D.md",
        "# D: Publish a guide\n\nStatus: accepted\n\n## Decision\n\nUse Markdown.\n",
    );
    let manifest = fs::read_to_string(p.0.join(".awr/project.toml")).unwrap();
    p.write(".awr/project.toml", &format!("{manifest}\n[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='decisions'\nadapter='markdown-directory-v1'\n"));
    let mut page = p.ok(&["object", "list", "work"]);
    let revision = page["project_revision"].clone();
    let mut seen = BTreeMap::new();
    loop {
        assert_eq!(page["total"], 53);
        assert_eq!(page["active_total"], 53);
        assert_eq!(page["retired_total"], 0);
        assert_eq!(page["project_revision"], revision);
        assert_eq!(page["total_is_current"], true);
        assert_eq!(page["content_included"], false);
        for item in page["items"].as_array().unwrap() {
            let key = item["external_key"].as_str().unwrap();
            assert!(seen.insert(key.to_owned(), item["id"].clone()).is_none());
            assert_eq!(item["title"], "Same title");
            assert_eq!(item["owner"], "资料编辑 张老师");
            assert!(
                item["source_ref"]["pointer"]
                    .as_str()
                    .unwrap()
                    .starts_with("/work_items/")
            );
            assert!(
                item["source"]["locator"]
                    .as_str()
                    .unwrap()
                    .ends_with("/work.yaml")
            );
            assert_eq!(
                item["source_revision"],
                item["source_ref"]["source_revision"]
            );
            let n = key[1..].parse::<usize>().unwrap();
            assert_eq!(
                item["raw_status"],
                ["ready", "待处理", "`ready`", "unclear", "completed"][n % 5]
            );
            assert_eq!(
                item["status"],
                ["ready", "planned", "ready", "unknown", "completed"][n % 5]
            );
            assert!(item["summary"].as_str().unwrap().len() < "original body ".repeat(40).len());
        }
        if !page["has_more"].as_bool().unwrap() {
            assert!(page["next_cursor"].is_null());
            break;
        }
        page = p.ok(&[
            "object",
            "list",
            "work",
            "--cursor",
            &page["next_cursor"].to_string(),
        ]);
    }
    assert_eq!(seen.len(), 53);
    for kind in [
        "goal", "plan", "rule", "decision", "source", "relation", "artifact", "evidence",
    ] {
        let v = p.ok(&["object", "list", kind, "--limit", "200"]);
        assert_eq!(
            v["items"].as_array().unwrap().len() as u64,
            v["total"].as_u64().unwrap()
        );
        assert_eq!(v["has_more"], false);
        if ["goal", "plan", "rule", "decision", "source"].contains(&kind) {
            assert_eq!(v["total"], if kind == "source" { 3 } else { 1 }, "{kind}");
        }
    }
    let before = seen["W000"].clone();
    let text = fs::read_to_string(p.0.join("work.yaml")).unwrap().replacen(
        "  title: Same title\n  owner:",
        "  title: Revised title\n  owner:",
        1,
    );
    p.write("work.yaml", &text);
    let after = p.ok(&["object", "show", "work", "W000", "--full"]);
    assert_eq!(after["object"]["id"], before);
    assert_eq!(after["object"]["title"], "Revised title");
    assert_eq!(after["object"]["summary"], "original body ".repeat(40));
    p.ok(&["source", "reindex"]);
    assert_eq!(
        p.ok(&["object", "show", "work", "W000"])["object"]["id"],
        before
    );
}

#[test]
fn cursors_reject_scope_project_and_source_revision_changes() {
    let p = Project::new(23);
    let first = p.ok(&["object", "list", "work"]);
    let cursor = first["next_cursor"].to_string();
    p.error(
        &["object", "list", "goal", "--cursor", &cursor],
        "InvalidInput",
    );
    p.error(
        &[
            "object", "list", "work", "--scope", "all", "--cursor", &cursor,
        ],
        "InvalidInput",
    );
    let other = Project::new(23);
    other.error(
        &["object", "list", "work", "--cursor", &cursor],
        "InvalidInput",
    );
    p.write_ledger(24);
    p.error(
        &["object", "list", "work", "--cursor", &cursor],
        "RevisionConflict",
    );
    assert_eq!(p.ok(&["object", "list", "work"])["total"], 24);
    p.error(
        &["object", "list", "work", "--limit", "201"],
        "InvalidInput",
    );
    p.error(
        &["object", "list", "work", "--cursor", "{}"],
        "InvalidInput",
    );
}

#[test]
fn removal_and_failed_refresh_have_distinct_totals_and_freshness() {
    let p = Project::new(23);
    p.write_ledger(22);
    assert_eq!(p.ok(&["object", "list", "work"])["total"], 22);
    let retired = p.ok(&["object", "list", "work", "--scope", "retired"]);
    assert_eq!(retired["total"], 1);
    assert_eq!(retired["items"][0]["external_key"], "W022");
    assert_eq!(retired["items"][0]["active"], false);
    assert_eq!(
        p.ok(&["object", "list", "work", "--scope", "all"])["total"],
        23
    );
    p.write("work.yaml", "work_items: [broken\n");
    let failed = p.error(&["object", "list", "work"], "SourceStale");
    let v: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["total"], 22);
    assert_eq!(v["total_is_current"], false);
    assert!(!v["source_issues"].as_array().unwrap().is_empty());
    assert!(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["freshness"] != "fresh")
    );
}

#[test]
fn full_source_reads_are_explicit_versioned_and_bounded() {
    let p = Project::new(1);
    let source = p.ok(&["object", "list", "source"])["items"][0].clone();
    let id = source["id"].as_str().unwrap();
    let meta = p.ok(&["source", "show", id]);
    assert!(meta.get("content").is_none());
    let full = p.ok(&[
        "source",
        "show",
        id,
        "--content",
        "--fingerprint",
        source["fingerprint"].as_str().unwrap(),
    ]);
    assert_eq!(
        full["content"],
        fs::read_to_string(p.0.join("work.yaml")).unwrap()
    );
    p.error(
        &["source", "show", id, "--content", "--max-bytes", "10"],
        "InvalidInput",
    );
    p.error(
        &[
            "object",
            "show",
            "work",
            "W000",
            "--full",
            "--max-bytes",
            "10",
        ],
        "InvalidInput",
    );
    p.write_ledger(2);
    p.error(&["source", "show", id, "--content"], "SourceConflict");
}

#[test]
fn removed_registration_retains_metadata_but_cannot_reopen_content() {
    let p = Project::new(1);
    let source = p.ok(&["object", "list", "source"])["items"][0].clone();
    let id = source["id"].as_str().unwrap();
    p.write("goal.md", "# Keep a guide\n\nA useful document.\n");
    p.write(".awr/project.toml", "[project]\nname='Catalog'\ncontext_profile='minimal'\n[[sources]]\ndomain='goal'\nrole='primary'\npath='goal.md'\nadapter='markdown-heading-v1'\n");
    // Revocation applies immediately, even before the next projection refresh.
    p.ok(&["source", "show", id]);
    p.error(&["source", "show", id, "--content"], "SourceUnavailable");
    let retired = p.ok(&["object", "list", "source", "--scope", "retired"]);
    assert_eq!(retired["total"], 1);
    assert_eq!(retired["items"][0]["id"], id);
    p.error(&["source", "show", id, "--content"], "SourceUnavailable");
    assert!(p.0.join("work.yaml").is_file());
}

#[test]
fn runtime_artifacts_and_source_or_runtime_evidence_share_bounded_reference_catalogs() {
    let p = Project::new(1);
    let ledger = fs::read_to_string(p.0.join("work.yaml")).unwrap();
    p.write(
        "work.yaml",
        &ledger.replace("  acceptance:", "  evidence: [guide.txt]\n  acceptance:"),
    );
    p.write("guide.txt", "An actual fixture guide.\n");
    let rev = p.ok(&["work", "show", "W000"])["project_revision"].to_string();
    let started = p.ok(&[
        "session",
        "start",
        "--work",
        "W000",
        "--agent",
        "fixture-writer",
        "--provider",
        "fixture",
        "--model",
        "none",
        "--expected-revision",
        &rev,
    ]);
    let imported = p.ok(&[
        "artifact",
        "add",
        "guide.txt",
        "--type",
        "guide",
        "--mime",
        "text/plain",
        "--source-event",
        started["event"]["id"].as_str().unwrap(),
        "--expected-revision",
        &started["project_revision"].to_string(),
    ]);
    let artifacts = p.ok(&["object", "list", "artifact"]);
    assert_eq!(artifacts["total"], 1);
    assert_eq!(artifacts["items"][0]["id"], imported["artifact"]["id"]);
    assert_eq!(
        artifacts["items"][0]["source_event_id"],
        started["event"]["id"]
    );
    assert_eq!(artifacts["items"][0]["content_included"], false);
    assert_eq!(
        p.ok(&["object", "list", "artifact", "--scope", "retired"])["total"],
        0
    );
    p.write("evidence.json",&json!({"external_key":"GUIDE-REVIEW","work_item_key":"W000","evidence_type":"review","level":"designed","summary":"A caller supplied reference","locator":imported["artifact"]["locator"],"sha256":imported["artifact"]["sha256"],"scope":["W000"]}).to_string());
    p.ok(&[
        "evidence",
        "add",
        "--input",
        "evidence.json",
        "--expected-revision",
        &artifacts["project_revision"].to_string(),
    ]);
    let first = p.ok(&["object", "list", "evidence", "--limit", "1"]);
    assert_eq!(first["total"], 2);
    assert_eq!(first["has_more"], true);
    let second = p.ok(&[
        "object",
        "list",
        "evidence",
        "--limit",
        "1",
        "--cursor",
        &first["next_cursor"].to_string(),
    ]);
    assert_eq!(second["has_more"], false);
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    assert!(first["items"][0]["source"].is_null() != second["items"][0]["source"].is_null());
    assert_eq!(p.ok(&["work", "show", "W000"])["work"]["status"], "ready");
}
