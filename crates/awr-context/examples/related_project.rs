use awr_context::related_work;
use awr_source::{Manifest, index_project};
use awr_store::Store;
fn main() -> awr_core::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !(2..=3).contains(&args.len()) {
        return Err(awr_core::Error::InvalidInput(
            "usage: related_project <root> <work-key> [source-sha]".into(),
        ));
    }
    let root = std::path::Path::new(&args[0]).canonicalize()?;
    let manifest = Manifest::load(&root)?;
    let database = root.join(".awr/state.db");
    if !database.is_file() {
        return Err(awr_core::Error::NotFound("initialized AWR database".into()));
    }
    let mut store = Store::open(&database)?;
    let indexed = index_project(&mut store, &root, &manifest, false)?;
    if !indexed.ok {
        return Err(awr_core::Error::SourceStale(
            "source refresh incomplete".into(),
        ));
    }
    let project = store.project(indexed.project_id)?;
    let related = related_work(
        &store,
        project.id,
        &args[1],
        project.current_branch_id,
        args.get(2).map(String::as_str),
        None,
    )?;
    println!("{}", serde_json::to_string_pretty(&related)?);
    Ok(())
}
