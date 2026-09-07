use awr_context::{CompletenessRequest, RuleScopeInput, check_completeness};
use awr_core::{Error, Result};
use awr_store::Store;
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !(2..=4).contains(&args.len()) {
        return Err(Error::InvalidInput(
            "usage: completeness_project <root> <work-key> [agent-id] [source-sha]".into(),
        ));
    }
    let root = std::path::Path::new(&args[0]).canonicalize()?;
    let database = root.join(".awr/state.db");
    if !database.is_file() {
        return Err(Error::NotFound("initialized AWR database".into()));
    }
    let mut store = Store::open(&database)?;
    let branch = store.project_by_root(&root)?.current_branch_id;
    let report = check_completeness(
        &mut store,
        &root,
        &CompletenessRequest {
            work_item_key: args[1].clone(),
            branch_id: branch,
            scope: RuleScopeInput {
                agent_id: args.get(2).cloned(),
                ..Default::default()
            },
            source_sha: args.get(3).cloned(),
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.complete {
        return Err(Error::ContextIncomplete(
            "See completeness fields and issues".into(),
        ));
    }
    Ok(())
}
