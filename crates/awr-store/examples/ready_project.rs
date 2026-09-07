use awr_store::Store;
fn main() -> awr_core::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            awr_core::Error::InvalidInput("usage: ready_project <project root>".into())
        })?;
    let store = Store::open_readonly(&root.join(".awr/state.db"))?;
    let project = store.project_by_root(&root)?;
    let report = store.ready_work(
        project.id,
        project.current_branch_id,
        awr_core::now_millis()?,
    )?;
    let ready=report.ready.iter().map(|w|serde_json::json!({"key":w.work.item.meta.external_key,"title":w.work.item.title,"next_action":w.work.item.next_action})).collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::json!({"project_revision":report.project_revision,"ready":ready,"blocked":report.blocked.len(),"diagnostic_codes":report.blocked.iter().flat_map(|w|w.diagnostics.iter().map(|d|d.code.clone())).collect::<std::collections::BTreeSet<_>>()})
    );
    Ok(())
}
