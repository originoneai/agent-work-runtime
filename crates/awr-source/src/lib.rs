//! Authoritative project source configuration and adapter contracts.
mod adapter;
mod locator;
mod manifest;

pub use adapter::{ParseContext, ProjectionBatch, SourceAdapter};
pub use awr_core::{Error, Result};
pub use locator::{Locator, SourceSnapshot, fingerprint, read_capped};
pub use manifest::{Manifest, ProjectConfig, SourceSpec};
