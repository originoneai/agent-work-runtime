use awr_core::*;

fn fixture() -> (WorkstreamAccountingContract, Vec<AccountingWork>) {
    let identity = AccountingContractIdentity {
        project_id: "project".into(),
        workstream_id: Id::from(1),
        contract_id: "contract".into(),
        revision: 1,
        digest: "digest-v1".into(),
    };
    let required_work: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|id| WorkstreamWorkBinding {
            project_id: identity.project_id.clone(),
            workstream_id: identity.workstream_id,
            work_item_id: id.into(),
        })
        .collect();
    let rows = required_work
        .iter()
        .map(|binding| AccountingWork {
            binding: binding.clone(),
            contract: identity.clone(),
            source_declared_done: false,
            stages: AccountingStages::default(),
        })
        .collect();
    (
        WorkstreamAccountingContract {
            version: 1,
            identity,
            required_work,
            shared_references: vec![],
        },
        rows,
    )
}

fn record(binding: &WorkstreamWorkBinding) -> AccountingStage {
    AccountingStage::Recorded(AccountingEvidence {
        reference: "receipt".into(),
        source_version: "source-v1".into(),
        environment: "candidate".into(),
        acceptance_round: "round-1".into(),
        occurred_at_ms: 10,
        attribution: binding.clone(),
    })
}

#[test]
fn counts_independent_stages_without_promoting_source_done() {
    let (contract, mut rows) = fixture();
    rows[0].source_declared_done = true;
    rows[0].stages.planned = record(&rows[0].binding);
    rows[0].stages.implemented = record(&rows[0].binding);
    rows[1].stages.released = record(&rows[1].binding);
    rows[1].stages.verified = AccountingStage::NotMet;
    let result = account_workstream(&contract, &rows).unwrap();
    assert_eq!(result.required_count, 2);
    assert_eq!(result.source_declared_done, 1);
    assert_eq!(result.planned.recorded, 1);
    assert_eq!(result.implemented.recorded, 1);
    assert_eq!(
        result.verified,
        AccountingStageCount {
            recorded: 0,
            not_met: 1,
            unknown: 1
        }
    );
    assert_eq!(result.merged.unknown, 2);
    assert_eq!(result.released.recorded, 1);
}

#[test]
fn mixed_goal_query_cannot_expand_frozen_denominator() {
    let (contract, mut rows) = fixture();
    let original = account_workstream(&contract, &rows).unwrap();
    let mut extra = rows[0].clone();
    extra.binding.work_item_id = "new-planning-item-with-same-goal".into();
    rows.push(extra);
    assert_eq!(
        account_workstream(&contract, &rows),
        Err(AccountingError::WorkSetMismatch)
    );
    assert_eq!(original.required_count, 2);
    assert_eq!(contract.required_work.len(), 2);
}

#[test]
fn duplicate_or_missing_members_and_rows_are_rejected() {
    let (mut contract, rows) = fixture();
    assert_eq!(
        account_workstream(&contract, &rows[..1]),
        Err(AccountingError::WorkSetMismatch)
    );
    assert_eq!(
        account_workstream(&contract, &[rows[0].clone(), rows[0].clone()]),
        Err(AccountingError::DuplicateWork)
    );
    contract
        .required_work
        .push(contract.required_work[0].clone());
    assert_eq!(
        account_workstream(&contract, &rows),
        Err(AccountingError::DuplicateWork)
    );
}

#[test]
fn contract_version_drift_is_not_current_delivery() {
    let (contract, rows) = fixture();
    for field in 0..4 {
        let mut changed = contract.clone();
        match field {
            0 => changed.identity.revision += 1,
            1 => changed.identity.digest = "changed".into(),
            2 => changed.identity.contract_id = "different".into(),
            _ => changed.version = 2,
        }
        assert!(account_workstream(&changed, &rows).is_err());
    }
    assert!(account_workstream(&contract, &rows).is_ok());
}

#[test]
fn cross_project_and_ownership_bindings_are_rejected() {
    let (contract, rows) = fixture();
    let mut changed = rows.clone();
    changed[0].binding.project_id = "other-project".into();
    assert_eq!(
        account_workstream(&contract, &changed),
        Err(AccountingError::BindingMismatch)
    );
    let mut changed = contract.clone();
    changed.required_work[0].workstream_id = Id::from(2);
    assert_eq!(
        account_workstream(&changed, &rows),
        Err(AccountingError::BindingMismatch)
    );
    let mut changed = rows.clone();
    changed[0].stages.verified = record(&WorkstreamWorkBinding {
        project_id: "other-project".into(),
        ..rows[0].binding.clone()
    });
    assert_eq!(
        account_workstream(&contract, &changed),
        Err(AccountingError::BindingMismatch)
    );
}

#[test]
fn shared_provider_references_are_not_owned_achievements() {
    let (mut contract, rows) = fixture();
    let shared = WorkstreamWorkBinding {
        project_id: "project".into(),
        workstream_id: Id::from(2),
        work_item_id: "provider-work".into(),
    };
    contract.shared_references.push(shared.clone());
    let result = account_workstream(&contract, &rows).unwrap();
    assert_eq!(result.required_count, 2);
    assert_eq!(result.shared_reference_count, 1);
    assert_eq!(result.verified.unknown, 2);
    contract.shared_references.push(shared);
    assert_eq!(
        account_workstream(&contract, &rows),
        Err(AccountingError::DuplicateWork)
    );
}

#[test]
fn historical_attribution_survives_current_work_move() {
    let (old_contract, mut old_rows) = fixture();
    old_rows[0].stages.verified = record(&old_rows[0].binding);
    let historical = account_workstream(&old_contract, &old_rows).unwrap();
    let mut new_contract = old_contract.clone();
    new_contract.identity.revision = 2;
    new_contract.identity.digest = "new-scope".into();
    new_contract.required_work.remove(0);
    assert_eq!(
        account_workstream(&new_contract, &old_rows),
        Err(AccountingError::ContractMismatch)
    );
    assert_eq!(
        account_workstream(&old_contract, &old_rows).unwrap(),
        historical
    );
    assert_eq!(historical.contract.workstream_id, Id::from(1));
    assert_eq!(historical.verified.recorded, 1);
}

#[test]
fn missing_evidence_metadata_is_not_a_recorded_stage() {
    let (contract, mut rows) = fixture();
    let mut evidence = match record(&rows[0].binding) {
        AccountingStage::Recorded(evidence) => evidence,
        _ => unreachable!(),
    };
    evidence.source_version.clear();
    rows[0].stages.verified = AccountingStage::Recorded(evidence);
    assert_eq!(
        account_workstream(&contract, &rows),
        Err(AccountingError::InvalidDefinition)
    );
}

#[test]
fn empty_contract_has_zero_denominator_without_completion_claim() {
    let (mut contract, _) = fixture();
    contract.required_work.clear();
    let result = account_workstream(&contract, &[]).unwrap();
    assert_eq!(result.required_count, 0);
    assert_eq!(result.verified, AccountingStageCount::default());
}
