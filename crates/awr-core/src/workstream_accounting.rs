//! Pure accounting over a caller-selected, authoritative contract snapshot.
//! The adapter must authenticate the project, load the approved immutable
//! contract and check evidence provenance, applicability and revocation before
//! calling this module. Structural consistency here is not evidence verification
//! or authorization. Nothing is loaded, persisted, or promoted by this module.
use crate::{Id, WorkstreamWorkBinding};
use std::collections::BTreeSet;
use thiserror::Error;

pub const WORKSTREAM_ACCOUNTING_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountingContractIdentity {
    pub project_id: String,
    pub workstream_id: Id,
    pub contract_id: String,
    pub revision: u64,
    pub digest: String,
}

/// Membership and ownership are frozen at this contract version. A later move
/// requires another approved snapshot; current ownership cannot rewrite history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkstreamAccountingContract {
    pub version: u32,
    pub identity: AccountingContractIdentity,
    pub required_work: Vec<WorkstreamWorkBinding>,
    /// Provider-owned references, never additional achievements of this stream.
    pub shared_references: Vec<WorkstreamWorkBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountingEvidence {
    pub reference: String,
    pub source_version: String,
    pub environment: String,
    pub acceptance_round: String,
    pub occurred_at_ms: i64,
    pub attribution: WorkstreamWorkBinding,
}

/// Recorded means the adapter supplied a record, not that this module checked
/// its contents. A source 'done' flag alone cannot create a Recorded stage.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AccountingStage {
    #[default]
    Unknown,
    NotMet,
    Recorded(AccountingEvidence),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountingStages {
    pub planned: AccountingStage,
    pub implemented: AccountingStage,
    pub verified: AccountingStage,
    pub merged: AccountingStage,
    pub released: AccountingStage,
}

impl AccountingStages {
    fn values(&self) -> [&AccountingStage; 5] {
        [
            &self.planned,
            &self.implemented,
            &self.verified,
            &self.merged,
            &self.released,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountingWork {
    pub binding: WorkstreamWorkBinding,
    pub contract: AccountingContractIdentity,
    /// Retained separately; never interpreted as any delivery stage.
    pub source_declared_done: bool,
    pub stages: AccountingStages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccountingStageCount {
    pub recorded: usize,
    pub not_met: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkstreamAccounting {
    pub contract: AccountingContractIdentity,
    pub required_count: usize,
    pub source_declared_done: usize,
    pub planned: AccountingStageCount,
    pub implemented: AccountingStageCount,
    pub verified: AccountingStageCount,
    pub merged: AccountingStageCount,
    pub released: AccountingStageCount,
    pub shared_reference_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AccountingError {
    #[error("unsupported accounting protocol version")]
    UnsupportedVersion,
    #[error("invalid accounting identity or evidence metadata")]
    InvalidDefinition,
    #[error("duplicate work identity")]
    DuplicateWork,
    #[error("work rows must cover exactly the required contract set")]
    WorkSetMismatch,
    #[error("project or frozen ownership binding does not match")]
    BindingMismatch,
    #[error("selected contract identity, revision or digest does not match")]
    ContractMismatch,
}

fn text(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

fn binding_valid(binding: &WorkstreamWorkBinding) -> bool {
    text(&binding.project_id)
        && text(&binding.work_item_id)
        && u128::from(binding.workstream_id) != 0
}

/// Counts only the frozen required set. Goal query results must first be
/// resolved by the adapter: extra, missing and duplicate rows are rejected.
/// Stages are independent, with no inferred promotion from a later stage.
/// An empty approved contract has denominator zero, never an invented percent.
pub fn account_workstream(
    contract: &WorkstreamAccountingContract,
    rows: &[AccountingWork],
) -> Result<WorkstreamAccounting, AccountingError> {
    if contract.version != WORKSTREAM_ACCOUNTING_VERSION {
        return Err(AccountingError::UnsupportedVersion);
    }
    let identity = &contract.identity;
    if !text(&identity.project_id)
        || !text(&identity.contract_id)
        || !text(&identity.digest)
        || identity.revision == 0
        || u128::from(identity.workstream_id) == 0
    {
        return Err(AccountingError::InvalidDefinition);
    }
    let mut declared = BTreeSet::new();
    for (bindings, owned) in [
        (&contract.required_work, true),
        (&contract.shared_references, false),
    ] {
        for binding in bindings {
            if !binding_valid(binding) {
                return Err(AccountingError::InvalidDefinition);
            }
            if binding.project_id != identity.project_id
                || (binding.workstream_id == identity.workstream_id) != owned
            {
                return Err(AccountingError::BindingMismatch);
            }
            if !declared.insert(&binding.work_item_id) {
                return Err(AccountingError::DuplicateWork);
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut counts = [AccountingStageCount::default(); 5];
    let mut source_done = 0;
    for row in rows {
        if !seen.insert(&row.binding.work_item_id) {
            return Err(AccountingError::DuplicateWork);
        }
        if row.contract != *identity {
            return Err(AccountingError::ContractMismatch);
        }
        let expected = contract
            .required_work
            .iter()
            .find(|binding| binding.work_item_id == row.binding.work_item_id)
            .ok_or(AccountingError::WorkSetMismatch)?;
        if row.binding != *expected {
            return Err(AccountingError::BindingMismatch);
        }
        source_done += usize::from(row.source_declared_done);
        for (stage, count) in row.stages.values().into_iter().zip(&mut counts) {
            match stage {
                AccountingStage::Unknown => count.unknown += 1,
                AccountingStage::NotMet => count.not_met += 1,
                AccountingStage::Recorded(evidence) => {
                    if evidence.attribution != row.binding {
                        return Err(AccountingError::BindingMismatch);
                    }
                    if !text(&evidence.reference)
                        || !text(&evidence.source_version)
                        || !text(&evidence.environment)
                        || !text(&evidence.acceptance_round)
                        || evidence.occurred_at_ms < 0
                    {
                        return Err(AccountingError::InvalidDefinition);
                    }
                    count.recorded += 1;
                }
            }
        }
    }
    if seen.len() != contract.required_work.len() {
        return Err(AccountingError::WorkSetMismatch);
    }
    let [planned, implemented, verified, merged, released] = counts;
    Ok(WorkstreamAccounting {
        contract: identity.clone(),
        required_count: contract.required_work.len(),
        source_declared_done: source_done,
        planned,
        implemented,
        verified,
        merged,
        released,
        shared_reference_count: contract.shared_references.len(),
    })
}
