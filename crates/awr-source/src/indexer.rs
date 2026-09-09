use crate::{
    Locator, Manifest, MarkdownDirectoryAdapter, MarkdownHeadingAdapter, MarkdownRulesAdapter,
    ParseContext, SourceAdapter, SourceSpec, YamlLedgerAdapter, observe_source,
};
use awr_core::{Error, Freshness, Id, Result, Revision, Source};
use awr_store::{SourceRegistration, Store};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, Serialize)]
pub struct IndexIssue {
    pub mapping: String,
    pub locator: Option<String>,
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct IndexedSource {
    pub source_id: Id,
    pub locator: String,
    pub action: String,
    pub source_revision: Revision,
    pub freshness: Freshness,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct IndexReport {
    pub ok: bool,
    pub project_id: Id,
    pub project_revision: Revision,
    pub indexed: usize,
    pub unchanged: usize,
    pub retired: usize,
    pub pending: usize,
    pub sources: Vec<IndexedSource>,
    pub issues: Vec<IndexIssue>,
}
impl IndexReport {
    fn issue(&mut self, mapping: &str, locator: Option<&str>, error: &Error) {
        self.ok = false;
        self.issues.push(IndexIssue {
            mapping: awr_core::safe_diagnostic(mapping),
            locator: locator.map(awr_core::safe_diagnostic),
            code: error.code().into(),
            message: error.report().message,
        });
    }
    fn source(&mut self, source: Source, action: &str, warnings: Vec<String>) {
        self.sources.push(IndexedSource {
            source_id: source.id,
            locator: source.locator,
            action: action.into(),
            source_revision: source.revision,
            freshness: source.freshness,
            warnings,
        });
    }
}

pub fn source_adapter(name: &str) -> Result<Box<dyn SourceAdapter>> {
    match name {
        "yaml-ledger-v1" => Ok(Box::new(YamlLedgerAdapter)),
        "markdown-ledger-v1" => Ok(Box::new(crate::MarkdownLedgerAdapter)),
        "markdown-heading-v1" => Ok(Box::new(MarkdownHeadingAdapter)),
        "markdown-rules-v1" => Ok(Box::new(MarkdownRulesAdapter)),
        "markdown-directory-v1" => Ok(Box::new(MarkdownDirectoryAdapter)),
        _ => Err(Error::Unsupported(format!("source adapter {name}"))),
    }
}
fn mapping_key(spec: &SourceSpec) -> String {
    format!(
        "{}|{}",
        spec.domain,
        spec.locator.clone().unwrap_or_else(|| spec
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default())
    )
}
/// Configuration identity shared by indexing, source mutations and read-only diagnosis.
pub fn source_configuration(spec: &SourceSpec, minimal_context: bool) -> serde_json::Value {
    let mut config =
        json!({"mapping_key":mapping_key(spec),"adapter_options":spec.options,"adapter_version":2});
    if minimal_context {
        config["context_profile"] = json!("minimal");
    }
    config
}
fn source_mapping(source: &Source) -> Option<&str> {
    source.config.get("mapping_key").and_then(|v| v.as_str())
}

/// Rebuildable source facts only. Existing runtime history is preserved, never inferred from files.
pub fn index_project(
    store: &mut Store,
    root: &Path,
    manifest: &Manifest,
    force: bool,
) -> Result<IndexReport> {
    process_project(store, root, manifest, Some(force))
}

/// Refresh source registry and availability without parsing or promoting pending facts to fresh.
pub fn scan_project(store: &mut Store, root: &Path, manifest: &Manifest) -> Result<IndexReport> {
    process_project(store, root, manifest, None)
}

fn process_project(
    store: &mut Store,
    root: &Path,
    manifest: &Manifest,
    mode: Option<bool>,
) -> Result<IndexReport> {
    manifest.validate()?;
    let project = store.register_project(
        root,
        manifest
            .project
            .external_key
            .as_deref()
            .unwrap_or(&manifest.project.name),
        &manifest.project.name,
    )?;
    let previous = store.sources(project.id)?;
    let mut report = IndexReport {
        ok: true,
        project_id: project.id,
        project_revision: project.project_revision,
        indexed: 0,
        unchanged: 0,
        retired: 0,
        pending: 0,
        sources: vec![],
        issues: vec![],
    };
    let mut desired = BTreeSet::new();
    let mut owners = BTreeMap::new();
    let mut plans = vec![];
    // Resolve all mappings first so aliases/overlapping directories cannot silently acquire one source twice.
    for spec in &manifest.sources {
        let key = mapping_key(spec);
        desired.insert(key.clone());
        let adapter = source_adapter(&spec.adapter)?;
        let discovered = adapter.discover(root, manifest, spec).and_then(|locators| {
            locators
                .into_iter()
                .map(|locator| {
                    let identity = if spec.adapter == "markdown-directory-v1" {
                        MarkdownDirectoryAdapter.source_identity(root, manifest, spec, &locator)?
                    } else {
                        locator.identity()?
                    };
                    Ok((locator, identity))
                })
                .collect::<Result<Vec<_>>>()
        });
        if let Ok(files) = &discovered {
            for (_, identity) in files {
                if let Some(other) =
                    owners.insert((spec.domain.clone(), identity.clone()), key.clone())
                {
                    return Err(Error::SourceConflict(format!(
                        "source {identity} belongs to overlapping mappings {other} and {key}"
                    )));
                }
            }
        }
        plans.push((key, spec, adapter, discovered));
    }
    let mut seen = BTreeSet::new();
    let mut discovered_mappings = BTreeSet::new();
    for (key, spec, adapter, discovered) in plans {
        let files = match discovered {
            Ok(files) => {
                discovered_mappings.insert(key.clone());
                files
            }
            Err(error) => {
                report.issue(&key, None, &error);
                let freshness = if matches!(error, Error::SourceUnavailable(_) | Error::Io(_)) {
                    Freshness::Unavailable
                } else {
                    Freshness::Stale
                };
                for source in previous
                    .iter()
                    .filter(|source| source_mapping(source) == Some(&key))
                {
                    match store.mark_source_freshness(source, freshness) {
                        Ok(source) => report.source(source, "unavailable", vec![]),
                        Err(error) => report.issue(&key, Some(&source.locator), &error),
                    }
                }
                continue;
            }
        };
        for (locator, identity) in files {
            let outcome = index_one(
                store,
                root,
                spec,
                adapter.as_ref(),
                &locator,
                &identity,
                mode,
                manifest.project.context_profile == crate::ContextProfile::Minimal,
            );
            match outcome {
                Ok((source, indexed, warnings)) => {
                    seen.insert(source.id);
                    let action = match indexed {
                        Some(true) => {
                            report.indexed += 1;
                            "indexed"
                        }
                        Some(false) => {
                            report.unchanged += 1;
                            "unchanged"
                        }
                        None => {
                            report.pending += 1;
                            "pending"
                        }
                    };
                    report.source(source, action, warnings);
                }
                Err(error) => {
                    report.issue(&key, Some(&identity), &error);
                    // Report the stored state. Do not downgrade a concurrent successful writer after a conflict.
                    if let Some(source) = store
                        .sources(project.id)?
                        .into_iter()
                        .find(|source| source.domain == spec.domain && source.locator == identity)
                    {
                        seen.insert(source.id);
                        report.source(source, "failed", vec![]);
                    }
                }
            }
        }
    }
    for old in previous {
        let Some(key) = source_mapping(&old) else {
            continue;
        };
        if seen.contains(&old.id) {
            continue;
        }
        // A failed directory scan cannot prove deletion. A removed manifest mapping can.
        if !desired.contains(key) || discovered_mappings.contains(key) {
            match store.retire_source(&old) {
                Ok(()) => {
                    report.retired += 1;
                    let mut retired = old;
                    retired.freshness = Freshness::Stale;
                    retired.revision += 1;
                    report.source(retired, "retired", vec![]);
                }
                Err(error) => report.issue(key, Some(&old.locator), &error),
            }
        }
    }
    report.project_revision = store.project(project.id)?.project_revision;
    Ok(report)
}

fn index_one(
    store: &mut Store,
    root: &Path,
    spec: &SourceSpec,
    adapter: &dyn SourceAdapter,
    locator: &Locator,
    identity: &str,
    mode: Option<bool>,
    minimal_context: bool,
) -> Result<(Source, Option<bool>, Vec<String>)> {
    let project = store.project_by_root(root)?;
    let source = store.register_source(
        project.id,
        &SourceRegistration {
            domain: &spec.domain,
            role: &spec.role,
            locator: identity,
            format: if spec.adapter == "yaml-ledger-v1" {
                "yaml"
            } else {
                "markdown"
            },
            adapter: &spec.adapter,
        },
    )?;
    let source = store.configure_source(&source, source_configuration(spec, minimal_context))?;
    let cap = crate::source_read_cap(&spec.adapter)?;
    let observed = observe_source(store, &source, root, locator, cap)?;
    if let Some(error) = observed.error {
        return Err(error);
    }
    let snapshot = observed
        .snapshot
        .ok_or_else(|| Error::SourceUnavailable("source returned no snapshot".into()))?;
    let mut source = observed.source;
    if mode != Some(true) && !observed.changed && source.freshness == Freshness::Fresh {
        let warnings = store.source_warnings(&source)?;
        return Ok((source, Some(false), warnings));
    }
    if mode.is_none() {
        let warnings = store.source_warnings(&source)?;
        return Ok((source, None, warnings));
    }
    source = store.mark_source_freshness(&source, Freshness::Stale)?;
    let context = ParseContext {
        source: &source,
        existing_ids: store.projection_ids(&source)?,
    };
    let batch = adapter.parse(&snapshot, &context, spec)?;
    let warnings = batch.warnings.clone();
    if matches!(locator, Locator::File(_))
        && locator.read(root, cap)?.fingerprint != snapshot.fingerprint
    {
        return Err(Error::SourceConflict(
            "source changed while parsing; reindex required".into(),
        ));
    }
    adapter.project(store, &source, &snapshot, batch)?;
    Ok((store.source(project.id, source.id)?, Some(true), warnings))
}
