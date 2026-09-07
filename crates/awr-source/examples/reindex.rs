use awr_core::{Error, Result};
use awr_source::{Manifest, index_project};
use awr_store::Store;
use std::{env, fs, path::PathBuf};

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) || (args.len() == 4 && args[3] != "--force") {
        return Err(Error::InvalidInput(
            "reindex <root> <manifest-path> <database> [--force]".into(),
        ));
    }
    let root = PathBuf::from(&args[0]).canonicalize()?;
    let manifest = Manifest::parse(&fs::read_to_string(&args[1])?)?;
    let mut store = Store::open(&PathBuf::from(&args[2]))?;
    let report = index_project(&mut store, &root, &manifest, args.len() == 4)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.ok {
        return Err(Error::SourceStale(
            "index incomplete; inspect reported source issues".into(),
        ));
    }
    Ok(())
}
