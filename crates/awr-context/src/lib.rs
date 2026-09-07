//! Deterministic, source-refreshed context for resuming agent work.
pub use awr_core::{Error, Result};
mod bootstrap;
pub use bootstrap::{BootstrapContext, BootstrapPack, BootstrapRequest, bootstrap};
