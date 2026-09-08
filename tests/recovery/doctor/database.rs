use awr_core::{CheckpointDraft, Id};
use awr_store::Store;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    sync::mpsc,
    time::Duration,
};

const WORK: &str = "work_items:\n- id: W\n  title: Prepare the recovery report\n  status: in_progress\n  next_action: Review the saved work\n  acceptance: [Preserve runtime history]\n- id: OTHER\n  title: Review another report\n  status: ready\n";
const RULES: &str = "# Preserve recorded work {severity=hard scope=project value=*}\n\nRetain the source facts and runtime receipts.\n";
const MANIFEST: &str = "[project]\nname='Database recovery'\nexternal_key='recovery'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-ledger-v1'\n[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n";

struct Fixture(PathBuf, bool);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-database-recovery-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("fixture-owner"), "awr-database-recovery-test").unwrap();
        fs::write(root.join("work.yaml"), WORK).unwrap();
        fs::write(root.join("rules.md"), RULES).unwrap();
        fs::write(root.join("sources.toml"), MANIFEST).unwrap();
        let f = Self(root, true);
        f.ok(&["init", "--manifest", "sources.toml", "--accept"]);
        f
    }
    fn db(&self) -> PathBuf {
        self.0.join(".awr/state.db")
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
        let r = self.run(args);
        assert!(
            r.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&r.stdout),
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn problems(&self, args: &[&str]) -> Value {
        let before = self.snapshot();
        let r = self.run(args);
        assert!(
            !r.status.success(),
            "Doctor incorrectly reported health: {}",
            String::from_utf8_lossy(&r.stdout)
        );
        let value: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(self.snapshot(), before);
        value
    }
    fn sql(&self, sql: &str) {
        assert_eq!(
            fs::read_to_string(self.0.join("fixture-owner")).unwrap(),
            "awr-database-recovery-test"
        );
        Connection::open(self.db())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }
    fn snapshot(&self) -> Value {
        let db = Connection::open_with_flags(self.db(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let schema: Vec<(String, String, String)> = db
            .prepare("SELECT type,name,coalesce(sql,'') FROM sqlite_master ORDER BY type,name")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let mut tables = BTreeMap::new();
        for (kind, name, _) in &schema {
            if kind != "table" {
                continue;
            }
            let mut query = db
                .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
                .unwrap();
            let count = query.column_count();
            let mut rows: Vec<Vec<String>> = query
                .query_map([], |r| {
                    Ok((0..count)
                        .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                        .collect())
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            rows.sort();
            tables.insert(name, rows);
        }
        let version: i64 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        let sources: BTreeMap<_, _> =
            ["work.yaml", "rules.md", "sources.toml", ".awr/project.toml"]
                .map(|name| {
                    (
                        name,
                        fs::read(self.0.join(name))
                            .ok()
                            .map(|bytes| awr_source::fingerprint(&bytes)),
                    )
                })
                .into_iter()
                .collect();
        json!({"schema":schema,"tables":tables,"version":version,"sources":sources})
    }
    fn downgrade_to_v2(&self) {
        self.sql("DROP TABLE search_fts; DROP TABLE search_state; DROP TABLE search_documents; DELETE FROM schema_migrations WHERE version=3; PRAGMA user_version=2;");
    }
    fn revision(&self) -> String {
        Connection::open_with_flags(self.db(), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap()
            .query_row("SELECT project_revision FROM projects", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
            .to_string()
    }
    fn start(&self, expired: bool) -> Value {
        let revision = self.revision();
        let mut args = vec![
            "session",
            "start",
            "--work",
            "W",
            "--agent",
            "primary-reviewer",
            "--provider",
            "fixture",
            "--model",
            "no-model-call",
            "--claim",
            "--expected-revision",
            &revision,
        ];
        if expired {
            args.extend(["--ttl-ms", "1"]);
        }
        self.ok(&args)
    }
    fn checkpoint(&self, sid: &str) -> Value {
        self.ok(&[
            "session",
            "checkpoint",
            "--session",
            sid,
            "--context-hash",
            &"a".repeat(64),
            "--digest",
            "Saved the report assumptions",
            "--next-action",
            "Review the retained report",
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn artifact(&self, source_event: &str) -> Value {
        fs::write(self.0.join("report.txt"), "PRIVATE_FIXTURE_REPORT_BODY").unwrap();
        self.ok(&[
            "artifact",
            "add",
            "report.txt",
            "--type",
            "report",
            "--mime",
            "text/plain",
            "--source-event",
            source_event,
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn proposal(&self) -> Value {
        self.ok(&[
            "proposal",
            "create",
            "--kind",
            "work",
            "--target",
            "W",
            "--intent",
            "Revise the report handoff",
            "--patch",
            r#"{"next_action":"Review the saved revisions"}"#,
            "--expected-revision",
            &self.revision(),
        ])
    }
    fn repair(&self, action: &str, id: &str) -> Value {
        self.ok(&[
            "doctor",
            "repair",
            action,
            id,
            "--expected-revision",
            &self.revision(),
            "--reason",
            "The selected interrupted fixture requires cleanup",
        ])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn passed(id: &str) {
    println!("AWR_DATABASE_CASE {id}");
}
fn has(report: &Value, code: &str) -> bool {
    report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["code"] == code)
}
fn assert_runtime_retained(before: &Value, after: &Value) {
    for table in [
        "sessions",
        "claims",
        "checkpoints",
        "artifacts",
        "branches",
        "mutation_proposals",
    ] {
        assert_eq!(
            after["tables"][table], before["tables"][table],
            "changed runtime table {table}"
        );
    }
    for event in before["tables"]["events"].as_array().unwrap() {
        assert!(
            after["tables"]["events"]
                .as_array()
                .unwrap()
                .contains(event),
            "retained event changed"
        );
    }
}

#[test]
fn case_schema_objects_and_guards_are_inspected_before_open() {
    for change in [
        "DROP TRIGGER event_no_delete",
        "DROP TRIGGER event_no_update; CREATE TRIGGER event_no_update BEFORE UPDATE ON events BEGIN SELECT 1; END;",
        "DROP INDEX active_claim",
        "DROP TABLE context_packs",
        "DROP TABLE schema_migrations",
        "ALTER TABLE work_items RENAME COLUMN next_action TO moved_next_action",
    ] {
        let f = Fixture::new();
        f.sql(change);
        let before = f.snapshot();
        let report = f.problems(&["doctor", "--database-only"]);
        assert!(
            !report["schema_issues"].as_array().unwrap().is_empty(),
            "{change}: {report}"
        );
        for result in [
            Store::open_readonly(&f.db()),
            Store::open_existing(&f.db()),
            Store::open(&f.db()),
        ] {
            assert!(result.is_err(), "invalid schema was opened after {change}");
        }
        assert_eq!(f.snapshot(), before);
    }
    passed("schema_objects_and_guards");
}

#[test]
fn case_schema_migration_catalog_checks_record_identity() {
    for sql in [
        "DELETE FROM schema_migrations WHERE version=1",
        "UPDATE schema_migrations SET name='other' WHERE version=2",
        "INSERT INTO schema_migrations VALUES(4,'unsupported',0)",
    ] {
        let f = Fixture::new();
        f.sql(sql);
        let before = f.snapshot();
        let report = f.problems(&["doctor", "--database-only"]);
        assert_eq!(report["schema_version"], 3);
        assert!(Store::open_existing(&f.db()).is_err());
        assert!(Store::open(&f.db()).is_err());
        assert_eq!(f.snapshot(), before);
    }
    passed("migration_catalog_consistency");
}

#[test]
fn case_migration_rejects_invalid_base_before_any_upgrade_commits() {
    let f = Fixture::new();
    f.downgrade_to_v2();
    f.sql("DELETE FROM schema_migrations WHERE version=1");
    let before = f.snapshot();
    assert!(Store::open(&f.db()).is_err());
    assert_eq!(
        f.snapshot(),
        before,
        "a rejected migration committed schema or catalog changes"
    );
    passed("migration_invalid_base_rollback");
}

#[test]
fn case_missing_and_foreign_databases_are_not_created_or_adopted() {
    let f = Fixture::new();
    let path = f.0.join("missing.db");
    for args in [
        vec![
            "doctor",
            "--database-only",
            "--database",
            path.to_str().unwrap(),
        ],
        vec![
            "doctor",
            "--database",
            path.to_str().unwrap(),
            "repair",
            "interrupt-session",
            &"0".repeat(26),
            "--expected-revision",
            "0",
            "--reason",
            "Inspect a missing fixture",
        ],
    ] {
        assert!(!f.run(&args).status.success());
        assert!(!path.exists());
    }
    passed("missing_database_preserved");
    let path = f.0.join("unrelated.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE notes(body TEXT); INSERT INTO notes VALUES('Keep original data');",
        )
        .unwrap();
    let before = fs::read(&path).unwrap();
    assert!(Store::open(&path).is_err());
    assert!(Store::inspect(&path).is_err());
    assert!(
        !f.run(&["doctor", "--database", path.to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    passed("foreign_database_preserved");
}

#[test]
fn case_old_and_future_versions_are_only_inspected() {
    for version in [2, 99] {
        let f = Fixture::new();
        if version == 2 {
            f.downgrade_to_v2();
        } else {
            f.sql("PRAGMA user_version=99");
        }
        let before = f.snapshot();
        let report = f.problems(&["doctor"]);
        assert_eq!(report["schema_version"], version);
        assert!(has(&report, "runtime_checks_unavailable"));
        assert!(Store::open_readonly(&f.db()).is_err());
        assert!(Store::open_existing(&f.db()).is_err());
        if version == 99 {
            assert!(Store::open(&f.db()).is_err());
        }
        assert_eq!(f.snapshot(), before);
    }
    passed("schema_versions_readonly");
}

#[test]
fn case_malformed_and_truncated_database_bytes_never_become_healthy() {
    let f = Fixture::new();
    let source = fs::read(f.db()).unwrap();
    for bytes in [
        b"This is not a SQLite database".to_vec(),
        source[..source.len() / 3].to_vec(),
    ] {
        let path = f.0.join(format!("damaged-{}.db", Id::new()));
        fs::write(&path, &bytes).unwrap();
        assert!(
            !f.run(&["doctor", "--database", path.to_str().unwrap()])
                .status
                .success()
        );
        assert!(Store::open(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    passed("database_corruption_detected");
}

#[test]
fn case_migration_sql_failure_rolls_back_and_supported_versions_keep_history() {
    let f = Fixture::new();
    let started = f.start(false);
    f.checkpoint(started["session"]["id"].as_str().unwrap());
    f.downgrade_to_v2();
    f.sql("CREATE TRIGGER reject_fixture_migration BEFORE INSERT ON schema_migrations WHEN NEW.version=3 BEGIN SELECT RAISE(ABORT,'fixture migration failure'); END;");
    let before = f.snapshot();
    assert!(Store::open(&f.db()).is_err());
    assert_eq!(f.snapshot(), before);
    passed("migration_sql_failure_rollback");
    f.sql("DROP TRIGGER reject_fixture_migration");
    let before = f.snapshot();
    drop(Store::open(&f.db()).unwrap());
    let after = f.snapshot();
    assert_eq!(after["version"], 3);
    assert_runtime_retained(&before, &after);
    assert_eq!(before["tables"]["sources"], after["tables"]["sources"]);
    assert_eq!(f.ok(&["doctor"])["database_ok"], true);
    let v1 = f.0.join("catalog-v1.db");
    {
        let db = Connection::open(&v1).unwrap();
        db.execute_batch(include_str!(
            "../../../crates/awr-store/migrations/001_catalog.sql"
        ))
        .unwrap();
        db.execute_batch("INSERT INTO schema_migrations VALUES(1,'catalog',42); PRAGMA user_version=1; PRAGMA application_id=1096241713;").unwrap();
    }
    drop(Store::open(&v1).unwrap());
    let migrated = Store::inspect(&v1).unwrap();
    assert!(migrated.ok);
    assert_eq!(migrated.migrations[0].applied_at, 42);
    passed("migration_success_preserves_runtime");
}

#[test]
fn case_missing_dependencies_and_dangling_edges_remain_visible() {
    let f = Fixture::new();
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace("  acceptance:", "  depends_on: [ABSENT]\n  acceptance:"),
    )
    .unwrap();
    f.ok(&["source", "reindex"]);
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "missing_dependency"));
    passed("missing_dependency");
    f.sql(&format!("INSERT INTO edges(id,project_id,from_kind,from_key,relation,to_kind,to_key,source_id,source_ref_json,source_revision,revision) SELECT '{}',project_id,'work_item','MISSING_ORIGIN','blocks','work_item','W',id,'{{}}',revision,1 FROM sources WHERE domain='ledger'",Id::new()));
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "dangling_edge"));
    assert!(has(&report, "missing_dependency"));
    passed("dangling_edge");
}

#[test]
fn case_foreign_key_damage_preserves_diagnostics_and_disables_repairs() {
    let f = Fixture::new();
    let s = f.start(false);
    let sid = s["session"]["id"].as_str().unwrap();
    f.sql(&format!(
        "PRAGMA foreign_keys=OFF; UPDATE sessions SET work_item_id='{}' WHERE id='{sid}'",
        Id::new()
    ));
    let before = f.snapshot();
    let report = f.problems(&["doctor"]);
    assert_eq!(report["database_ok"], false);
    assert!(report["foreign_key_violations"].as_u64().unwrap() > 0);
    assert!(has(&report, "orphan_session") && has(&report, "invalid_claim"));
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["repair"].is_null())
    );
    assert!(
        !f.run(&[
            "doctor",
            "repair",
            "interrupt-session",
            sid,
            "--reason",
            "Inspect damaged bindings",
            "--expected-revision",
            &f.revision()
        ])
        .status
        .success()
    );
    assert_eq!(f.snapshot(), before);
    let uncheckable = Fixture::new();
    uncheckable.sql(
        "CREATE TABLE invalid_fixture_foreign_key(id TEXT REFERENCES work_items(absent_column))",
    );
    let report = uncheckable.problems(&["doctor", "--database-only"]);
    assert!(report["foreign_key_check_error"].is_string());
    passed("inconsistent_runtime_bindings");
}

#[test]
fn case_invalid_branch_pointer_repair_retains_its_history() {
    let f = Fixture::new();
    let b = f.ok(&[
        "branch",
        "create",
        "review",
        "--actor",
        "reviewer",
        "--reason",
        "Keep an independent review",
        "--expected-revision",
        &f.revision(),
    ]);
    let id = b["branch"]["id"].as_str().unwrap();
    f.sql(&format!("UPDATE projects SET current_branch_id='{id}'; UPDATE branches SET fork_project_revision=999999 WHERE id='{id}';"));
    let before = f.snapshot();
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "invalid_branch"));
    f.repair("clear-invalid-branch", id);
    let after = f.snapshot();
    assert_runtime_retained(&before, &after);
    let db = Connection::open_with_flags(f.db(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert!(
        db.query_row("SELECT current_branch_id FROM projects", [], |r| r
            .get::<_, Option<String>>(0))
            .unwrap()
            .is_none()
    );
    passed("invalid_branch_pointer");
}

#[test]
fn case_incomplete_checkpoint_retains_the_previous_checkpoint() {
    let f = Fixture::new();
    let s = f.start(false);
    let sid = s["session"]["id"].as_str().unwrap();
    let cp = f.checkpoint(sid);
    let attempt = {
        let mut store = Store::open_existing(&f.db()).unwrap();
        let p = store.project_by_root(&f.0).unwrap();
        store
            .begin_checkpoint_save(
                p.id,
                p.project_revision,
                sid.parse().unwrap(),
                CheckpointDraft {
                    context_hash: "b".repeat(64),
                    digest: "An unfinished report revision".into(),
                    next_action: "Review the draft before saving".into(),
                    open_loops: vec![],
                    changed_entities: vec![],
                },
            )
            .unwrap()
    };
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "incomplete_checkpoint"));
    assert_eq!(
        f.ok(&["session", "show", sid])["checkpoint"]["id"],
        cp["checkpoint"]["id"]
    );
    f.repair("abandon-checkpoint", &attempt.id.to_string());
    let shown = f.ok(&["session", "show", sid]);
    assert_eq!(shown["checkpoint"]["id"], cp["checkpoint"]["id"]);
    assert_eq!(shown["checkpoint_saves"]["incomplete_count"], 0);
    passed("incomplete_checkpoint");
}

#[test]
fn case_pending_and_failed_mutations_are_not_replayed_by_doctor() {
    let f = Fixture::new();
    let pending = f.proposal();
    let failed = f.proposal();
    let pending = pending["proposal"]["id"].as_str().unwrap();
    let failed = failed["proposal"]["id"].as_str().unwrap();
    f.sql(&format!(
        "UPDATE mutation_proposals SET status='failed' WHERE id='{failed}'"
    ));
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "pending_mutation") && has(&report, "failed_mutation"));
    for id in [pending, failed] {
        assert!(
            report["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["object_id"] == id && f["repair"].is_null())
        );
    }
    assert_eq!(fs::read_to_string(f.0.join("work.yaml")).unwrap(), WORK);
    passed("pending_and_failed_mutation");
}

#[test]
fn case_changed_and_missing_sources_never_rewrite_retained_projections() {
    let f = Fixture::new();
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace("Review the saved work", "Review the new source facts"),
    )
    .unwrap();
    assert!(has(&f.problems(&["doctor"]), "source_fingerprint_changed"));
    fs::remove_file(f.0.join("work.yaml")).unwrap();
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "source_access_failed"), "{report}");
    passed("source_access_and_fingerprint");
}

#[test]
fn case_missing_manifest_keeps_expiry_visible_and_cleanup_explicit() {
    let f = Fixture::new();
    let started = f.start(true);
    fs::remove_file(f.0.join(".awr/project.toml")).unwrap();
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "source_manifest_unavailable") && has(&report, "expired_claim"));
    f.repair("expire-claim", started["claim"]["id"].as_str().unwrap());
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "source_manifest_unavailable") && !has(&report, "expired_claim"));
    assert!(!f.0.join(".awr/project.toml").exists());
    passed("missing_manifest_keeps_runtime_diagnostics");
}

#[test]
fn case_artifact_integrity_orphans_and_read_limits_are_explicit() {
    let f = Fixture::new();
    let s = f.start(false);
    let a = f.artifact(s["event"]["id"].as_str().unwrap());
    let file = f.0.join(a["artifact"]["locator"].as_str().unwrap());
    let limited = f.problems(&["doctor", "--max-bytes", "1"]);
    assert!(has(&limited, "artifact_unverified"));
    assert_eq!(limited["artifacts_checked"], 0);
    assert!(!limited.to_string().contains("PRIVATE_FIXTURE"));
    fs::write(f.0.join("rules.md"), "S".repeat(16 * 1024 * 1024 + 1)).unwrap();
    let oversized = f.problems(&["doctor"]);
    assert!(has(&oversized, "source_access_failed"));
    assert!(oversized.to_string().len() < 12000);
    fs::write(f.0.join("rules.md"), RULES).unwrap();
    passed("diagnostic_read_limits");
    let original = fs::read(&file).unwrap();
    fs::write(&file, vec![b'X'; original.len()]).unwrap();
    let orphan = f.0.join(".awr/artifacts/unregistered-report");
    fs::write(&orphan, "PRIVATE_FIXTURE_ORPHAN").unwrap();
    let report = f.problems(&["doctor"]);
    assert!(has(&report, "artifact_digest_mismatch") && has(&report, "orphan_artifact"));
    assert!(!report.to_string().contains("PRIVATE_FIXTURE"));
    assert_eq!(
        fs::read_to_string(&orphan).unwrap(),
        "PRIVATE_FIXTURE_ORPHAN"
    );
    fs::remove_file(file).unwrap();
    assert!(has(&f.problems(&["doctor"]), "artifact_missing"));
    passed("artifact_integrity_and_orphans");
}

#[test]
fn case_projection_rebuild_keeps_runtime_ownership_and_old_receipts() {
    let f = Fixture::new();
    let started = f.start(false);
    let sid = started["session"]["id"].as_str().unwrap();
    f.checkpoint(sid);
    f.artifact(started["event"]["id"].as_str().unwrap());
    let before = f.snapshot();
    fs::write(
        f.0.join("work.yaml"),
        WORK.replace("Prepare the recovery report", "Revised recovery report"),
    )
    .unwrap();
    f.ok(&["source", "reindex"]);
    let after = f.snapshot();
    assert_runtime_retained(&before, &after);
    assert_eq!(
        f.ok(&["work", "show", "W"])["work"]["title"],
        "Revised recovery report"
    );
    let hits = f.ok(&["search", "Revised", "--type", "work"]);
    assert!(
        hits["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["external_key"] == "W")
    );
    assert_runtime_retained(&after, &f.snapshot());
    assert_eq!(f.ok(&["session", "show", sid])["session"]["id"], sid);
    passed("projection_rebuild_preserves_runtime");
}

#[test]
fn case_source_only_rebuild_does_not_invent_missing_runtime_history() {
    let original = Fixture::new();
    let session = original.start(false);
    let sid = session["session"]["id"].as_str().unwrap();
    original.checkpoint(sid);
    original.artifact(session["event"]["id"].as_str().unwrap());
    let rebuilt = Fixture::new();
    assert_eq!(
        fs::read(original.0.join("work.yaml")).unwrap(),
        fs::read(rebuilt.0.join("work.yaml")).unwrap()
    );
    assert_eq!(
        rebuilt.ok(&["work", "show", "W"])["work"]["title"],
        "Prepare the recovery report"
    );
    let fresh = rebuilt.snapshot();
    for table in ["sessions", "claims", "checkpoints", "artifacts"] {
        assert!(fresh["tables"][table].as_array().unwrap().is_empty());
    }
    for event in original.snapshot()["tables"]["events"].as_array().unwrap() {
        assert!(
            !fresh["tables"]["events"]
                .as_array()
                .unwrap()
                .contains(event)
        );
    }
    assert!(!rebuilt.run(&["session", "show", sid]).status.success());
    passed("source_only_rebuild_is_not_runtime_restore");
}

#[test]
fn case_consistent_backup_plus_artifacts_restores_original_runtime() {
    let f = Fixture::new();
    let s = f.start(false);
    let sid = s["session"]["id"].as_str().unwrap();
    let checkpoint = f.checkpoint(sid);
    let artifact = f.artifact(s["event"]["id"].as_str().unwrap());
    let path = f.0.join(artifact["artifact"]["locator"].as_str().unwrap());
    let saved_artifact = f.0.join("retained-artifact");
    fs::copy(&path, &saved_artifact).unwrap();
    let before = f.snapshot();
    let backup = f.0.join("retained-backup.db");
    {
        let source = Connection::open_with_flags(f.db(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let mut destination = Connection::open(&backup).unwrap();
        assert!(matches!(
            rusqlite::backup::Backup::new(&source, &mut destination)
                .unwrap()
                .step(-1)
                .unwrap(),
            rusqlite::backup::StepResult::Done
        ));
    }
    f.ok(&[
        "event",
        "append",
        "--session",
        sid,
        "--type",
        "report.observed",
        "--summary",
        "A later fixture observation",
        "--expected-revision",
        &f.revision(),
    ]);
    assert_ne!(f.snapshot(), before);
    // Every process/connection above has finished. Replace only this marked
    // fixture's database and preserve the displaced files for the fixture lifetime.
    for suffix in ["", "-wal", "-shm"] {
        let source = f.0.join(format!(".awr/state.db{suffix}"));
        if source.exists() {
            fs::rename(source, f.0.join(format!("later-state.db{suffix}"))).unwrap();
        }
    }
    fs::copy(&backup, f.db()).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(f.snapshot(), before);
    assert!(has(&f.problems(&["doctor"]), "artifact_missing"));
    fs::copy(&saved_artifact, &path).unwrap();
    let report = f.ok(&["doctor"]);
    assert_eq!(report["artifacts_checked"], 1);
    let shown = f.ok(&["session", "show", sid]);
    assert_eq!(shown["checkpoint"]["id"], checkpoint["checkpoint"]["id"]);
    assert_eq!(shown["claims"][0]["id"], s["claim"]["id"]);
    assert_eq!(f.snapshot(), before);
    passed("consistent_backup_restores_runtime");
}

struct CrashWriter {
    child: Child,
    ready: Value,
}
impl CrashWriter {
    fn new(f: &Fixture, uncommitted: bool) -> Self {
        let spec = f.0.join("wal-writer.json");
        fs::write(
            &spec,
            serde_json::to_vec(&json!({"root":f.0,"uncommitted":uncommitted})).unwrap(),
        )
        .unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fixture_wal_writer",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("AWR_DATABASE_FIXTURE", &spec)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut writer = Self {
            child,
            ready: Value::Null,
        };
        let output = writer.child.stdout.take().unwrap();
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                if let Ok(line) = line {
                    if let Some((_, value)) = line.split_once("AWR_DATABASE_WRITER_READY ") {
                        let _ = send.send(value.to_owned());
                        break;
                    }
                }
            }
        });
        writer.ready = serde_json::from_str(
            &receive
                .recv_timeout(Duration::from_secs(20))
                .expect("live writer did not become ready"),
        )
        .unwrap();
        assert_eq!(writer.ready["pid"], writer.child.id());
        assert!(writer.child.try_wait().unwrap().is_none());
        assert!(writer.ready["wal_bytes"].as_u64().unwrap() > 32);
        writer
    }
    fn kill(&mut self) {
        assert!(self.child.try_wait().unwrap().is_none());
        self.child.kill().unwrap();
        assert!(!self.child.wait().unwrap().success());
    }
}
impl Drop for CrashWriter {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "Private live writer for the temporary WAL fixtures"]
fn fixture_wal_writer() {
    let path = PathBuf::from(
        std::env::var_os("AWR_DATABASE_FIXTURE").expect("fixture specification required"),
    );
    let spec: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let f = Fixture(PathBuf::from(spec["root"].as_str().unwrap()), false);
    assert_eq!(path.parent().unwrap(), f.0);
    assert_eq!(
        fs::read_to_string(f.0.join("fixture-owner")).unwrap(),
        "awr-database-recovery-test"
    );
    let conn = Connection::open(f.db()).unwrap();
    conn.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let s = f.start(false);
    let sid = s["session"]["id"].as_str().unwrap();
    let checkpoint = f.checkpoint(sid);
    if spec["uncommitted"] == true {
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        conn.execute(
            "UPDATE sessions SET agent_id='uncommitted-reviewer' WHERE id=?1",
            [sid],
        )
        .unwrap();
        conn.execute_batch("UPDATE projects SET project_revision=project_revision+100")
            .unwrap();
        conn.execute("INSERT INTO events(id,project_id,work_item_id,session_id,branch_id,event_type,importance,summary,payload_json,project_revision,created_at) SELECT ?1,project_id,work_item_id,session_id,branch_id,'report.observed','normal','Uncommitted observation','{}',project_revision+100,created_at FROM events LIMIT 1",[Id::new().to_string()]).unwrap();
    }
    let wal = f.0.join(".awr/state.db-wal");
    println!(
        "AWR_DATABASE_WRITER_READY {}",
        json!({"pid":std::process::id(),"session":sid,"checkpoint":checkpoint["checkpoint"]["id"],"wal_bytes":fs::metadata(wal).unwrap().len()})
    );
    std::io::stdout().flush().unwrap();
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    panic!("parent should terminate the confirmed live fixture writer");
}

fn crash_reopen(uncommitted: bool) {
    let f = Fixture::new();
    let mut writer = CrashWriter::new(&f, uncommitted);
    let before = f.snapshot();
    assert_eq!(before["tables"]["sessions"].as_array().unwrap().len(), 1);
    // Prove the committed rows are still WAL-dependent, not merely reading a
    // database that was already checkpointed before the child was killed.
    let main_only = f.0.join("main-file-without-wal.db");
    fs::copy(f.db(), &main_only).unwrap();
    let copy = Connection::open_with_flags(main_only, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        copy.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(copy);
    writer.kill();
    drop(Store::open_existing(&f.db()).unwrap());
    assert_eq!(f.snapshot(), before);
    let sid = writer.ready["session"].as_str().unwrap();
    let shown = f.ok(&["session", "show", sid]);
    assert_eq!(shown["session"]["agent_id"], "primary-reviewer");
    assert_eq!(shown["checkpoint"]["id"], writer.ready["checkpoint"]);
    let report = f.ok(&["doctor"]);
    assert_eq!(report["database_ok"], true);
    assert_eq!(f.snapshot(), before);
}

#[test]
fn case_committed_wal_reopens_after_a_live_writer_is_killed() {
    crash_reopen(false);
    passed("wal_committed_process_death");
}
#[test]
fn case_uncommitted_wal_rolls_back_after_a_live_writer_is_killed() {
    crash_reopen(true);
    passed("wal_uncommitted_process_death");
}
