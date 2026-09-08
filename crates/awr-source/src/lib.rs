//! Authoritative project source configuration and adapter contracts.
mod adapter;
mod directory;
mod freshness;
mod indexer;
mod locator;
mod manifest;
mod markdown;
mod mutation;
mod yaml_ledger;

pub use adapter::{ParseContext, ProjectionBatch, SourceAdapter};
pub use awr_core::{Error, Result};
pub use directory::{DirectoryDelta, DirectoryInventory, MarkdownDirectoryAdapter};
pub use freshness::{SourceObservation, observe_source};
pub use indexer::{
    IndexIssue, IndexReport, IndexedSource, index_project, scan_project, source_adapter,
};
pub use locator::{Locator, SourceSnapshot, fingerprint, read_capped};
pub use manifest::{Manifest, ProjectConfig, SourceSpec};
pub use markdown::{
    MarkdownHeadingAdapter, MarkdownRulesAdapter, MarkdownSection, markdown_sections,
};
pub use mutation::{MutationSourceCheck, verify_mutation_source};
pub use yaml_ledger::YamlLedgerAdapter;
