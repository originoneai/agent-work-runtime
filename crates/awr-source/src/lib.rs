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
mod yaml_mutation;

pub use adapter::{ParseContext, ProjectionBatch, SourceAdapter};
pub use awr_core::{Error, Result};
pub use directory::{DirectoryDelta, DirectoryInventory, MarkdownDirectoryAdapter};
pub use freshness::{SourceObservation, observe_source};
pub use indexer::{
    IndexIssue, IndexReport, IndexedSource, index_project, scan_project, source_adapter,
};
pub use locator::{Locator, SourceSnapshot, fingerprint, read_capped, read_source_capped};
mod safe_fs;
pub use manifest::{Manifest, ProjectConfig, SourceSpec};
pub use markdown::{
    MarkdownHeadingAdapter, MarkdownRulesAdapter, MarkdownSection, markdown_sections,
};
pub use mutation::{MutationSourceCheck, inspect_mutation_source, verify_mutation_source};
pub use safe_fs::{open_dir_exact, open_file_exact};
pub use yaml_ledger::YamlLedgerAdapter;
pub use yaml_mutation::{
    PreparedYamlMutation, parse_mutation_projection, prepare_yaml_mutation,
    read_yaml_mutation_record,
};
