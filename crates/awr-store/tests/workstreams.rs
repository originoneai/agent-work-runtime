use awr_core::*;
use awr_store::{SourceRegistration, Store};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct LegacyFixture {
    root: PathBuf,
    project: Id,
    work: Id,
}
impl LegacyFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-stream-migrate-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        let mut store = Store::open(&root.join("state.db")).unwrap();
        let project = store
            .register_project(&root, "fixture", "Fixture")
            .unwrap()
            .id;
        let source = store
            .register_source(
                project,
                &SourceRegistration {
                    domain: "ledger",
                    role: "primary",
                    locator: "file:///fixture/work.yaml",
                    format: "yaml",
                    adapter: "yaml-ledger-v1",
                },
            )
            .unwrap();
        let work = Id::new();
        let item: WorkItem = serde_json::from_value(json!({
            "id":work,"external_key":"WORK-1","revision":1,
            "source_ref":{"source_id":source.id,"locator":source.locator,"source_revision":source.revision+1,"source_fingerprint":"fixture-v1"},
            "title":"Keep existing history","kind":null,"owner":null,"required":true,
            "raw_status":"planned","status":"planned","priority":null,"milestone":null,"score":null,"evidence_level":null,
            "summary":"Synthetic work","next_action":"Implement","blocker":null,"acceptance":[],"tags":[],"paths":[]
        })).unwrap();
        store
            .commit_source_projection(
                &source,
                "fixture-v1",
                ProjectionBatch {
                    work_items: vec![item],
                    ..Default::default()
                },
            )
            .unwrap();
        let revision = store.project(project).unwrap().project_revision;
        let (session, event) = store
            .start_session(
                project,
                revision,
                SessionDraft {
                    work_item_key: Some("WORK-1".into()),
                    agent_id: "fixture".into(),
                    provider: "fixture".into(),
                    model: "fixture".into(),
                    branch_id: None,
                    claim: true,
                    claim_ttl_ms: Some(3_600_000),
                },
            )
            .unwrap();
        let (_, event) = store
            .create_checkpoint(
                project,
                event.project_revision,
                session.session.id,
                CheckpointDraft {
                    context_hash: "a".repeat(64),
                    digest: "Existing checkpoint".into(),
                    next_action: "Continue".into(),
                    open_loops: vec!["Finish work".into()],
                    changed_entities: vec![],
                },
            )
            .unwrap();
        store
            .record_evidence(
                project,
                event.project_revision,
                EvidenceDraft {
                    external_key: "REPORT-1".into(),
                    work_item_key: Some("WORK-1".into()),
                    evidence_type: "verification_report".into(),
                    level: EvidenceLevel::LocallyVerified,
                    summary: "Synthetic pre-migration evidence".into(),
                    locator: "fixture-report.json".into(),
                    sha256: Some("b".repeat(64)),
                    source_sha: Some("c".repeat(40)),
                    command: Some("fixture check".into()),
                    scope: vec!["fixture".into()],
                    branch_id: None,
                    verified_at: Some(1),
                },
            )
            .unwrap();
        drop(store);
        let fixture = Self {
            root,
            project,
            work,
        };
        // Reconstruct the exact shipped v4 schema; all pre-existing rows remain.
        fixture.sql("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE; DROP TABLE source_content_reviews; DROP TABLE conversation_workstreams; DROP TABLE session_workstreams; DROP TRIGGER session_identity_no_update; DROP TRIGGER workstream_claim_exclusive_insert; DROP TRIGGER workstream_claim_exclusive_update; DELETE FROM schema_migrations WHERE version>=6; DROP TABLE workstream_ownership; DROP TABLE workstreams; DROP TABLE workstream_catalogs; DELETE FROM schema_migrations WHERE version=5; PRAGMA user_version=4; COMMIT;");
        fixture
    }
    fn path(&self) -> PathBuf {
        self.root.join("state.db")
    }
    fn sql(&self, sql: &str) {
        Connection::open(self.path())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }
    fn rows(&self) -> Value {
        let conn = Connection::open(self.path()).unwrap();
        let mut tables = serde_json::Map::new();
        for table in [
            "projects",
            "sources",
            "work_items",
            "sessions",
            "claims",
            "checkpoints",
            "evidence",
            "events",
        ] {
            let mut stmt = conn
                .prepare(&format!("SELECT * FROM {table} ORDER BY id"))
                .unwrap();
            let count = stmt.column_count();
            let rows = stmt
                .query_map([], |row| {
                    (0..count)
                        .map(|i| {
                            row.get::<_, rusqlite::types::Value>(i)
                                .map(|v| format!("{v:?}"))
                        })
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            tables.insert(table.into(), json!(rows));
        }
        json!(tables)
    }
    fn schema(&self) -> Value {
        let conn = Connection::open(self.path()).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        let rows = conn
            .prepare("SELECT name,sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")
            .unwrap()
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let migrations = conn
            .prepare("SELECT version,name,applied_at FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        json!({"version":version,"schema":rows,"migrations":migrations})
    }
}
impl Drop for LegacyFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn migration_preview_and_apply_preserve_work_and_all_runtime_history() {
    let f = LegacyFixture::new();
    let before = f.rows();
    let schema = f.schema();
    let file_before = fs::read(f.path()).unwrap();
    assert!(Store::open_existing(&f.path()).is_err());
    let preview = Store::preview_snapshot(&f.path(), 32 * 1024 * 1024).unwrap();
    let expected = preview.workstream_binding(f.project, f.work).unwrap();
    assert_eq!(
        preview
            .workstream_catalog(f.project)
            .unwrap()
            .workstreams
            .len(),
        1
    );
    assert_eq!(f.schema(), schema);
    assert_eq!(fs::read(f.path()).unwrap(), file_before);
    drop(preview);
    let migrated = Store::open(&f.path()).unwrap();
    assert_eq!(
        migrated.workstream_binding(f.project, f.work).unwrap(),
        expected
    );
    assert_eq!(f.rows(), before);
    assert!(Store::inspect(&f.path()).unwrap().ok);
    assert_eq!(f.schema()["version"], awr_store::SCHEMA_VERSION);
    drop(migrated);
    let reopened = Store::open(&f.path()).unwrap();
    assert_eq!(
        reopened.workstream_binding(f.project, f.work).unwrap(),
        expected
    );
    assert_eq!(f.rows(), before);
}

#[test]
fn failed_migration_rolls_back_catalog_ownership_and_schema_then_recovers() {
    let f = LegacyFixture::new();
    f.sql("CREATE TRIGGER fail_scope_migration BEFORE INSERT ON schema_migrations WHEN NEW.version=5 BEGIN SELECT RAISE(ABORT,'fixture failure'); END;");
    let before = f.rows();
    let schema = f.schema();
    assert!(Store::open(&f.path()).is_err());
    assert_eq!(f.schema(), schema);
    assert_eq!(f.rows(), before);
    f.sql("DROP TRIGGER fail_scope_migration;");
    let recovered = Store::open(&f.path()).unwrap();
    let expected = WorkstreamCatalog::legacy(f.project.to_string()).unwrap();
    assert_eq!(recovered.workstream_catalog(f.project).unwrap(), expected);
    assert_eq!(
        recovered
            .workstream_binding(f.project, f.work)
            .unwrap()
            .workstream_id,
        expected.legacy_default.unwrap()
    );
    assert_eq!(f.rows(), before);
    assert!(Store::inspect(&f.path()).unwrap().ok);
}
