use awr_core::{EvidenceLevel, Id, WorkstreamWorkBinding, workstream_adoption::*};

fn fixture() -> (DeliveryRequirement, DeliveryFacts) {
    let provider = WorkstreamWorkBinding {
        project_id: "project".into(),
        work_item_id: "upstream".into(),
        workstream_id: Id::from(1),
    };
    let consumer = WorkstreamWorkBinding {
        project_id: "project".into(),
        work_item_id: "downstream".into(),
        workstream_id: Id::from(2),
    };
    let selected = DeliveryVersion {
        completion_receipt: Id::from(3),
        contract_sha256: "a".repeat(64),
        artifact_sha256: "b".repeat(64),
        source_sha: "c".repeat(40),
        environment: "candidate-v1".into(),
        acceptance_round: "round-1".into(),
        export_scope_sha256: "d".repeat(64),
    };
    let requirement = DeliveryRequirement {
        provider: provider.clone(),
        consumer: consumer.clone(),
        selected: selected.clone(),
        policy: DeliveryVersionPolicy::FixedDelivery,
        minimum_level: EvidenceLevel::LocallyVerified,
    };
    let facts = DeliveryFacts {
        provider,
        consumer,
        delivery: Some(selected.clone()),
        current_selection: Some(selected),
        acceptance: DeliveryAcceptance::Verified {
            evidence_id: Id::from(4),
            author: "author".into(),
            reviewer: "independent-reviewer".into(),
            level: EvidenceLevel::LocallyVerified,
            verified_at_ms: 90,
        },
        availability: DeliveryAvailability::Available,
        export_authority: DeliveryExportAuthority::Granted,
        observed_at_ms: 100,
    };
    (requirement, facts)
}
fn expect(
    required: &DeliveryRequirement,
    facts: &DeliveryFacts,
    status: DeliveryStatus,
    reason: DeliveryReason,
) {
    assert_eq!(
        assess_delivery(required, facts).unwrap(),
        DeliveryAssessment { status, reason }
    );
}

#[test]
fn fixed_delivery_survives_new_planning_and_retains_original_proof() {
    let (required, mut facts) = fixture();
    let adopted = adopt_delivery(required, facts.clone(), 100).unwrap();
    let original = adopted.clone();
    facts.observed_at_ms = 200;
    facts.current_selection.as_mut().unwrap().contract_sha256 = "e".repeat(64);
    facts.current_selection.as_mut().unwrap().completion_receipt = Id::from(8);
    assert_eq!(
        adopted.reassess(&facts).unwrap().status,
        DeliveryStatus::Satisfied
    );
    facts.current_selection = None;
    assert_eq!(
        adopted.reassess(&facts).unwrap().status,
        DeliveryStatus::Satisfied
    );
    facts.availability = DeliveryAvailability::Unavailable;
    assert_eq!(
        adopted.reassess(&facts).unwrap().reason,
        DeliveryReason::ArtifactUnavailable
    );
    facts.acceptance = DeliveryAcceptance::Revoked;
    assert_eq!(
        adopted.reassess(&facts).unwrap().status,
        DeliveryStatus::Revoked
    );
    assert_eq!(adopted, original);
    assert_eq!(adopted.adopted_at_ms(), 100);
    assert_eq!(adopted.original_proof().observed_at_ms, 100);
    assert_eq!(
        adopted.requirement().selected.completion_receipt,
        Id::from(3)
    );
}

#[test]
fn current_contract_needs_explicit_current_selection() {
    let (mut required, mut facts) = fixture();
    required.policy = DeliveryVersionPolicy::CurrentContract;
    expect(
        &required,
        &facts,
        DeliveryStatus::Satisfied,
        DeliveryReason::VerifiedDelivery,
    );
    facts.current_selection = None;
    expect(
        &required,
        &facts,
        DeliveryStatus::Unknown,
        DeliveryReason::CurrentSelectionUnknown,
    );
    facts.current_selection = facts.delivery.clone();
    facts.current_selection.as_mut().unwrap().completion_receipt = Id::from(9);
    expect(
        &required,
        &facts,
        DeliveryStatus::Stale,
        DeliveryReason::CurrentSelectionChanged,
    );
    facts.export_authority = DeliveryExportAuthority::Revoked;
    expect(
        &required,
        &facts,
        DeliveryStatus::Revoked,
        DeliveryReason::ExportRevoked,
    );
}

