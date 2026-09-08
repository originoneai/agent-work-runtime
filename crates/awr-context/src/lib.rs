//! Deterministic, source-refreshed context for resuming agent work.
pub use awr_core::{Error, Result};
mod bootstrap;
mod branch;
mod budget;
mod compile;
mod completeness;
mod delta;
mod hard;
mod related;
pub use bootstrap::{BootstrapContext, BootstrapPack, BootstrapRequest, bootstrap};
pub use branch::BranchContextBinding;
pub use budget::{
    BUDGET_POLICY, BudgetedContext, ContextChunk, ContextIdentity, ContextSection, RankedChunk,
    SelectedEntity, TOKEN_COUNT_SCOPE, TOKENIZER, budget_context, hard_chunks, token_count,
};
pub use compile::{
    ContextOmission, ContextRequest, WorkContextReport, compile_branch_context, compile_context,
};
pub use completeness::{
    CompletenessIssue, CompletenessRequest, ContextCompleteness, check_completeness,
    inspect_completeness,
};
pub use delta::{
    DeltaBaseline, DeltaContextReport, DeltaContextRequest, DeltaRequest, RecentDelta,
    branch_delta, context_delta, recent_delta,
};
pub use hard::{
    HardContext, HardWork, RuleScopeInput, RuleSelection, SourceVersion, hard_context, select_rules,
};
pub use related::{
    DecisionFact, DecisionGap, DependencyFact, EvidenceGap, EvidenceSummary, RelatedWorkContext,
    related_work,
};

/// Withholding selected facts must never produce a falsely complete context packet.
fn public_context<T: serde::Serialize>(result: Result<T>) -> Result<T> {
    let report = result?;
    awr_core::ensure_public_data(&report).map_err(|_| Error::ContextIncomplete(
        "selected context contains sensitive content; the packet is withheld until its source is corrected".into()
    ))?;
    Ok(report)
}
