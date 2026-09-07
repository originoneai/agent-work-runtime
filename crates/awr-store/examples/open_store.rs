//! Developer entry point before project initialization is available in the CLI.
use awr_store::Store;
fn main() -> awr_core::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| awr_core::Error::InvalidInput("usage: open_store <database-path>".into()))?;
    let store = Store::open(&path)?;
    println!("{}", serde_json::to_string_pretty(&store.doctor()?)?);
    Ok(())
}
