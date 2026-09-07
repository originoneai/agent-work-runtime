//! Deterministic, source-refreshed context for resuming agent work.
pub use awr_core::{Error, Result};
mod bootstrap;
mod budget;
mod compile;
mod completeness;
mod delta;
mod hard;
mod related;
pub use bootstrap::{BootstrapContext, BootstrapPack, BootstrapRequest, bootstrap};
pub use budget::{
    BUDGET_POLICY, BudgetedContext, ContextChunk, ContextIdentity, ContextSection, RankedChunk,
    SelectedEntity, TOKEN_COUNT_SCOPE, TOKENIZER, budget_context, hard_chunks, token_count,
};
pub use compile::{ContextOmission, ContextRequest, WorkContextReport, compile_context};
pub use completeness::{
    CompletenessIssue, CompletenessRequest, ContextCompleteness, check_completeness,
    inspect_completeness,
};
pub use delta::{DeltaBaseline, DeltaRequest, RecentDelta, recent_delta};
pub use hard::{
    HardContext, HardWork, RuleScopeInput, RuleSelection, SourceVersion, hard_context, select_rules,
};
pub use related::{
    DecisionFact, DecisionGap, DependencyFact, EvidenceGap, EvidenceSummary, RelatedWorkContext,
    related_work,
};
