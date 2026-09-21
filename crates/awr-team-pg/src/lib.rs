//! PostgreSQL coordination store for Team V1.
//! Personal SQLite runtime does not depend on this crate.
mod bootstrap;
mod error;
mod execution;
mod graph;
mod import;
mod lease;
mod migrate;
mod path;
mod pool;
mod read;
mod review;
mod runner;
mod source;
mod tx;

pub use bootstrap::Bootstrap;
pub use error::{PgError, PgResult};
pub use execution::{ExecutionRecord, ExecutionStore, OutboxDelivery, exactly_once_supported};
pub use graph::{
    DependencyEdge, GraphStore, SplitProposal, paths_conflict, require_main_scope,
    validate_required_graph,
};
pub use import::{BackupRecord, FencingBarrier, ImportJob, ImportStore, InspectReport, RestoreRun};
pub use lease::{ClaimRecord, LeaseStore, SessionRecord};
pub use migrate::{EXPECTED_SCHEMA_VERSION, check_schema, migrate};
pub use path::{
    MAX_FILE_BYTES, MAX_PACKAGE_BYTES, MAX_SOURCE_FILES, validate_package, validate_source_path,
};
pub use pool::{PgClient, PgPool};
pub use read::{
    EventCursor, EventPage, EventRecord, PreparedWork, ReadStore, WorkGraph, capabilities,
    dispatch_query,
};
pub use review::{CompletionReceipt, EvidenceRecord, ReviewRound, ReviewStore};
#[doc(hidden)]
pub use runner::fence_key;
pub use runner::{CrashPoint, ReferenceRunner, RunnerOutcome};
pub use source::{CandidateRecord, CurrentSource, IngestRequest, SourceFile, SourceStore};
pub use tx::{CommandOutcome, CommandRequest, TeamStore};

pub const SCHEMA: &str = "awr_team";

/// Connect a single dedicated client (owner migration / bootstrap path).
/// Domain stores use pooled connections via [`PgPool`] instead (ADR-0004).
pub async fn connect(url: &str) -> PgResult<tokio_postgres::Client> {
    pool::connect(url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_contract_is_stable() {
        assert_eq!(SCHEMA, "awr_team");
        assert_eq!(EXPECTED_SCHEMA_VERSION, 9);
    }
}
