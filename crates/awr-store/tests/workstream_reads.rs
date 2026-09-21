use awr_core::*;
use awr_store::{CatalogKind, CatalogScope, EventQuery, SearchQuery, WorkstreamReadSelection};
#[allow(dead_code)]
#[path = "support/workstreams.rs"]
mod fixture;
use fixture::{Fixture, SNAPSHOT};

fn denied<T>(result: Result<T>) {
    assert!(matches!(
        result,
        Err(Error::Workstream(WorkstreamError::AccessDenied))
    ));
}

#[test]
fn selectors_never_grant_access_or_guess_between_multiple_scopes() {
    let mut f = Fixture::new();
    let a = f.start(0);
    let both = f.access(&[0, 1]);
    assert!(matches!(
        f.store
            .read_workstream(f.project, &both, &Default::default(), SNAPSHOT),
        Err(Error::Workstream(WorkstreamError::ScopeRequired))
    ));
    denied(f.store.read_workstream(
        f.project,
        &f.access(&[0]),
        &WorkstreamReadSelection {
            work_item_key: Some("W1".into()),
            ..Default::default()
        },
        SNAPSHOT,
    ));
    denied(f.store.read_workstream(
        f.project,
        &f.access(&[0]),
        &WorkstreamReadSelection {
            work_item_key: Some("absent".into()),
            ..Default::default()
        },
        SNAPSHOT,
    ));
    denied(f.store.read_workstream(
        f.project,
        &f.access(&[0]),
        &WorkstreamReadSelection {
            session_id: Some(Id::new()),
            ..Default::default()
        },
        SNAPSHOT,
    ));
    assert!(matches!(
        f.store.read_workstream(
            f.project,
            &both,
            &WorkstreamReadSelection {
                workstream_id: Some(f.scopes[1]),
                session_id: Some(a.id),
                ..Default::default()
            },
            SNAPSHOT
        ),
        Err(Error::Workstream(WorkstreamError::BindingMismatch))
    ));
    let binding = McpSessionBinding {
        client: "client".into(),
        conversation: "conversation".into(),
    };
    f.store
        .select_conversation_workstream(f.project, f.rev(), binding.clone(), f.scopes[1])
        .unwrap();
    let view = f
        .store
        .read_workstream(
            f.project,
            &both,
            &WorkstreamReadSelection {
                conversation: Some(binding.clone()),
                ..Default::default()
            },
            SNAPSHOT,
        )
        .unwrap();
    assert_eq!(view.workstream().id, f.scopes[1]);
    let view = f
        .store
        .read_workstream(
            f.project,
            &both,
            &WorkstreamReadSelection {
                conversation: Some(binding),
                session_id: Some(a.id),
                ..Default::default()
            },
            SNAPSHOT,
        )
        .unwrap();
    assert_eq!(view.workstream().id, f.scopes[0]);
    let mut stale = f.access(&[0]);
    stale.grants[0].authority_version = 2;
    assert!(matches!(
        f.store
            .read_workstream(f.project, &stale, &Default::default(), SNAPSHOT),
        Err(Error::Workstream(WorkstreamError::StaleAuthority))
    ));
    f.source = f
        .store
        .mark_source_freshness(&f.source, Freshness::Stale)
        .unwrap();
    assert!(matches!(
        f.store.read_workstream(
            f.project,
            &both,
            &WorkstreamReadSelection {
                workstream_id: Some(f.scopes[0]),
                ..Default::default()
            },
            SNAPSHOT
        ),
        Err(Error::SourceStale(_))
    ));
}

#[test]
fn catalog_counts_pages_and_cursors_are_scope_bound() {
    let f = Fixture::new();
    let a = f.read(0);
    let b = f.read(1);
    let (first, cursor) = a
        .catalog_page(CatalogKind::Work, CatalogScope::All, None, 1)
        .unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.items.len(), 1);
    assert!(first.has_more);
    assert!(first.next_cursor.is_none());
    assert!(cursor.is_some());
    let (second, end) = a
        .catalog_page(CatalogKind::Work, CatalogScope::All, cursor.as_ref(), 1)
        .unwrap();
    assert_eq!(second.total, 2);
    assert!(!second.has_more);
    assert!(end.is_none());
    assert_ne!(first.items[0].item["id"], second.items[0].item["id"]);
    denied(b.catalog_page(CatalogKind::Work, CatalogScope::All, cursor.as_ref(), 1));
    let mut forged = cursor.clone().unwrap();
    forged.cursor.after_id = f.works[1];
    denied(a.catalog_page(CatalogKind::Work, CatalogScope::All, Some(&forged), 1));
    let mut different_subject = f.access(&[0]);
    different_subject.subject = "another-reader".into();
    let other = f
        .store
        .read_workstream(f.project, &different_subject, &Default::default(), SNAPSHOT)
        .unwrap();
    denied(other.catalog_page(CatalogKind::Work, CatalogScope::All, cursor.as_ref(), 1));
    let goals = a
        .catalog_page(CatalogKind::Goal, CatalogScope::All, None, 100)
        .unwrap()
        .0;
    assert_eq!(goals.total, 1);
    assert_eq!(goals.items[0].item["external_key"], "G0");
    let plans = a
        .catalog_page(CatalogKind::Plan, CatalogScope::All, None, 100)
        .unwrap()
        .0;
    assert_eq!(plans.total, 1);
    assert_eq!(plans.items[0].item["external_key"], "P0");
    assert_eq!(
        a.catalog_page(CatalogKind::Rule, CatalogScope::All, None, 100)
            .unwrap()
            .0
            .total,
        1
    );
    assert!(matches!(
        a.catalog_page(CatalogKind::Source, CatalogScope::All, None, 100),
        Err(Error::Unsupported(_))
    ));
    denied(a.work_item("W1"));
    denied(a.work_item("missing"));
    let all = f
        .store
        .catalog_page(
            f.project,
            f.rev(),
            CatalogKind::Work,
            CatalogScope::All,
            None,
            100,
        )
        .unwrap();
    assert_eq!(all.total, 4);
}

