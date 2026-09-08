use crate::{Manifest, MarkdownDirectoryAdapter, SourceSpec, source_adapter};
use awr_core::{Error, Freshness, MutationPatch, Result, Source};
use serde::Serialize;
use serde_json::json;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct MutationSourceCheck {
    pub locator: String,
    pub fingerprint: String,
    pub adapter: String,
    pub checked_at: i64,
}

fn mapping_key(spec: &SourceSpec) -> String {
    format!(
        "{}|{}",
        spec.domain,
        spec.locator.clone().unwrap_or_else(|| spec
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default())
    )
}

/// Read-only observation of the actual current manifest mapping and source bytes. This is
/// not a file lock: a future writer must repeat the comparison immediately before writing.
pub fn verify_mutation_source(
    root: &Path,
    source: &Source,
    patch: &MutationPatch,
) -> Result<MutationSourceCheck> {
    patch.validate()?;
    let reference = &patch.target.meta.source_ref;
    if source.id != reference.source_id
        || source.fingerprint != reference.source_fingerprint
        || source.revision != reference.source_revision
        || source.config != patch.source_config
    {
        return Err(Error::SourceConflict(
            "proposal binding differs from the indexed source; create a new proposal".into(),
        ));
    }
    match source.freshness {
        Freshness::Fresh => (),
        Freshness::Stale => return Err(Error::SourceStale(
            "proposal source is stale; reindex before creating a replacement proposal".into(),
        )),
        Freshness::Unavailable => return Err(Error::SourceUnavailable(
            "proposal source is unavailable; restore and reindex before creating a replacement proposal".into(),
        )),
    }
    let root = root.canonicalize()?;
    let manifest = Manifest::load(&root)?;
    let mut matches = Vec::new();
    // Resolve every mapping in this domain, so a newly added alias cannot silently acquire
    // the proposal's source. A failed scan cannot establish unique ownership.
    for spec in manifest
        .sources
        .iter()
        .filter(|s| s.domain == source.domain)
    {
        let adapter = source_adapter(&spec.adapter)?;
        for locator in adapter.discover(&root, &manifest, spec)? {
            let identity = if spec.adapter == "markdown-directory-v1" {
                MarkdownDirectoryAdapter.source_identity(&root, &manifest, spec, &locator)?
            } else {
                locator.identity()?
            };
            if identity != source.locator {
                continue;
            }
            let config = json!({"mapping_key":mapping_key(spec),"adapter_options":spec.options,"adapter_version":1});
            if source.adapter != spec.adapter
                || source.role != spec.role
                || config != patch.source_config
            {
                return Err(Error::SourceConflict(
                    "proposal source mapping or adapter configuration has changed".into(),
                ));
            }
            matches.push(locator);
        }
    }
    if matches.len() != 1 {
        return Err(Error::SourceConflict(format!(
            "proposal source must have exactly one current manifest mapping; found {}",
            matches.len()
        )));
    }
    let snapshot = matches[0].read(&root, 16 * 1024 * 1024)?;
    if snapshot.fingerprint != reference.source_fingerprint || snapshot.locator != reference.locator
    {
        return Err(Error::SourceConflict("proposal source bytes or immutable locator changed; keep the proposal for review and create a new one from current facts".into()));
    }
    Ok(MutationSourceCheck {
        locator: snapshot.locator,
        fingerprint: snapshot.fingerprint,
        adapter: source.adapter.clone(),
        checked_at: awr_core::now_millis()?,
    })
}
