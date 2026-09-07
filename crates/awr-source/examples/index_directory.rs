//! Development directory intake; deleted-row reconciliation is provided by the incremental indexer.
use awr_core::{EntityKind, Error, Freshness, Result};
use awr_source::{Manifest, MarkdownDirectoryAdapter, ParseContext, SourceAdapter, observe_source};
use awr_store::{SourceRegistration, Store};
use serde_json::json;
use std::{env, fs, path::PathBuf};

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(Error::InvalidInput(
            "index_directory <root> <manifest-path> <source-index> <database>".into(),
        ));
    }
    let root = PathBuf::from(&args[0]).canonicalize()?;
    let manifest = Manifest::parse(&fs::read_to_string(&args[1])?)?;
    let index = args[2]
        .parse::<usize>()
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    let spec = manifest
        .sources
        .get(index)
        .ok_or_else(|| Error::InvalidInput("source index outside manifest".into()))?;
    let adapter = MarkdownDirectoryAdapter;
    let mut store = Store::open(&PathBuf::from(&args[3]))?;
    let project = store.register_project(
        &root,
        manifest
            .project
            .external_key
            .as_deref()
            .unwrap_or(&manifest.project.name),
        &manifest.project.name,
    )?;
    let mut reports = vec![];
    for locator in adapter.discover(&root, &manifest, spec)? {
        let identity = adapter.source_identity(&root, &manifest, spec, &locator)?;
        let source = store.register_source(
            project.id,
            &SourceRegistration {
                domain: "decisions",
                role: &spec.role,
                locator: &identity,
                format: "markdown",
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
        for decision in store.source_projection_payloads(&source, EntityKind::Decision)? {
            reports.push(json!({"title":decision["title"],"status":decision["status"],"raw_status":decision["raw_status"],
                "source_revision":source.revision,"source_fingerprint":source.fingerprint,"warnings":warnings,
                "decision_chars":decision["decision"].as_str().unwrap_or("").chars().count(),
                "rationale_chars":decision["rationale"].as_str().unwrap_or("").chars().count()}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"project_revision":store.project(project.id)?.project_revision,"decisions":reports})
        )?
    );
    Ok(())
}