#[test]
fn private_runtime_objects_and_event_cursors_cannot_cross_scopes() {
    let mut f = Fixture::new();
    let a = f.start(0);
    let b = f.start(1);
    let ca = f.checkpoint(a.id);
    let cb = f.checkpoint(b.id);
    let event = f.event(1, Some(b.id));
    let artifact = f
        .store
        .record_artifact(
            f.project,
            f.rev(),
            ArtifactDraft {
                artifact_type: "report".into(),
                locator: "private-report.txt".into(),
                sha256: "b".repeat(64),
                size: 10,
                mime: "text/plain".into(),
                source_event_id: event.id,
            },
        )
        .unwrap()
        .0;
    let evidence = f.evidence(1);
    let av = f.read(0);
    let bv = f.read(1);
    denied(av.session(b.id));
    denied(av.checkpoint(cb.id));
    denied(av.artifact(artifact.id));
    denied(av.evidence(&evidence.external_key));
    denied(av.event(event.id));
    denied(av.recovery_checkpoint(b.id));
    assert_eq!(av.recovery_checkpoint(a.id).unwrap().unwrap().id, ca.id);
    assert_eq!(bv.artifact(artifact.id).unwrap().id, artifact.id);
    assert_eq!(
        bv.evidence(&evidence.external_key).unwrap().item.id,
        evidence.id
    );
    let (page, cursor) = av
        .query_events(
            &EventQuery {
                limit: 1,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].session_id, Some(a.id));
    assert!(cursor.is_some());
    assert!(page.next_cursor.is_none());
    denied(bv.query_events(
        &EventQuery {
            limit: 1,
            ..Default::default()
        },
        cursor.as_ref(),
    ));
    denied(av.query_events(
        &EventQuery {
            work_item_id: Some(f.works[1]),
            ..Default::default()
        },
        None,
    ));
    let mut next = cursor;
    let mut count = page.events.len();
    while next.is_some() {
        let (page, cursor) = av
            .query_events(
                &EventQuery {
                    limit: 1,
                    ..Default::default()
                },
                next.as_ref(),
            )
            .unwrap();
        assert!(page.events.iter().all(|e| e.session_id == Some(a.id)));
        count += page.events.len();
        next = cursor;
    }
    assert_eq!(count, 2);
}

#[test]
fn search_corpus_and_recovery_limits_exclude_other_scopes_before_selection() {
    let mut f = Fixture::new();
    let a = f.start(0);
    let _b = f.start(1);
    let _b2 = f.start(3);
    f.event(0, Some(a.id));
    let query = SearchQuery {
        text: Some("Search".into()),
        limit: 100,
        ..Default::default()
    };
    let full_before = f.store.search(f.project, &query).unwrap();
    let before = f.read(0).search(&query).unwrap();
    assert!(before.hits.len() < full_before.hits.len());
    assert!(!before.truncated);
    assert!(
        before
            .hits
            .iter()
            .all(|h| h.work_item_key.as_deref() != Some("W1")
                && h.work_item_key.as_deref() != Some("W3"))
    );
    let full_after = f.store.search(f.project, &query).unwrap();
    assert_eq!(
        serde_json::to_value(&full_before).unwrap(),
        serde_json::to_value(full_after).unwrap()
    );
    for _ in 0..10 {
        f.event(1, None);
    }
    let after = f.read(0).search(&query).unwrap();
    assert_eq!(
        serde_json::to_value(before.hits).unwrap(),
        serde_json::to_value(after.hits).unwrap()
    );
    let candidates = f.read(0).resume_candidates(None, None).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, a.id);
    denied(f.read(0).resume_candidates(Some("W1"), None));
    let limited = f
        .read(0)
        .search(&SearchQuery { limit: 1, ..query })
        .unwrap();
    assert!(limited.truncated);
    assert_eq!(limited.hits.len(), 1);
    assert!(f.store.doctor().unwrap().ok);
}

