use awr_context::{RuleScopeInput, hard_context};
use awr_source::{Manifest, index_project};
use awr_store::Store;
fn main() -> awr_core::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(awr_core::Error::InvalidInput(
            "usage: hard_project <root> <work-key> <agent-id>".into(),
        ));
    }
    let root = std::path::Path::new(&args[0]).canonicalize()?;
    let manifest = Manifest::load(&root)?;
    let database = root.join(".awr/state.db");
    if !database.is_file() {
        return Err(awr_core::Error::NotFound("initialized AWR database".into()));
    }
    let mut store = Store::open(&database)?;
    let report = index_project(&mut store, &root, &manifest, false)?;
    if !report.ok {
        return Err(awr_core::Error::SourceStale(
            "source refresh incomplete".into(),
        ));
    }
    let project = store.project(report.project_id)?;
    let context = hard_context(
        &store,
        project.id,
        &args[1],
        project.current_branch_id,
        &RuleScopeInput {
            agent_id: Some(args[2].clone()),
            ..Default::default()
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&context)?);
    if !context.complete {
        return Err(awr_core::Error::ContextIncomplete(
            "hard context contains unresolved facts".into(),
        ));
    }
    Ok(())
}
