use serde::Serialize;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("source unavailable: {0}")]
    SourceUnavailable(String),
    #[error("source is stale: {0}")]
    SourceStale(String),
    #[error("source fingerprint conflict: {0}")]
    SourceConflict(String),
    #[error("revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("dependency blocked: {0}")]
    DependencyBlocked(String),
    #[error("claim conflict: {0}")]
    ClaimConflict(String),
    #[error("rule violation: {0}")]
    RuleViolation(String),
    #[error("required evidence missing: {0}")]
    EvidenceMissing(String),
    #[error("mutation unsupported: {0}")]
    MutationUnsupported(String),
    #[error("proposal {proposal_id} requires manual handling: {reason}")]
    ProposalRequired {
        proposal_id: crate::Id,
        reason: String,
    },
    #[error(
        "mutation application {attempt_event_id} for proposal {proposal_id} needs recovery: {reason}"
    )]
    MutationIncomplete {
        proposal_id: crate::Id,
        attempt_event_id: crate::Id,
        reason: String,
    },
    #[error("work action proposal {proposal_id} stopped during {stage}: {reason}")]
    WorkActionIncomplete {
        proposal_id: crate::Id,
        stage: String,
        reason: String,
    },
    #[error("mutation conflict: {0}")]
    MutationConflict(String),
    #[error(
        "checkpoint attempt {attempt_id} did not complete: {reason}; inspect session show before retrying"
    )]
    CheckpointIncomplete {
        attempt_id: crate::Id,
        reason: String,
    },
    #[error("context incomplete: {0}")]
    ContextIncomplete(String),
    #[error("context budget exceeded: required {required}, budget {budget}")]
    BudgetExceeded { required: usize, budget: usize },
    #[error("invalid transition: {0}")]
    InvalidTransition(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("operation is not implemented: {0}")]
    Unsupported(String),
    #[error("host protocol version {requested} is not supported")]
    ProtocolUnsupported { requested: u32, supported: Vec<u32> },
    #[error("required host capabilities are unavailable")]
    CapabilityUnavailable {
        unknown: Vec<String>,
        unsupported: Vec<String>,
    },
    #[error("storage error: {0}")]
    Storage(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
pub struct ErrorReport {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NotFound",
            Self::SourceUnavailable(_) => "SourceUnavailable",
            Self::SourceStale(_) => "SourceStale",
            Self::SourceConflict(_) => "SourceConflict",
            Self::RevisionConflict { .. } => "RevisionConflict",
            Self::DependencyBlocked(_) => "DependencyBlocked",
            Self::ClaimConflict(_) => "ClaimConflict",
            Self::RuleViolation(_) => "RuleViolation",
            Self::EvidenceMissing(_) => "EvidenceMissing",
            Self::MutationUnsupported(_) => "MutationUnsupported",
            Self::ProposalRequired { .. } => "proposal_required",
            Self::MutationIncomplete { .. } => "MutationIncomplete",
            Self::WorkActionIncomplete { .. } => "WorkActionIncomplete",
            Self::MutationConflict(_) => "MutationConflict",
            Self::CheckpointIncomplete { .. } => "CheckpointIncomplete",
            Self::ContextIncomplete(_) => "ContextIncomplete",
            Self::BudgetExceeded { .. } => "BudgetExceeded",
            Self::InvalidTransition(_) => "InvalidTransition",
            Self::InvalidInput(_) => "InvalidInput",
            Self::Unsupported(_) => "Unsupported",
            Self::ProtocolUnsupported { .. } => "ProtocolUnsupported",
            Self::CapabilityUnavailable { .. } => "CapabilityUnavailable",
            Self::Storage(_) => "Storage",
            Self::Io(_) => "Io",
            Self::Json(_) => "Json",
        }
    }
    pub fn report(&self) -> ErrorReport {
        ErrorReport {
            code: self.code(),
            message: crate::safe_diagnostic(&self.to_string()),
            details: (match self {
                Self::RuleViolation(message) => crate::secrets::sensitive_rejection_details(message),
                Self::RevisionConflict { expected, actual } => {
                    Some(serde_json::json!({"expected": expected, "actual": actual}))
                }
                Self::BudgetExceeded { required, budget } => {
                    Some(serde_json::json!({"required": required, "budget": budget}))
                }
                Self::ProtocolUnsupported { requested, supported } => Some(serde_json::json!({
                    "requested": requested, "supported": supported,
                    "source_write_performed": false, "runtime_write_performed": false
                })),
                Self::CapabilityUnavailable { unknown, unsupported } => Some(serde_json::json!({
                    "unknown": unknown, "unsupported": unsupported,
                    "source_write_performed": false, "runtime_write_performed": false
                })),
                Self::CheckpointIncomplete { attempt_id, reason } => {
                    Some(serde_json::json!({"attempt_id":attempt_id,"reason":reason}))
                }
                Self::ProposalRequired {
                    proposal_id,
                    reason,
                } => Some(
                    serde_json::json!({"proposal_id":proposal_id,"reason":reason,"source_write_performed":false}),
                ),
                Self::MutationIncomplete {
                    proposal_id,
                    attempt_event_id,
                    reason,
                } => Some(
                    serde_json::json!({"proposal_id":proposal_id,"attempt_event_id":attempt_event_id,"reason":reason}),
                ),
                Self::WorkActionIncomplete {
                    proposal_id,
                    stage,
                    reason,
                } => Some(
                    serde_json::json!({"proposal_id":proposal_id,"stage":stage,"reason":reason,"source_write_performed":null}),
                ),
                _ => None,
            }).map(crate::redact_sensitive_value),
        }
    }
}
