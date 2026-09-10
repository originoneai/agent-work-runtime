mod support;
use awr_core::*;
use support::Fixture;

fn work(f: &Fixture, key: &str) -> WorkItem {
    WorkItem {
        ordinary_completion: None,
        archived: false,
        meta: f.meta(key),
        title: key.into(),
        kind: None,
        owner: None,
        required: true,
        raw_status: "planned".into(),
        status: WorkStatus::Planned,
        priority: None,
        milestone: None,
        score: None,
        evidence_level: Some(EvidenceLevel::Designed),
        summary: "".into(),
        next_action: "build".into(),
        blocker: None,
        acceptance: vec![],
        tags: vec![],
        paths: vec!["crates/core/lib.rs".into()],
    }
}
fn draft(key: &str) -> EvidenceDraft {
    EvidenceDraft {
        external_key: key.into(),
        work_item_key: Some("W".into()),
        evidence_type: "command_report".into(),
        level: EvidenceLevel::LocallyVerified,
        summary: "Feature check passed".into(),
        locator: "reports/check.json".into(),
        sha256: Some("a".repeat(64)),
        source_sha: Some("b".repeat(40)),
        command: Some("cargo test -p example".into()),
        scope: vec!["W".into()],
        branch_id: None,
        verified_at: Some(100),
    }
}

#[test]
fn decisions_require_acceptance_and_explicit_relevance_or_unknown_scope() {
    let mut f = Fixture::new();
    let mut batch = ProjectionBatch {
        work_items: vec![work(&f, "W")],
        ..Default::default()
    };
    for (key, status, keys, paths) in [
        ("accepted", DecisionStatus::Accepted, vec!["W"], vec![]),
        ("path", DecisionStatus::Accepted, vec![], vec!["crates/**"]),
        ("unscoped", DecisionStatus::Accepted, vec![], vec![]),
        ("unrelated", DecisionStatus::Accepted, vec!["OTHER"], vec![]),
        ("proposal", DecisionStatus::Proposed, vec!["W"], vec![]),
        ("superseded", DecisionStatus::Superseded, vec!["W"], vec![]),
    ] {
        batch.decisions.push(Decision {
            adoption: None,
            superseded_by: None,
            meta: f.meta(key),
            title: key.into(),
            status,
            raw_status: format!("{status:?}"),
            decision: "Choice".into(),
            rationale: "Reason".into(),
            affected_keys: keys.into_iter().map(String::from).collect(),
            paths: paths.into_iter().map(String::from).collect(),
        });
    }
    f.commit(batch);
    assert_eq!(f.store.decisions(f.project.id).unwrap().len(), 6);
    assert_eq!(
        f.store
            .decision(f.project.id, "proposal")
            .unwrap()
            .item
            .status,
        DecisionStatus::Proposed
    );
    let relevant = f.store.decisions_for_work(f.project.id, "W").unwrap();
    assert_eq!(relevant.len(), 3);
    assert_eq!(
        relevant
            .iter()
            .filter(|d| d.relevance == Applicability::Applicable)
            .count(),
        2
    );
    assert_eq!(
        relevant
            .iter()
            .find(|d| d.decision.item.meta.external_key == "unscoped")
            .unwrap()
            .relevance,
        Applicability::Unknown
    );
}

