use awr_core::*;

fn request() -> OperationReadSet {
    OperationReadSet {
        protocol_version: OPERATION_READSET_VERSION,
        identity: OperationIdentity {
            work: WorkstreamWorkBinding {
                project_id: "project".into(),
                workstream_id: Id::from(1u128),
                work_item_id: "work".into(),
            },
            subject: "agent".into(),
            request_id: "request".into(),
            action: "dispatch.v1".into(),
            payload_sha256: "a".repeat(64),
        },
        coordinator_epoch: "epoch-1".into(),
        policy_revision: 2,
        authority_version: 3,
        work_version: 4,
        contract_sha256: "b".repeat(64),
        tokens: [
            OperationTokenKind::Claim,
            OperationTokenKind::Session,
            OperationTokenKind::Fence,
            OperationTokenKind::Dependency,
            OperationTokenKind::Resource,
            OperationTokenKind::Source,
            OperationTokenKind::Graph,
        ]
        .into_iter()
        .map(|kind| OperationToken {
            kind,
            key: "bound-object".into(),
            version: "v1".into(),
        })
        .collect(),
    }
}

#[test]
fn every_required_token_must_be_present_and_unchanged() {
    let original = request();
    for i in 0..original.tokens.len() {
        let mut omitted = original.clone();
        omitted.tokens.remove(i);
        assert_eq!(
            validate_operation_readset(&omitted, &RequiredOperationReadSet(original.clone())),
            Err(OperationReadSetError::MissingToken)
        );
        let mut drifted = original.clone();
        drifted.tokens[i].version = "v2".into();
        assert_eq!(
            validate_operation_readset(&original, &RequiredOperationReadSet(drifted)),
            Err(OperationReadSetError::ChangedToken)
        );
    }
}

#[test]
fn mandatory_versions_and_contract_are_cas_conditions() {
    let original = request();
    for field in [
        "coordinator_epoch",
        "policy_revision",
        "authority_version",
        "work_version",
        "contract_sha256",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value[field] = if field == "coordinator_epoch" {
            serde_json::json!("epoch-2")
        } else if field == "contract_sha256" {
            serde_json::json!("c".repeat(64))
        } else {
            serde_json::json!(99)
        };
        let current = serde_json::from_value(value).unwrap();
        assert_eq!(
            validate_operation_readset(&original, &RequiredOperationReadSet(current)),
            Err(OperationReadSetError::Changed(field))
        );
    }
}

#[test]
fn identity_and_payload_cannot_be_substituted() {
    let original = request();
    let changes: Vec<Box<dyn Fn(&mut OperationReadSet)>> = vec![
        Box::new(|r| r.identity.work.project_id = "other".into()),
        Box::new(|r| r.identity.work.workstream_id = Id::from(2u128)),
        Box::new(|r| r.identity.work.work_item_id = "other".into()),
        Box::new(|r| r.identity.subject = "other".into()),
        Box::new(|r| r.identity.request_id = "other".into()),
        Box::new(|r| r.identity.action = "other".into()),
        Box::new(|r| r.identity.payload_sha256 = "c".repeat(64)),
    ];
    for change in changes {
        let mut modified = original.clone();
        change(&mut modified);
        assert_eq!(
            validate_operation_readset(&modified, &RequiredOperationReadSet(original.clone())),
            Err(OperationReadSetError::IdentityMismatch)
        );
        assert!(classify_operation_replay(&modified, Some(&original)).is_err());
    }
}

#[test]
fn adopted_dependency_token_changes_are_rejected() {
    let original = request();
    // This tests supplied binding tokens, not planner or transaction isolation.
    assert_eq!(
        validate_operation_readset(&original, &RequiredOperationReadSet(original.clone())),
        Ok(())
    );
    let mut revoked = original.clone();
    revoked
        .tokens
        .iter_mut()
        .find(|t| t.kind == OperationTokenKind::Dependency)
        .unwrap()
        .version = "revoked-v2".into();
    assert_eq!(
        validate_operation_readset(&original, &RequiredOperationReadSet(revoked)),
        Err(OperationReadSetError::ChangedToken)
    );
}

#[test]
fn exact_replay_precedes_current_cas_and_does_not_repeat_unknown_effects() {
    let original = request();
    assert_eq!(
        classify_operation_replay(&original, None),
        Ok(OperationReplay::NewRequest)
    );
    let mut current = original.clone();
    current.work_version += 1;
    assert!(
        validate_operation_readset(&original, &RequiredOperationReadSet(current.clone())).is_err()
    );
    assert_eq!(
        classify_operation_replay(&original, Some(&original)),
        Ok(OperationReplay::ExistingRequest)
    );
    assert_eq!(
        classify_operation_replay(&current, Some(&original)),
        Err(OperationReadSetError::IdempotencyConflict)
    );
    let mut reordered = original.clone();
    reordered.tokens.reverse();
    assert_eq!(
        classify_operation_replay(&reordered, Some(&original)),
        Ok(OperationReplay::ExistingRequest)
    );
}

#[test]
fn fail_closed_on_missing_unknown_duplicate_or_extra_input() {
    let original = request();
    let mut value = serde_json::to_value(&original).unwrap();
    value.as_object_mut().unwrap().remove("tokens");
    assert!(serde_json::from_value::<OperationReadSet>(value).is_err());
    let mut value = serde_json::to_value(&original).unwrap();
    value["project_revision"] = serde_json::json!(100);
    assert!(serde_json::from_value::<OperationReadSet>(value).is_err());
    let mut invalid = original.clone();
    invalid.protocol_version = 2;
    assert_eq!(
        invalid.validate(),
        Err(OperationReadSetError::UnsupportedVersion(2))
    );
    let mut invalid = original.clone();
    invalid.tokens.push(invalid.tokens[0].clone());
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.authority_version = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = original.clone();
    invalid.identity.payload_sha256.clear();
    assert!(invalid.validate().is_err());
    let mut extra = original.clone();
    extra.tokens.push(OperationToken {
        kind: OperationTokenKind::Source,
        key: "unrelated".into(),
        version: "v1".into(),
    });
    assert_eq!(
        validate_operation_readset(&extra, &RequiredOperationReadSet(original)),
        Err(OperationReadSetError::UnexpectedToken)
    );
}

#[test]
fn empty_tokens_only_match_an_action_with_no_additional_requirements() {
    let mut simple = request();
    simple.tokens.clear();
    assert_eq!(
        validate_operation_readset(&simple, &RequiredOperationReadSet(simple.clone())),
        Ok(())
    );
    assert_eq!(
        validate_operation_readset(&simple, &RequiredOperationReadSet(request())),
        Err(OperationReadSetError::MissingToken)
    );
}

#[test]
fn opaque_epoch_and_initial_work_revision_are_valid() {
    let mut initial = request();
    initial.coordinator_epoch = "01ARZ3NDEKTSV4RRFFQ69G5FAV".into();
    initial.work_version = 0;
    initial.policy_revision = 0;
    assert_eq!(
        validate_operation_readset(&initial, &RequiredOperationReadSet(initial.clone())),
        Ok(())
    );
    initial.coordinator_epoch.clear();
    assert!(initial.validate().is_err());
}
