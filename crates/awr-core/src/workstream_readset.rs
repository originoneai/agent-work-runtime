//! Pure, versioned operation preconditions. No authorization or persistence.
//!
//! Adapters authenticate/authorize before receipt lookup, construct the complete
//! action-specific read set from trusted records, and compare it in the same
//! transaction as effects and receipt insertion. Equality is not proof that a
//! claim is live, a dependency is accepted, or a resource is safe to reuse.
//! Those checks (including freeze, expiry and unknown execution) remain required.
//! Project audit cursors intentionally have no place in this business CAS.
use crate::{Revision, WorkstreamWorkBinding};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

pub const OPERATION_READSET_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationIdentity {
    pub work: WorkstreamWorkBinding,
    /// Authenticated principal identity; adapters must not trust the wire value.
    pub subject: String,
    /// Unique within the authenticated tenant and project, across all actions.
    pub request_id: String,
    pub action: String,
    /// SHA-256 of exact effect-bearing payload bytes, computed by the adapter.
    /// Canonicalization, if desired, must be explicitly versioned by the action.
    pub payload_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationTokenKind {
    Claim,
    Session,
    Fence,
    Dependency,
    Resource,
    Source,
    /// Required for graph mutations, not ordinary progress writes.
    Graph,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationToken {
    pub kind: OperationTokenKind,
    /// Trusted canonical identity, scoped to the operation's project. Resource
    /// keys include workspace/domain, canonical key and access mode; dependency
    /// keys identify the adopted binding, not an upstream 'latest' selector.
    pub key: String,
    /// Exact opaque version/digest. Adapters bind all relevant state: claim and
    /// session versions, fence, receipt validity/binding, reservation generation,
    /// source section/file fingerprint, or graph revision, respectively.
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationReadSet {
    pub protocol_version: u32,
    pub identity: OperationIdentity,
    pub coordinator_epoch: String,
    pub policy_revision: Revision,
    pub authority_version: Revision,
    pub work_version: Revision,
    pub contract_sha256: String,
    /// No defaults: a missing field is a protocol error. Empty is valid only
    /// when the trusted action planner also requires no additional tokens.
    pub tokens: Vec<OperationToken>,
}

/// Supplied by an authenticated adapter from a coherent snapshot, never decoded
/// from request arguments. Contains exactly the action's required dependencies;
/// a batch adapter must union participating objects or reject unsupported batches.
#[derive(Debug, Clone)]
pub struct RequiredOperationReadSet(pub OperationReadSet);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OperationReadSetError {
    #[error("unsupported operation read set protocol {0}")]
    UnsupportedVersion(u32),
    #[error("invalid operation read set: {0}")]
    Invalid(&'static str),
    #[error("operation identity mismatch")]
    IdentityMismatch,
    #[error("operation precondition changed: {0}")]
    Changed(&'static str),
    #[error("required operation token is missing")]
    MissingToken,
    #[error("unexpected operation token")]
    UnexpectedToken,
    #[error("operation token changed")]
    ChangedToken,
    #[error("request identity was reused with different intent")]
    IdempotencyConflict,
    #[error("receipt does not belong to this project/request")]
    ReceiptMismatch,
}

type ReadSetResult<T> = Result<T, OperationReadSetError>;

fn text_valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

fn sha256_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl OperationReadSet {
    pub fn validate(&self) -> ReadSetResult<()> {
        if self.protocol_version != OPERATION_READSET_VERSION {
            return Err(OperationReadSetError::UnsupportedVersion(
                self.protocol_version,
            ));
        }
        let identity = &self.identity;
        if !text_valid(&identity.work.project_id)
            || u128::from(identity.work.workstream_id) == 0
            || !text_valid(&identity.work.work_item_id)
            || !text_valid(&identity.subject)
            || !text_valid(&identity.request_id)
            || !text_valid(&identity.action)
            || !sha256_valid(&identity.payload_sha256)
            || !sha256_valid(&self.contract_sha256)
            || !text_valid(&self.coordinator_epoch)
            || self.authority_version == 0
        {
            return Err(OperationReadSetError::Invalid(
                "identity, hash or mandatory version",
            ));
        }
        let mut seen = BTreeSet::new();
        if self.tokens.len() > 4096
            || self.tokens.iter().any(|token| {
                !text_valid(&token.key)
                    || !text_valid(&token.version)
                    || !seen.insert((token.kind, &token.key))
            })
        {
            return Err(OperationReadSetError::Invalid("invalid or duplicate token"));
        }
        Ok(())
    }
}

/// Preconditions only. A successful comparison grants no capability, performs no
/// effects, and does not substitute for transaction isolation or action checks.
pub fn validate_operation_readset(
    supplied: &OperationReadSet,
    required: &RequiredOperationReadSet,
) -> ReadSetResult<()> {
    supplied.validate()?;
    let current = &required.0;
    current.validate()?;
    if supplied.identity != current.identity {
        return Err(OperationReadSetError::IdentityMismatch);
    }
    if supplied.coordinator_epoch != current.coordinator_epoch {
        return Err(OperationReadSetError::Changed("coordinator_epoch"));
    }
    for (name, expected, actual) in [
        (
            "policy_revision",
            supplied.policy_revision,
            current.policy_revision,
        ),
        (
            "authority_version",
            supplied.authority_version,
            current.authority_version,
        ),
        ("work_version", supplied.work_version, current.work_version),
    ] {
        if expected != actual {
            return Err(OperationReadSetError::Changed(name));
        }
    }
    if supplied.contract_sha256 != current.contract_sha256 {
        return Err(OperationReadSetError::Changed("contract_sha256"));
    }
    for token in &current.tokens {
        let found = supplied
            .tokens
            .iter()
            .find(|other| other.kind == token.kind && other.key == token.key)
            .ok_or(OperationReadSetError::MissingToken)?;
        if found.version != token.version {
            return Err(OperationReadSetError::ChangedToken);
        }
    }
    if supplied.tokens.len() != current.tokens.len() {
        return Err(OperationReadSetError::UnexpectedToken);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationReplay {
    NewRequest,
    /// Return the stored result/status, including pending/unknown; never execute
    /// again merely because a receipt has no final result yet.
    ExistingRequest,
}

/// Compare against the original immutable request stored with its receipt. The
/// adapter must scope lookup by authenticated tenant + project + request_id and
/// authorize receipt access first. Check this before current CAS: a prior effect
/// must not repeat just because its successful write advanced work_version.
/// Token order is immaterial; all token identities and values are exact intent.
pub fn classify_operation_replay(
    incoming: &OperationReadSet,
    recorded: Option<&OperationReadSet>,
) -> ReadSetResult<OperationReplay> {
    incoming.validate()?;
    let Some(recorded) = recorded else {
        return Ok(OperationReplay::NewRequest);
    };
    recorded.validate()?;
    if incoming.identity.work.project_id != recorded.identity.work.project_id
        || incoming.identity.request_id != recorded.identity.request_id
    {
        return Err(OperationReadSetError::ReceiptMismatch);
    }
    validate_operation_readset(incoming, &RequiredOperationReadSet(recorded.clone()))
        .map_err(|_| OperationReadSetError::IdempotencyConflict)?;
    Ok(OperationReplay::ExistingRequest)
}
