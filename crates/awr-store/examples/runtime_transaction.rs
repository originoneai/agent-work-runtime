use awr_core::{Error, EventDraft, Id};
use awr_store::Store;
fn main() -> awr_core::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            Error::InvalidInput("usage: runtime_transaction <existing fixture directory>".into())
        })?;
    let mut store = Store::open(&root.join("state.db"))?;
    let project = store.register_project(&root, "runtime-example", "Runtime Example")?;
    let revision = project.project_revision;
    let event = store.append_event(
        project.id,
        revision,
        EventDraft::new("work_started", "Begin implementing the requested feature"),
    )?;
    assert_eq!(store.project(project.id)?.project_revision, revision + 1);
    let stale = store
        .append_event(
            project.id,
            revision,
            EventDraft::new("checkpoint_created", "This stale write must fail"),
        )
        .unwrap_err();
    assert!(
        matches!(stale,Error::RevisionConflict{expected,actual} if expected==revision&&actual==revision+1)
    );
    let mut invalid = EventDraft::new("work_started", "This cross-reference must roll back");
    invalid.branch_id = Some(Id::new());
    let failed = store
        .append_event(project.id, revision + 1, invalid)
        .unwrap_err();
    assert_eq!(failed.code(), "Storage");
    assert_eq!(store.project(project.id)?.project_revision, revision + 1);
    let events = store.events_since(project.id, revision, 10)?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, event.id);
    println!(
        "{}",
        serde_json::json!({"committed_revision":event.project_revision,"event_id":event.id,
        "events_since_start":events.len(),"stale_error":stale.report(),"rollback_error":failed.report(),
        "revision_after_failed_write":store.project(project.id)?.project_revision})
    );
    Ok(())
}
