//! Pure delivery adoption rules; no storage, authorization, dispatch or IO.
//!
//! Adapters must authenticate the caller, resolve canonical work ownership and
//! verify receipt/report bytes, reviewer independence and export authority before
//! constructing `DeliveryFacts`. These deliberately non-Serde facts are NOT wire
//! authorization claims. Public constructors permit synthetic/domain callers;
//! structural checks here cannot establish provenance. A runtime must recheck a
//! coherent action-specific snapshot atomically with adoption/claim/dispatch/
//! completion, and retain historical bindings and recovery responsibility when
//! validity changes during execution. This module implements none of that IO.
use crate::{EvidenceLevel, Id, WorkstreamWorkBinding, is_source_sha, verification_rank};
use thiserror::Error;

/// Exact immutable delivery identity, never a mutable path or a "latest" alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryVersion {
    pub completion_receipt: Id,
    pub contract_sha256: String,
    pub artifact_sha256: String,
    pub source_sha: String,
    pub environment: String,
    pub acceptance_round: String,
    /// Hash of the exact approved disclosure scope, not an arbitrary path.
    pub export_scope_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryVersionPolicy {
    FixedDelivery,
    CurrentContract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryRequirement {
    pub provider: WorkstreamWorkBinding,
    pub consumer: WorkstreamWorkBinding,
    pub selected: DeliveryVersion,
    pub policy: DeliveryVersionPolicy,
    pub minimum_level: EvidenceLevel,
}

/// Provenance is an adapter verdict, never inferred from a source status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryAcceptance {
    Unknown,
    Untrusted,
    AuthorDeclaredDone,
    Rejected,
    Revoked,
    Verified {
        evidence_id: Id,
        author: String,
        reviewer: String,
        level: EvidenceLevel,
        verified_at_ms: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryAvailability {
    Unknown,
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryExportAuthority {
    Unknown,
    Granted,
    Denied,
    Revoked,
}

/// Separate trusted input. Every verdict concerns exactly `delivery` and the
/// two frozen work identities, including its export scope and acceptance round.
/// `current_selection` is the authoritative current contract AND selected receipt;
/// omission is unknown, not permission to resolve an implicit latest version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryFacts {
    pub provider: WorkstreamWorkBinding,
    pub consumer: WorkstreamWorkBinding,
    pub delivery: Option<DeliveryVersion>,
    pub current_selection: Option<DeliveryVersion>,
    pub acceptance: DeliveryAcceptance,
    pub availability: DeliveryAvailability,
    pub export_authority: DeliveryExportAuthority,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    Satisfied,
    Waiting,
    Stale,
    Revoked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryReason {
    VerifiedDelivery,
    MissingDelivery,
    DeliveryChanged,
    CurrentSelectionUnknown,
    CurrentSelectionChanged,
    AcceptanceUnknown,
    UntrustedAcceptance,
    IndependentAcceptanceRequired,
    AcceptanceRejected,
    AcceptanceRevoked,
    EvidenceLevelInsufficient,
    ArtifactAvailabilityUnknown,
    ArtifactUnavailable,
    ExportAuthorityUnknown,
    ExportDenied,
    ExportRevoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryAssessment {
    pub status: DeliveryStatus,
    pub reason: DeliveryReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeliveryError {
    #[error("invalid or unbounded delivery metadata")]
    InvalidDefinition,
    #[error("delivery ownership must bind distinct works in one project")]
    BindingMismatch,
    #[error("delivery observation or verification time is invalid")]
    InvalidTime,
    #[error("delivery is not adoptable: {0:?}")]
    NotSatisfied(DeliveryAssessment),
}

/// Created only after evaluation. Getters are immutable; re-evaluation never
/// rewrites the selected version, original proof, or adoption timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryAdoption {
    requirement: DeliveryRequirement,
    original_proof: DeliveryFacts,
    adopted_at_ms: i64,
}

fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn binding(value: &WorkstreamWorkBinding) -> bool {
    text(&value.project_id) && text(&value.work_item_id) && u128::from(value.workstream_id) != 0
}
fn version(value: &DeliveryVersion) -> bool {
    u128::from(value.completion_receipt) != 0
        && sha256(&value.contract_sha256)
        && sha256(&value.artifact_sha256)
        && sha256(&value.export_scope_sha256)
        && is_source_sha(&value.source_sha)
        && text(&value.environment)
        && text(&value.acceptance_round)
}
fn assessment(status: DeliveryStatus, reason: DeliveryReason) -> DeliveryAssessment {
    DeliveryAssessment { status, reason }
}

/// Deterministic precedence: invalid input, absent/changed receipt, explicit
/// revocation, current-selection drift, acceptance, availability, then authority.
/// A satisfied result concerns this snapshot only and grants no execution right.
pub fn assess_delivery(
    required: &DeliveryRequirement,
    facts: &DeliveryFacts,
) -> Result<DeliveryAssessment, DeliveryError> {
    if !binding(&required.provider)
        || !binding(&required.consumer)
        || !version(&required.selected)
        || verification_rank(required.minimum_level).is_none_or(|rank| rank < 2)
        || facts.delivery.as_ref().is_some_and(|v| !version(v))
        || facts
            .current_selection
            .as_ref()
            .is_some_and(|v| !version(v))
    {
        return Err(DeliveryError::InvalidDefinition);
    }
    if required.provider.project_id != required.consumer.project_id
        || required.provider.work_item_id == required.consumer.work_item_id
        || required.provider != facts.provider
        || required.consumer != facts.consumer
    {
        return Err(DeliveryError::BindingMismatch);
    }
    if facts.observed_at_ms < 0 {
        return Err(DeliveryError::InvalidTime);
    }
    if let DeliveryAcceptance::Verified {
        evidence_id,
        author,
        reviewer,
        verified_at_ms,
        ..
    } = &facts.acceptance
    {
        if u128::from(*evidence_id) == 0 || !text(author) || !text(reviewer) {
            return Err(DeliveryError::InvalidDefinition);
        }
        if *verified_at_ms < 0 || *verified_at_ms > facts.observed_at_ms {
            return Err(DeliveryError::InvalidTime);
        }
    }
    use DeliveryReason::*;
    use DeliveryStatus::*;
    let Some(delivery) = &facts.delivery else {
        return Ok(assessment(Waiting, MissingDelivery));
    };
    if delivery != &required.selected {
        return Ok(assessment(Stale, DeliveryChanged));
    }
    if facts.acceptance == DeliveryAcceptance::Revoked {
        return Ok(assessment(Revoked, AcceptanceRevoked));
    }
    if facts.export_authority == DeliveryExportAuthority::Revoked {
        return Ok(assessment(Revoked, ExportRevoked));
    }
    if required.policy == DeliveryVersionPolicy::CurrentContract {
        match &facts.current_selection {
            None => return Ok(assessment(Unknown, CurrentSelectionUnknown)),
            Some(current) if current != delivery => {
                return Ok(assessment(Stale, CurrentSelectionChanged));
            }
            _ => (),
        }
    }
    match &facts.acceptance {
        DeliveryAcceptance::Unknown => return Ok(assessment(Unknown, AcceptanceUnknown)),
        DeliveryAcceptance::Untrusted => return Ok(assessment(Unknown, UntrustedAcceptance)),
        DeliveryAcceptance::AuthorDeclaredDone => {
            return Ok(assessment(Waiting, IndependentAcceptanceRequired));
        }
        DeliveryAcceptance::Rejected => return Ok(assessment(Waiting, AcceptanceRejected)),
        DeliveryAcceptance::Verified {
            author,
            reviewer,
            level,
            ..
        } => {
            if author == reviewer {
                return Ok(assessment(Waiting, IndependentAcceptanceRequired));
            }
            let Some(rank) = verification_rank(*level) else {
                return Ok(assessment(Unknown, AcceptanceUnknown));
            };
            if rank < verification_rank(required.minimum_level).unwrap() {
                return Ok(assessment(Waiting, EvidenceLevelInsufficient));
            }
        }
        DeliveryAcceptance::Revoked => unreachable!("handled above"),
    }
    match facts.availability {
        DeliveryAvailability::Unknown => {
            return Ok(assessment(Unknown, ArtifactAvailabilityUnknown));
        }
        DeliveryAvailability::Unavailable => return Ok(assessment(Stale, ArtifactUnavailable)),
        DeliveryAvailability::Available => (),
    }
    Ok(match facts.export_authority {
        DeliveryExportAuthority::Unknown => assessment(Unknown, ExportAuthorityUnknown),
        DeliveryExportAuthority::Denied => assessment(Waiting, ExportDenied),
        DeliveryExportAuthority::Granted => assessment(Satisfied, VerifiedDelivery),
        DeliveryExportAuthority::Revoked => unreachable!("handled above"),
    })
}

pub fn adopt_delivery(
    requirement: DeliveryRequirement,
    facts: DeliveryFacts,
    adopted_at_ms: i64,
) -> Result<DeliveryAdoption, DeliveryError> {
    // The adapter's coherent snapshot must be from the adoption action itself.
    if adopted_at_ms < 0 || adopted_at_ms != facts.observed_at_ms {
        return Err(DeliveryError::InvalidTime);
    }
    let result = assess_delivery(&requirement, &facts)?;
    if result.status != DeliveryStatus::Satisfied {
        return Err(DeliveryError::NotSatisfied(result));
    }
    Ok(DeliveryAdoption {
        requirement,
        original_proof: facts,
        adopted_at_ms,
    })
}

impl DeliveryAdoption {
    pub fn requirement(&self) -> &DeliveryRequirement {
        &self.requirement
    }
    pub fn original_proof(&self) -> &DeliveryFacts {
        &self.original_proof
    }
    pub fn adopted_at_ms(&self) -> i64 {
        self.adopted_at_ms
    }

    /// Recheck for each action from fresh trusted facts, without losing history.
    /// Non-satisfied results require the adapter to prevent new effects/final
    /// acceptance and preserve any prior effects for recovery, not erase them.
    pub fn reassess(&self, facts: &DeliveryFacts) -> Result<DeliveryAssessment, DeliveryError> {
        if facts.observed_at_ms < self.adopted_at_ms {
            return Err(DeliveryError::InvalidTime);
        }
        assess_delivery(&self.requirement, facts)
    }
}