#[test]
fn every_version_dimension_is_exact() {
    let (required, facts) = fixture();
    let mutations: Vec<Box<dyn Fn(&mut DeliveryVersion)>> = vec![
        Box::new(|v| v.completion_receipt = Id::from(99)),
        Box::new(|v| v.contract_sha256 = "f".repeat(64)),
        Box::new(|v| v.artifact_sha256 = "f".repeat(64)),
        Box::new(|v| v.source_sha = "f".repeat(40)),
        Box::new(|v| v.environment = "other-env".into()),
        Box::new(|v| v.acceptance_round = "round-2".into()),
        Box::new(|v| v.export_scope_sha256 = "f".repeat(64)),
    ];
    for mutate in mutations {
        let mut changed = facts.clone();
        mutate(changed.delivery.as_mut().unwrap());
        expect(
            &required,
            &changed,
            DeliveryStatus::Stale,
            DeliveryReason::DeliveryChanged,
        );
    }
}

#[test]
fn source_done_unknown_untrusted_rejected_and_self_review_never_satisfy() {
    let (required, facts) = fixture();
    use DeliveryAcceptance::*;
    let cases = [
        (
            Unknown,
            DeliveryStatus::Unknown,
            DeliveryReason::AcceptanceUnknown,
        ),
        (
            Untrusted,
            DeliveryStatus::Unknown,
            DeliveryReason::UntrustedAcceptance,
        ),
        (
            AuthorDeclaredDone,
            DeliveryStatus::Waiting,
            DeliveryReason::IndependentAcceptanceRequired,
        ),
        (
            Rejected,
            DeliveryStatus::Waiting,
            DeliveryReason::AcceptanceRejected,
        ),
        (
            Revoked,
            DeliveryStatus::Revoked,
            DeliveryReason::AcceptanceRevoked,
        ),
        (
            Verified {
                evidence_id: Id::from(5),
                author: "same".into(),
                reviewer: "same".into(),
                level: EvidenceLevel::Released,
                verified_at_ms: 90,
            },
            DeliveryStatus::Waiting,
            DeliveryReason::IndependentAcceptanceRequired,
        ),
        (
            Verified {
                evidence_id: Id::from(5),
                author: "a".into(),
                reviewer: "b".into(),
                level: EvidenceLevel::Unknown,
                verified_at_ms: 90,
            },
            DeliveryStatus::Unknown,
            DeliveryReason::AcceptanceUnknown,
        ),
        (
            Verified {
                evidence_id: Id::from(5),
                author: "a".into(),
                reviewer: "b".into(),
                level: EvidenceLevel::Implemented,
                verified_at_ms: 90,
            },
            DeliveryStatus::Waiting,
            DeliveryReason::EvidenceLevelInsufficient,
        ),
    ];
    for (acceptance, status, reason) in cases {
        let mut changed = facts.clone();
        changed.acceptance = acceptance;
        expect(&required, &changed, status, reason);
        assert!(matches!(
            adopt_delivery(required.clone(), changed, 100),
            Err(DeliveryError::NotSatisfied(_))
        ));
    }
}