#[test]
fn moved_work_does_not_relabel_historical_events_artifacts_or_evidence() {
    let mut f = Fixture::new();
    let session = f.start(0);
    let cp = f.checkpoint(session.id);
    let old = f.event(0, None);
    let evidence = f.evidence(0);
    let artifact = f
        .store
        .record_artifact(
            f.project,
            f.rev(),
            ArtifactDraft {
                artifact_type: "report".into(),
                locator: "old-report.txt".into(),
                sha256: "b".repeat(64),
                size: 10,
                mime: "text/plain".into(),
                source_event_id: old.id,
            },
        )
        .unwrap()
        .0;
    f.store
        .end_session(f.project, f.rev(), session.id, SessionOutcome::Ended)
        .unwrap();
    f.move_work(0, 1);
    let new = f.event(0, None);
    let (late_artifact, late_event) = f
        .store
        .record_artifact(
            f.project,
            f.rev(),
            ArtifactDraft {
                artifact_type: "report".into(),
                locator: "late-old-report.txt".into(),
                sha256: "b".repeat(64),
                size: 10,
                mime: "text/plain".into(),
                source_event_id: old.id,
            },
        )
        .unwrap();
    let a = f.read(0);
    let b = f.read(1);
    assert_eq!(a.event(old.id).unwrap().id, old.id);
    denied(b.event(old.id));
    assert_eq!(b.event(new.id).unwrap().id, new.id);
    denied(a.event(new.id));
    assert_eq!(a.checkpoint(cp.id).unwrap().id, cp.id);
    denied(b.checkpoint(cp.id));
    assert_eq!(a.artifact(artifact.id).unwrap().id, artifact.id);
    denied(b.artifact(artifact.id));
    assert_eq!(a.artifact(late_artifact.id).unwrap().id, late_artifact.id);
    assert_eq!(a.event(late_event.id).unwrap().id, late_event.id);
    denied(b.artifact(late_artifact.id));
    denied(b.event(late_event.id));
    assert_eq!(
        a.evidence(&evidence.external_key).unwrap().item.id,
        evidence.id
    );
    denied(b.evidence(&evidence.external_key));
    denied(a.work_item("W0"));
    assert_eq!(b.work_item("W0").unwrap().item.meta.id, f.works[0]);
    assert!(b.resume_candidates(Some("W0"), None).unwrap().is_empty());
    f.move_work(0, 0);
    let a = f.read(0);
    let b = f.read(1);
    assert_eq!(a.event(old.id).unwrap().id, old.id);
    assert_eq!(b.event(new.id).unwrap().id, new.id);
    denied(a.event(new.id));
    denied(b.event(old.id));
    assert_eq!(a.event(late_event.id).unwrap().id, late_event.id);
    denied(b.event(late_event.id));
    assert!(f.store.doctor().unwrap().ok);
}

#[test]
fn workless_session_cannot_lend_its_scope_to_another_works_event() {
    let mut f = Fixture::new();
    let session = f
        .store
        .start_session_in_workstream(
            f.project,
            f.rev(),
            SessionDraft {
                work_item_key: None,
                agent_id: "observer".into(),
                provider: "fixture".into(),
                model: "fixture".into(),
                branch_id: None,
                claim: false,
                claim_ttl_ms: None,
            },
            None,
            f.scopes[0],
        )
        .unwrap()
        .0
        .session;
    let mixed = f.event(1, Some(session.id));
    denied(f.read(0).event(mixed.id));
    denied(f.read(1).event(mixed.id));
}

#[test]
fn legacy_scoped_reads_keep_project_facts_and_do_not_mutate_the_origin() {
    let root = std::env::temp_dir().join(format!("awr-legacy-read-{}", Id::new()));
    std::fs::create_dir(&root).unwrap();
    let mut store = awr_store::Store::memory().unwrap();
    let project = store.register_project(&root, "legacy", "Legacy").unwrap();
    let scope = store.workstream_catalog(project.id).unwrap().workstreams[0].clone();
    let (session, _) = store
        .start_session(
            project.id,
            project.project_revision,
            SessionDraft {
                work_item_key: None,
                agent_id: "reader".into(),
                provider: "fixture".into(),
                model: "fixture".into(),
                branch_id: None,
                claim: false,
                claim_ttl_ms: None,
            },
        )
        .unwrap();
    let revision = store.project(project.id).unwrap().project_revision;
    let access = WorkstreamAccess {
        project_id: project.id.to_string(),
        subject: "owner".into(),
        grants: vec![WorkstreamGrant {
            workstream_id: scope.id,
            authority_version: scope.authority_version,
            read: true,
            write: true,
            manage: true,
        }],
    };
    let view = store
        .read_workstream(project.id, &access, &Default::default(), SNAPSHOT)
        .unwrap();
    assert_eq!(
        view.session(session.session.id).unwrap().id,
        session.session.id
    );
    assert_eq!(
        view.query_events(&Default::default(), None)
            .unwrap()
            .0
            .events
            .len(),
        store
            .query_events(project.id, &Default::default())
            .unwrap()
            .events
            .len()
    );
    assert_eq!(
        store.project(project.id).unwrap().project_revision,
        revision
    );
    assert!(store.doctor().unwrap().ok);
    std::fs::remove_dir(root).unwrap();
}
