//! Authoritative project source configuration and adapter contracts.
mod adapter;
mod directory;
mod document;
mod freshness;
mod indexer;
mod ledger_mapping;
mod limits;
mod locator;
mod manifest;
mod markdown;
mod markdown_ledger;
mod mutation;
mod yaml_create;
mod yaml_edit;
mod yaml_ledger;
mod yaml_mutation;

pub use adapter::{ParseContext, ProjectionBatch, SourceAdapter};
pub use awr_core::{Error, Result};
pub use directory::{DirectoryDelta, DirectoryInventory, MarkdownDirectoryAdapter};
pub use document::{
    DocumentAction, DocumentEdit, PreparedDocument, document_path_registration,
    prepare_document_draft, prepare_document_edit,
};
pub use freshness::{SourceObservation, observe_source};
pub use indexer::{
    IndexIssue, IndexReport, IndexedSource, index_project, scan_project, source_adapter,
    source_configuration,
};
pub use ledger_mapping::LedgerMapping;
pub use limits::{MARKDOWN_READ_CAP, YAML_READ_CAP, source_read_cap};
pub use locator::{Locator, SourceSnapshot, fingerprint, read_capped, read_source_capped};
mod safe_fs;
pub use manifest::{
    ContextProfile, Manifest, ProjectConfig, SOURCE_ADAPTERS, SourceSpec, minimal_context,
};
pub use markdown::{
    MarkdownHeadingAdapter, MarkdownRulesAdapter, MarkdownSection, markdown_sections,
};
pub use markdown_ledger::MarkdownLedgerAdapter;
pub use mutation::{
    MutationSourceCheck, inspect_mutation_source, inspect_registered_source, verify_mutation_source,
};
pub use safe_fs::{open_dir_exact, open_file_exact};
pub use yaml_create::{PreparedWorkCreation, prepare_work_creation};
pub use yaml_ledger::YamlLedgerAdapter;
pub use yaml_mutation::{
    PreparedYamlMutation, parse_mutation_projection, prepare_yaml_mutation,
    read_yaml_mutation_record, yaml_field_writable,
};