#[test]
fn missing_unknown_unavailable_and_denied_are_distinct() {
    let (required, facts) = fixture();
    let mut changed = facts.clone();
    changed.delivery = None;
    expect(
        &required,
        &changed,
        DeliveryStatus::Waiting,
        DeliveryReason::MissingDelivery,
    );
    for (availability, status, reason) in [
        (
            DeliveryAvailability::Unknown,
            DeliveryStatus::Unknown,
            DeliveryReason::ArtifactAvailabilityUnknown,
        ),
        (
            DeliveryAvailability::Unavailable,
            DeliveryStatus::Stale,
            DeliveryReason::ArtifactUnavailable,
        ),
    ] {
        let mut changed = facts.clone();
        changed.availability = availability;
        expect(&required, &changed, status, reason);
    }
    for (authority, status, reason) in [
        (
            DeliveryExportAuthority::Unknown,
            DeliveryStatus::Unknown,
            DeliveryReason::ExportAuthorityUnknown,
        ),
        (
            DeliveryExportAuthority::Denied,
            DeliveryStatus::Waiting,
            DeliveryReason::ExportDenied,
        ),
        (
            DeliveryExportAuthority::Revoked,
            DeliveryStatus::Revoked,
            DeliveryReason::ExportRevoked,
        ),
    ] {
        let mut changed = facts.clone();
        changed.export_authority = authority;
        expect(&required, &changed, status, reason);
    }
}

#[test]
fn rejects_cross_project_or_forged_ownership_and_self_dependency() {
    let (required, facts) = fixture();
    let mut other = facts.clone();
    other.provider.workstream_id = Id::from(9);
    assert_eq!(
        assess_delivery(&required, &other),
        Err(DeliveryError::BindingMismatch)
    );
    let mut other = facts.clone();
    other.consumer.work_item_id = "other".into();
    assert_eq!(
        assess_delivery(&required, &other),
        Err(DeliveryError::BindingMismatch)
    );
    let mut request = required.clone();
    request.consumer.project_id = "other".into();
    let mut other = facts.clone();
    other.consumer = request.consumer.clone();
    assert_eq!(
        assess_delivery(&request, &other),
        Err(DeliveryError::BindingMismatch)
    );
    let mut request = required;
    request.consumer = request.provider.clone();
    let mut other = facts;
    other.consumer = request.consumer.clone();
    assert_eq!(
        assess_delivery(&request, &other),
        Err(DeliveryError::BindingMismatch)
    );
}

#[test]
fn metadata_and_full_hashes_are_bounded_and_validated() {
    let (required, facts) = fixture();
    for bad in [
        "a".repeat(63),
        "G".repeat(64),
        "a".repeat(65),
        " ".repeat(64),
    ] {
        let mut request = required.clone();
        request.selected.contract_sha256 = bad;
        assert_eq!(
            assess_delivery(&request, &facts),
            Err(DeliveryError::InvalidDefinition)
        );
    }
    for bad in ["abc1234".into(), "z".repeat(40), "a".repeat(41)] {
        let mut request = required.clone();
        request.selected.source_sha = bad;
        assert_eq!(
            assess_delivery(&request, &facts),
            Err(DeliveryError::InvalidDefinition)
        );
    }
    for bad in [" ".into(), "a\n".into(), "a".repeat(4097)] {
        let mut request = required.clone();
        request.selected.environment = bad;
        assert_eq!(
            assess_delivery(&request, &facts),
            Err(DeliveryError::InvalidDefinition)
        );
    }
    let mut request = required.clone();
    request.minimum_level = EvidenceLevel::Unknown;
    assert_eq!(
        assess_delivery(&request, &facts),
        Err(DeliveryError::InvalidDefinition)
    );
    let mut request = required;
    request.provider.workstream_id = Id::from(0);
    assert_eq!(
        assess_delivery(&request, &facts),
        Err(DeliveryError::InvalidDefinition)
    );
}

#[test]
fn snapshot_times_cannot_precede_evidence_or_adoption() {
    let (required, facts) = fixture();
    assert_eq!(
        adopt_delivery(required.clone(), facts.clone(), 101),
        Err(DeliveryError::InvalidTime)
    );
    let adopted = adopt_delivery(required.clone(), facts.clone(), 100).unwrap();
    let mut older = facts.clone();
    older.observed_at_ms = 99;
    assert_eq!(adopted.reassess(&older), Err(DeliveryError::InvalidTime));
    let mut future_evidence = facts;
    if let DeliveryAcceptance::Verified { verified_at_ms, .. } = &mut future_evidence.acceptance {
        *verified_at_ms = 101;
    }
    assert_eq!(
        assess_delivery(&required, &future_evidence),
        Err(DeliveryError::InvalidTime)
    );
}
