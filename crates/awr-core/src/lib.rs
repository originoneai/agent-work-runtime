//! Domain contracts shared by storage, source adapters, runtime and interfaces.
mod error;
mod model;
mod query;
mod runtime;

pub use error::{Error, ErrorReport, Result};
pub use model::*;
pub use query::*;
pub use runtime::*;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn now_millis() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    i64::try_from(elapsed.as_millis()).map_err(|_| Error::InvalidInput("timestamp overflow".into()))
}
