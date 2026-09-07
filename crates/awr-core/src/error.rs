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
    #[error("mutation conflict: {0}")]
    MutationConflict(String),
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
            Self::MutationConflict(_) => "MutationConflict",
            Self::ContextIncomplete(_) => "ContextIncomplete",
            Self::BudgetExceeded { .. } => "BudgetExceeded",
            Self::InvalidTransition(_) => "InvalidTransition",
            Self::InvalidInput(_) => "InvalidInput",
            Self::Unsupported(_) => "Unsupported",
            Self::Storage(_) => "Storage",
            Self::Io(_) => "Io",
            Self::Json(_) => "Json",
        }
    }
    pub fn report(&self) -> ErrorReport {
        ErrorReport {
            code: self.code(),
            message: self.to_string(),
            details: match self {
                Self::RevisionConflict { expected, actual } => {
                    Some(serde_json::json!({"expected": expected, "actual": actual}))
                }
                Self::BudgetExceeded { required, budget } => {
                    Some(serde_json::json!({"required": required, "budget": budget}))
                }
                _ => None,
            },
        }
    }
}
