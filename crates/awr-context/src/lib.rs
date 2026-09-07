//! Deterministic, source-refreshed context for resuming agent work.
pub use awr_core::{Error, Result};
mod bootstrap;
mod hard;
mod related;
pub use bootstrap::{BootstrapContext, BootstrapPack, BootstrapRequest, bootstrap};
pub use hard::{
    HardContext, HardWork, RuleScopeInput, RuleSelection, SourceVersion, hard_context, select_rules,
};
pub use related::{
    DecisionFact, DecisionGap, DependencyFact, EvidenceGap, EvidenceSummary, RelatedWorkContext,
    related_work,
};