#[test]
fn unknown_work_paths_preserve_potential_decisions_and_explicit_scope_can_resolve_them() {
    let mut f = Fixture::new();
    let mut task = work(&f, "W");
    task.paths = vec!["crates/".into()];
    let decision = Decision {
        adoption: None,
        superseded_by: None,
        meta: f.meta("path-choice"),
        title: "Scoped choice".into(),
        status: DecisionStatus::Accepted,
        raw_status: "accepted".into(),
        decision: "Apply this source policy".into(),
        rationale: "Background".into(),
        affected_keys: vec![],
        paths: vec!["crates/core/**".into()],
    };
    f.commit(ProjectionBatch {
        work_items: vec![task],
        decisions: vec![decision],
        ..Default::default()
    });
    let unknown = f.store.decisions_for_work(f.project.id, "W").unwrap();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].relevance, Applicability::Unknown);
    let paths = vec!["crates/core/lib.rs".into()];
    let selected = f
        .store
        .decisions_for_work_with_paths(f.project.id, "W", Some(&paths))
        .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].relevance, Applicability::Applicable);
    assert!(
        f.store
            .decisions_for_work_with_paths(f.project.id, "W", Some(&[]))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn evidence_is_explicit_version_bound_and_append_only_by_key() {
    let mut f = Fixture::new();
    f.commit(ProjectionBatch {
        work_items: vec![work(&f, "W")],
        ..Default::default()
    });
    let rev = f.store.project(f.project.id).unwrap().project_revision;
    let event = f
        .store
        .append_event(
            f.project.id,
            rev,
            EventDraft::new("test_passed", "A test passed"),
        )
        .unwrap();
    assert!(
        f.store
            .evidence_for_work(f.project.id, "W", None, None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.store
            .work_item(f.project.id, "W")
            .unwrap()
            .item
            .evidence_level,
        Some(EvidenceLevel::Designed)
    );
    let mut missing = draft("missing");
    missing.command = None;
    assert!(matches!(
        f.store
            .record_evidence(f.project.id, event.project_revision, missing),
        Err(Error::EvidenceMissing(_))
    ));
    assert_eq!(
        f.store.project(f.project.id).unwrap().project_revision,
        event.project_revision
    );
    assert!(matches!(
        f.store.record_evidence(f.project.id, rev, draft("stale")),
        Err(Error::RevisionConflict { .. })
    ));
    let (record, recorded) = f
        .store
        .record_evidence(f.project.id, event.project_revision, draft("E-1"))
        .unwrap();
    assert_eq!(recorded.event_type, "evidence.recorded");
    assert_eq!(
        record.work_item_id,
        f.store
            .work_item(f.project.id, "W")
            .ok()
            .map(|w| w.item.meta.id)
    );
    let current = f
        .store
        .evidence_for_work(f.project.id, "W", Some(&"b".repeat(40)), None)
        .unwrap();
    assert_eq!(current[0].currency, EvidenceCurrency::Current);
    assert!(current[0].missing_bindings.is_empty());
    assert_eq!(
        current[0].evidence.item.level,
        EvidenceLevel::LocallyVerified
    );
    assert_eq!(
        f.store
            .evidence_for_work(f.project.id, "W", Some(&"c".repeat(40)), None)
            .unwrap()[0]
            .currency,
        EvidenceCurrency::Historical
    );
    assert_eq!(
        f.store
            .evidence_for_work(f.project.id, "W", None, None)
            .unwrap()[0]
            .currency,
        EvidenceCurrency::Unknown
    );
    assert_eq!(
        f.store
            .evidence_for_work(f.project.id, "W", Some(&"b".repeat(40)), Some(Id::new()))
            .unwrap()[0]
            .currency,
        EvidenceCurrency::Historical
    );
    assert!(matches!(
        f.store
            .record_evidence(f.project.id, recorded.project_revision, draft("E-1")),
        Err(Error::SourceConflict(_))
    ));
    assert_eq!(
        f.store.project(f.project.id).unwrap().project_revision,
        recorded.project_revision
    );
    let mut old = draft("E-2");
    old.source_sha = Some("d".repeat(40));
    f.store
        .record_evidence(f.project.id, recorded.project_revision, old)
        .unwrap();
    assert_eq!(f.store.evidence_records(f.project.id).unwrap().len(), 2);
    assert_eq!(
        f.store
            .evidence(f.project.id, "E-1")
            .unwrap()
            .item
            .source_sha,
        Some("b".repeat(40))
    );
    assert_eq!(
        f.store
            .work_item(f.project.id, "W")
            .unwrap()
            .item
            .evidence_level,
        Some(EvidenceLevel::Designed)
    );
    assert!(f.store.doctor().unwrap().ok);
}
