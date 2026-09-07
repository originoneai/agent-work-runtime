use awr_store::{SourceRegistration, Store};
fn main() -> awr_core::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            awr_core::Error::InvalidInput("usage: catalog <existing fixture directory>".into())
        })?;
    let mut store = Store::open(&root.join("state.db"))?;
    let project = store.register_project(&root, "catalog-example", "Catalog Example")?;
    let original_revision = project.project_revision;
    let definition = SourceRegistration {
        domain: "ledger",
        role: "primary",
        locator: "file://ledger/work-ledger.yaml",
        format: "yaml",
        adapter: "yaml-ledger-v1",
    };
    let source = store.register_source(project.id, &definition)?;
    let after_source = store.project(project.id)?;
    let same = store.register_source(project.id, &definition)?;
    let unchanged = store.project(project.id)?;
    assert_eq!(source.id, same.id);
    assert_eq!(after_source.project_revision, unchanged.project_revision);
    assert_eq!(store.project_by_root(&root)?.id, project.id);
    assert_eq!(store.source(project.id, source.id)?.id, source.id);
    assert_eq!(store.sources(project.id)?.len(), 1);
    assert!(store.doctor()?.ok);
    println!(
        "{}",
        serde_json::json!({"project_id":project.id,"source_id":source.id,
        "initial_revision":original_revision,"source_registered_revision":after_source.project_revision,
        "idempotent_revision":unchanged.project_revision,"freshness":source.freshness,"schema":store.doctor()?.schema_version})
    );
    Ok(())
}
