//! Domain contracts shared by storage, source adapters, runtime and interfaces.
mod workstream_readset;
pub use workstream_readset::*;
mod branch;
mod branch_close;
mod completion;
mod error;
mod event_payload;
mod execution;
mod management;
mod model;
pub use management::*;
mod mutation;
mod query;
mod runtime;
mod secrets;
mod work_action;
mod workstream;
mod workstream_accounting;

pub use branch::*;
pub use branch_close::*;
pub use completion::*;
pub use error::{
    DiagnosticLocation, Error, ErrorReport, Result, SourceDiagnostic, render_diagnostic_details,
};
pub use event_payload::*;
pub use execution::*;
pub use model::*;
pub use mutation::*;
pub use query::*;
pub use runtime::*;
pub use secrets::*;
pub use work_action::*;
pub use workstream::*;
pub use workstream_accounting::*;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn now_millis() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    i64::try_from(elapsed.as_millis()).map_err(|_| Error::InvalidInput("timestamp overflow".into()))
}

mod client;
pub use client::ClientBinding;
mod compaction;
pub use compaction::*;
mod guidance;
pub use guidance::*;
mod ordinary;
pub use ordinary::*;
