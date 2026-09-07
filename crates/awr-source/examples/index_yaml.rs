//! Read-only developer intake. The user-facing source CLI is a separate ledger item.
use awr_core::{EntityKind, Error, Freshness, Result};
use awr_source::{
    Locator, Manifest, ParseContext, SourceAdapter, YamlLedgerAdapter, observe_source,
};
use awr_store::{SourceRegistration, Store};
use serde_json::json;
use std::{collections::BTreeMap, env, path::PathBuf};

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 3 {
        return Err(Error::InvalidInput(
            "index_yaml <project-root> <ledger-path> <database>".into(),
        ));
    }
    let root = PathBuf::from(&args[0]).canonicalize()?;
    let source_path = PathBuf::from(&args[1]);
    let source_path = if source_path.is_absolute() {
        source_path
    } else {
        root.join(source_path)
    }
    .canonicalize()?;
    let mut manifest = Manifest::parse(
        "[project]\nname='YAML intake'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='ledger.yaml'\nadapter='yaml-ledger-v1'\n",
    )?;
    manifest.sources[0].path = Some(source_path.clone());
    let spec = &manifest.sources[0];
    let adapter = YamlLedgerAdapter;
    let locator = Locator::from_spec(&root, &manifest, spec)?;
    let snapshot = locator.read(&root, 16 * 1024 * 1024)?;
    let mut store = Store::open(&PathBuf::from(&args[2]))?;
    let project = store.register_project(&root, "yaml-intake", "YAML intake")?;
    let source = store.register_source(
        project.id,
        &SourceRegistration {
            domain: "ledger",
            role: "primary",
            locator: &snapshot.locator,
            format: "yaml",
            adapter: adapter.name(),
        },
    )?;
    let observed = observe_source(&mut store, &source, &root, &locator, 16 * 1024 * 1024)?;
    if let Some(error) = observed.error {
        return Err(error);
    }
    let snapshot = observed
        .snapshot
        .ok_or_else(|| Error::SourceUnavailable("missing snapshot".into()))?;
    let mut warnings = vec![];
    if observed.changed || observed.source.freshness != Freshness::Fresh {
        let context = ParseContext {
            source: &observed.source,
            existing_ids: store.projection_ids(&source)?,
        };
        let batch = adapter.parse(&snapshot, &context, spec)?;
        warnings = batch.warnings.clone();
        adapter.project(&mut store, &observed.source, &snapshot, batch)?;
    }
    let source = store.source(project.id, source.id)?;
    let work = store.source_projection_payloads(&source, EntityKind::WorkItem)?;
    let mut statuses = BTreeMap::<String, usize>::new();
    for item in &work {
        *statuses
            .entry(item["status"].as_str().unwrap_or("unknown").into())
            .or_default() += 1;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"source_revision":source.revision,"freshness":source.freshness,
        "fingerprint":source.fingerprint,"project_revision":store.project(project.id)?.project_revision,
        "work_items":work.len(),"goals":store.source_projection_payloads(&source,EntityKind::Goal)?.len(),
        "milestones":store.source_projection_payloads(&source,EntityKind::Plan)?.len(),
        "evidence_references":store.source_projection_payloads(&source,EntityKind::Evidence)?.len(),
        "statuses":statuses,"warnings":warnings})
        )?
    );
    Ok(())
}
