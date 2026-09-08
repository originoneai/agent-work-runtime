//! Internal SQL checks complement the public Store API compile-fail checks.
use super::*;
use awr_core::{EventDraft, Id};
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-event-guard-{}", Id::new()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn event_rows_survive_every_sql_mutation_form_on_all_store_connections() {
    let fixture = Fixture::new();
    let path = fixture.0.join("state.db");
    let mut created = Store::open(&path).unwrap();
    let project = created
        .register_project(&fixture.0, "report", "Report review")
        .unwrap();
    let event = created
        .append_event(
            project.id,
            project.project_revision,
            EventDraft::new("report.observed", "The original review receipt"),
        )
        .unwrap();
    let before = serde_json::to_value(&event).unwrap();
    let stores = [
        Store::open(&path).unwrap(),
        Store::open_existing(&path).unwrap(),
        created.memory_snapshot(4 * 1024 * 1024).unwrap(),
        Store::open_readonly(&path).unwrap(),
    ];
    for store in stores {
        for statement in [
            "UPDATE events SET summary='changed' WHERE id=?1",
            "UPDATE events SET payload_json='{}',created_at=0,importance='critical' WHERE id=?1",
            "DELETE FROM events WHERE id=?1",
            "INSERT INTO events(id,project_id,event_type,importance,summary,payload_json,project_revision,created_at) SELECT id,project_id,event_type,importance,'changed',payload_json,project_revision,created_at FROM events WHERE id=?1 ON CONFLICT(id) DO UPDATE SET summary='changed'",
            "INSERT OR REPLACE INTO events(id,project_id,event_type,importance,summary,payload_json,project_revision,created_at) SELECT id,project_id,event_type,importance,'changed',payload_json,project_revision,created_at FROM events WHERE id=?1",
        ] {
            assert!(
                store
                    .conn
                    .execute(statement, [event.id.to_string()])
                    .is_err(),
                "existing event was overwritten by: {statement}"
            );
            assert_eq!(
                serde_json::to_value(store.event(project.id, event.id).unwrap()).unwrap(),
                before
            );
            assert_eq!(
                store.project(project.id).unwrap().project_revision,
                event.project_revision
            );
        }
    }
    println!("AWR_ISOLATION_CASE event_row_immutability");
}
