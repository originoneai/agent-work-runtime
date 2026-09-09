use awr_core::{Error, Result};

pub const MARKDOWN_READ_CAP: u64 = 2 * 1024 * 1024;
pub const YAML_READ_CAP: u64 = 4 * 1024 * 1024;

/// Inclusive byte limits for the supported source adapters, independent of transport.
pub fn source_read_cap(adapter: &str) -> Result<u64> {
    match adapter {
        "yaml-ledger-v1" => Ok(YAML_READ_CAP),
        "markdown-ledger-v1"
        | "markdown-heading-v1"
        | "markdown-rules-v1"
        | "markdown-directory-v1" => Ok(MARKDOWN_READ_CAP),
        _ => Err(Error::Unsupported(format!("source adapter {adapter}"))),
    }
}

pub(crate) fn check_source_size(bytes: &[u8], cap: u64) -> Result<()> {
    if bytes.len() as u64 > cap {
        return Err(Error::InvalidInput(format!(
            "source exceeds {cap} byte read cap"
        )));
    }
    awr_core::ensure_public_bytes(bytes)
}
