use awr_context::{DeltaBaseline, DeltaRequest, recent_delta};
use awr_core::{Error, Result};
use awr_source::{Manifest, index_project};
use awr_store::Store;
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !(2..=3).contains(&args.len()) {
        return Err(Error::InvalidInput(
            "usage: delta_project <root> <work-key> [checkpoint-id|revision]".into(),
        ));
    }
    let root = std::path::Path::new(&args[0]).canonicalize()?;
    let manifest = Manifest::load(&root)?;
    let database = root.join(".awr/state.db");
    if !database.is_file() {
        return Err(Error::NotFound("initialized AWR database".into()));
    }
    let mut store = Store::open(&database)?;
    let indexed = index_project(&mut store, &root, &manifest, false)?;
    if !indexed.ok {
        return Err(Error::SourceStale("source refresh incomplete".into()));
    }
    let project = store.project(indexed.project_id)?;
    let baseline = match args.get(2) {
        None => DeltaBaseline::Auto,
        Some(value) => match value.parse::<u64>() {
            Ok(revision) => DeltaBaseline::Revision { revision },
            Err(_) => DeltaBaseline::Checkpoint {
                id: value.parse().map_err(|_| {
                    Error::InvalidInput("baseline must be a checkpoint ID or revision".into())
                })?,
            },
        },
    };
    let delta = recent_delta(
        &store,
        project.id,
        &args[1],
        project.current_branch_id,
        &DeltaRequest {
            baseline,
            ..Default::default()
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&delta)?);
    Ok(())
}
