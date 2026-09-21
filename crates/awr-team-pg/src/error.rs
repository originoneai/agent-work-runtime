use thiserror::Error;

pub type PgResult<T> = Result<T, PgError>;

#[derive(Debug, Error)]
pub enum PgError {
    #[error(transparent)]
    Workstream(#[from] awr_core::WorkstreamError),
    #[error("schema incompatible: {0}")]
    SchemaIncompatible(String),
    #[error("idempotency conflict")]
    IdempotencyConflict,
    #[error("project not available")]
    ProjectNotAvailable,
    #[error("unsafe source path: {0}")]
    UnsafeSourcePath(String),
    #[error("file is not valid UTF-8: {0}")]
    InvalidUtf8(String),
    #[error("snapshot content drifted from its recorded digest: {0}")]
    SnapshotDrift(String),
    #[error("stale or unbound approval")]
    StaleApproval,
    #[error("authority epoch mismatch")]
    EpochMismatch,
    #[error("parser version mismatch")]
    ParserMismatch,
    #[error("candidate is not approved")]
    CandidateNotApproved,
    #[error("author cannot approve their own candidate")]
    AuthorCannotApprove,
    #[error("candidate is not the active source")]
    InactiveCandidate,
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("event cursor expired")]
    CursorExpired,
    #[error("coordinator epoch changed")]
    EpochChanged,
    #[error("session not found")]
    SessionNotFound,
    #[error("claim held")]
    ClaimHeld,
    #[error("lease expired")]
    LeaseExpired,
    #[error("recovery blocked")]
    RecoveryBlocked,
    #[error("open wait blocks progress")]
    WaitOpen,
    #[error("forbidden")]
    Forbidden,
    #[error("stale fence")]
    StaleFence,
    #[error("dependency cycle")]
    DependencyCycle,
    #[error("missing required dependency")]
    MissingDependency,
    #[error("resource conflict")]
    ResourceConflict,
    #[error("scope unsupported")]
    ScopeUnsupported,
    #[error("parent evidence required")]
    ParentEvidenceRequired,
    #[error("dependency binding invalid")]
    BindingInvalid,
    #[error("claimed work blocks activation")]
    ClaimBlocksActivation,
    #[error("graph budget exceeded")]
    GraphBudgetExceeded,
    #[error("execution not found")]
    ExecutionNotFound,
    #[error("scope exceeded")]
    ScopeExceeded,
    #[error("exactly-once unsupported")]
    ExactlyOnceUnsupported,
    #[error("evidence invalid")]
    EvidenceInvalid,
    #[error("review required")]
    ReviewRequired,
    #[error("author cannot review")]
    AuthorCannotReview,
    #[error("completion rejected")]
    CompletionRejected,
    #[error("policy downgrade")]
    PolicyDowngrade,
    #[error("context incomplete")]
    ContextIncomplete,
    #[error("serialized response exceeds service limit")]
    ResponseTooLarge,
    #[error("source divergence")]
    SourceDivergence,
    #[error("restore incomplete")]
    RestoreIncomplete,
    #[error("outbox replay forbidden")]
    OutboxReplayForbidden,
    #[error("rollback forbidden")]
    RollbackForbidden,
    #[error("{0}")]
    Db(#[from] tokio_postgres::Error),
    #[error("connection pool: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),
    #[error("{0}")]
    Protocol(String),
}
