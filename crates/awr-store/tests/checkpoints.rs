#[allow(dead_code)] // Shared fixture helpers also support projection tests.
mod support;
use awr_core::*;
use support::Fixture;

fn draft(next: &str) -> CheckpointDraft {
    CheckpointDraft {
        context_hash: "a".repeat(64),
        digest: "Implemented persistent work state".into(),
        next_action: next.into(),
        open_loops: vec!["wire CLI".into()],
        changed_entities: vec!["runtime/session".into()],
    }
}
#[test]
fn checkpoint_and_latest_pointer_are_atomic_and_survive_reopen() {
    let mut f = Fixture::new();
    let revision = f.store.project(f.project.id).unwrap().project_revision;
    let (session, event) = f
        .store
        .start_session(
            f.project.id,
            revision,
            SessionDraft {
                work_item_key: None,
                agent_id: "executor".into(),
                provider: "fixture".into(),
                model: "test".into(),
                branch_id: None,
                claim: false,
                claim_ttl_ms: None,
            },
        )
        .unwrap();
    let (first, event) = f
        .store
        .create_checkpoint(
            f.project.id,
            event.project_revision,
            session.session.id,
            draft("Add history command"),
        )
        .unwrap();
    assert_eq!(first.project_revision, event.project_revision - 1);
    assert_eq!(
        f.store
            .session(f.project.id, session.session.id)
            .unwrap()
            .last_checkpoint_id,
        Some(first.id)
    );
    assert!(matches!(
        f.store.create_checkpoint(
            f.project.id,
            first.project_revision,
            session.session.id,
            draft("stale")
        ),
        Err(Error::RevisionConflict { .. })
    ));
    let (second, event) = f
        .store
        .create_checkpoint(
            f.project.id,
            event.project_revision,
            session.session.id,
            draft("Add artifact command"),
        )
        .unwrap();
    let reopened = awr_store::Store::open_readonly(&f.root.join("state.db")).unwrap();
    let latest = reopened
        .latest_checkpoint(f.project.id, session.session.id)
        .unwrap()
        .unwrap();
    assert_eq!(latest.id, second.id);
    assert_eq!(latest.next_action, "Add artifact command");
    assert_eq!(latest.open_loops, ["wire CLI"]);
    assert_eq!(latest.changed_entities, ["runtime/session"]);
    assert_eq!(
        reopened
            .checkpoint(f.project.id, first.id)
            .unwrap()
            .next_action,
        "Add history command"
    );
    assert!(matches!(
        reopened.checkpoint(Id::new(), first.id),
        Err(Error::NotFound(_))
    ));
    let (_, ended) = f
        .store
        .end_session(
            f.project.id,
            event.project_revision,
            session.session.id,
            SessionOutcome::Ended,
        )
        .unwrap();
    assert!(matches!(
        f.store.create_checkpoint(
            f.project.id,
            ended.project_revision,
            session.session.id,
            draft("late")
        ),
        Err(Error::InvalidTransition(_))
    ));
    assert!(f.store.doctor().unwrap().ok);
}

#[test]
fn artifact_metadata_is_bound_to_its_origin_event_and_revision() {
    let mut f = Fixture::new();
    let revision = f.store.project(f.project.id).unwrap().project_revision;
    let event = f
        .store
        .append_event(
            f.project.id,
            revision,
            EventDraft::new("report.produced", "Report generated"),
        )
        .unwrap();
    let draft = ArtifactDraft {
        artifact_type: "report".into(),
        locator: ".awr/artifacts/report".into(),
        sha256: "b".repeat(64),
        size: 1024,
        mime: "application/json".into(),
        source_event_id: event.id,
    };
    let (artifact, recorded) = f
        .store
        .record_artifact(f.project.id, event.project_revision, draft.clone())
        .unwrap();
    let reloaded = f.store.artifact(f.project.id, artifact.id).unwrap();
    assert_eq!(reloaded.sha256, "b".repeat(64));
    assert_eq!(reloaded.source_event_id, Some(event.id));
    assert_eq!(reloaded.size, 1024);
    assert_eq!(reloaded.mime, "application/json");
    let mut invalid = draft;
    invalid.source_event_id = Id::new();
    assert!(matches!(
        f.store
            .record_artifact(f.project.id, recorded.project_revision, invalid),
        Err(Error::NotFound(_))
    ));
    assert_eq!(
        f.store.project(f.project.id).unwrap().project_revision,
        recorded.project_revision
    );
}
