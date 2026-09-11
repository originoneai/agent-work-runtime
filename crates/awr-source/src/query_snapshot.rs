use crate::{IndexReport, IndexedSource, Manifest, index_project_locked};
use awr_core::{Freshness, Id, Result};
use awr_store::Store;
use serde_json::json;
use std::path::Path;

pub struct QuerySnapshot {
    pub store: Store,
    pub refresh: IndexReport,
    pub source_state_fingerprint: String,
}
/// Runtime revision is deliberately excluded: a session event cannot change source identity.
pub fn source_state_fingerprint(store: &Store, project: Id) -> Result<String> {
    let sources=store.sources(project)?.into_iter().map(|s|json!({"id":s.id,"domain":s.domain,"role":s.role,"locator":s.locator,"adapter":s.adapter,"config":s.config,"revision":s.revision,"fingerprint":s.fingerprint,"freshness":s.freshness})).collect::<Vec<_>>();
    Ok(crate::fingerprint(&serde_json::to_vec(
        &json!({"version":1,"project_id":project,"sources":sources}),
    )?))
}
/// Hold the same source transition guard through refresh and SQLite's coherent RAM copy.
/// Runtime events may advance the snapshot beyond the source refresh interval.
pub fn refresh_snapshot(origin: &mut Store, root: &Path) -> Result<QuerySnapshot> {
    let guard = origin.lock_sources()?;
    let manifest = Manifest::load(root)?;
    let mut refresh = index_project_locked(origin, root, &manifest, false, &guard, None)?;
    let store = origin.memory_snapshot(256 * 1024 * 1024)?;
    refresh.project_revision = store.project(refresh.project_id)?.project_revision;
    let source_state_fingerprint = source_state_fingerprint(&store, refresh.project_id)?;
    Ok(QuerySnapshot {
        store,
        refresh,
        source_state_fingerprint,
    })
}
/// Last recorded facts, without reading business files or promoting their currentness.
pub fn recorded_snapshot(root: &Path) -> Result<QuerySnapshot> {
    let store = Store::read_snapshot(&root.join(".awr/state.db"), 256 * 1024 * 1024)?;
    let project = store.project_by_root(root)?;
    let sources = store.sources(project.id)?;
    let pending = sources
        .iter()
        .filter(|s| s.freshness != Freshness::Fresh)
        .count();
    let refresh = IndexReport {
        ok: pending == 0,
        project_id: project.id,
        project_revision: project.project_revision,
        change_window: crate::indexer::SourceChangeWindow {
            after_revision: project.project_revision,
            through_revision: project.project_revision,
        },
        projection_complete: pending == 0,
        indexed: 0,
        unchanged: sources.len(),
        retired: 0,
        pending,
        sources: sources
            .into_iter()
            .map(|s| {
                Ok(IndexedSource {
                    warnings: store.source_warnings(&s)?,
                    source_id: s.id,
                    locator: s.locator,
                    action: "recorded".into(),
                    source_revision: s.revision,
                    freshness: s.freshness,
                })
            })
            .collect::<Result<_>>()?,
        issues: vec![],
    };
    let source_state_fingerprint = source_state_fingerprint(&store, project.id)?;
    Ok(QuerySnapshot {
        store,
        refresh,
        source_state_fingerprint,
    })
}
