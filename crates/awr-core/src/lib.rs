//! Domain contracts shared by storage, source adapters, runtime and interfaces.
mod branch;
mod branch_close;
mod completion;
mod error;
mod event_payload;
mod model;
mod mutation;
mod query;
mod runtime;
mod work_action;

pub use branch::*;
pub use branch_close::*;
pub use completion::*;
pub use error::{Error, ErrorReport, Result};
pub use event_payload::*;
pub use model::*;
pub use mutation::*;
pub use query::*;
pub use runtime::*;
pub use work_action::*;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn now_millis() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    i64::try_from(elapsed.as_millis()).map_err(|_| Error::InvalidInput("timestamp overflow".into()))
}
