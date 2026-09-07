use awr_store::Store;
fn main() -> awr_core::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            awr_core::Error::InvalidInput("usage: evidence_project <project root>".into())
        })?;
    let store = Store::open_readonly(&root.join(".awr/state.db"))?;
    let project = store.project_by_root(&root)?;
    let decisions = store.decisions(project.id)?;
    let records = store.evidence_records(project.id)?;
    let relevant = store.decisions_for_work(project.id, "AWR-P2-003")?;
    let evidence =
        store.evidence_for_work(project.id, "AWR-P2-002", None, project.current_branch_id)?;
    println!(
        "{}",
        serde_json::json!({"project_revision":project.project_revision,"decisions":decisions.len(),"accepted_relevant_or_unknown":relevant.len(),"evidence_records":records.len(),"previous_work_evidence":evidence.iter().map(|e|serde_json::json!({"key":e.evidence.item.external_key,"level":e.evidence.item.level,"currency":e.currency,"missing_bindings":e.missing_bindings})).collect::<Vec<_>>()})
    );
    Ok(())
}
