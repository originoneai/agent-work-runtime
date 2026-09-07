//! Authoritative project source configuration and adapter contracts.
mod adapter;
mod freshness;
mod locator;
mod manifest;
mod markdown;
mod yaml_ledger;

pub use adapter::{ParseContext, ProjectionBatch, SourceAdapter};
pub use awr_core::{Error, Result};
pub use freshness::{SourceObservation, observe_source};
pub use locator::{Locator, SourceSnapshot, fingerprint, read_capped};
pub use manifest::{Manifest, ProjectConfig, SourceSpec};
pub use markdown::{
    MarkdownHeadingAdapter, MarkdownRulesAdapter, MarkdownSection, markdown_sections,
};
pub use yaml_ledger::YamlLedgerAdapter;
