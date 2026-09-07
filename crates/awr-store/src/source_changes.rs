//! Source change receipts are written inside the projection transaction, not reconstructed later.
use crate::{catalog::revision_at, db_error};
use awr_core::{EventDraft, Freshness, Id, Result, Revision, Source};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceState {
    pub id: Id,
    pub domain: String,
    pub role: String,
    pub locator: String,
    pub format: String,
    pub adapter: String,
    pub revision: Revision,
    pub fingerprint: String,
    pub freshness: Freshness,
    pub active: bool,
}
impl SourceState {
    pub(crate) fn from_source(source: &Source, active: bool) -> Self {
        Self {
            id: source.id,
            domain: source.domain.clone(),
            role: source.role.clone(),
            locator: source.locator.clone(),
            format: source.format.clone(),
            adapter: source.adapter.clone(),
            revision: source.revision,
            fingerprint: source.fingerprint.clone(),
            freshness: source.freshness,
            active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionChange {
    /// Table/domain name; edges use their structural tuple as external_key.
    pub kind: String,
    pub id: Id,
    pub external_key: String,
    pub action: String,
    pub before_revision: Option<Revision>,
    pub after_revision: Option<Revision>,
}
pub(crate) type Versions = BTreeMap<(String, Id), (String, Revision)>;

pub(crate) fn projection_versions(conn: &Connection, source: &Source) -> Result<Versions> {
    let mut result = BTreeMap::new();
    for table in [
        "goals",
        "plans",
        "rules",
        "work_items",
        "decisions",
        "evidence",
        "edges",
    ] {
        let key = if table == "edges" {
            "json_array(from_kind,from_key,relation,to_kind,to_key)"
        } else {
            "external_key"
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT id,{key},revision FROM {table} WHERE project_id=?1 AND source_id=?2 AND active=1"
        )).map_err(db_error)?;
        let rows = stmt
            .query_map(
                params![source.project_id.to_string(), source.id.to_string()],
                |r| {
                    Ok((
                        crate::catalog::id_at(r, 0)?,
                        r.get::<_, String>(1)?,
                        revision_at(r, 2)?,
                    ))
                },
            )
            .map_err(db_error)?;
        for row in rows {
            let (id, key, revision) = row.map_err(db_error)?;
            result.insert((table.into(), id), (key, revision));
        }
    }
    Ok(result)
}

pub(crate) fn changes(before: &Versions, after: &Versions) -> Vec<ProjectionChange> {
    let mut result = Vec::new();
    for ((kind, id), (key, revision)) in before {
        match after.get(&(kind.clone(), *id)) {
            Some((_, next)) if next == revision => {}
            next => result.push(ProjectionChange {
                kind: kind.clone(),
                id: *id,
                external_key: key.clone(),
                action: if next.is_some() { "updated" } else { "removed" }.into(),
                before_revision: Some(*revision),
                after_revision: next.map(|(_, r)| *r),
            }),
        }
    }
    for ((kind, id), (key, revision)) in after {
        if !before.contains_key(&(kind.clone(), *id)) {
            result.push(ProjectionChange {
                kind: kind.clone(),
                id: *id,
                external_key: key.clone(),
                action: "added".into(),
                before_revision: None,
                after_revision: Some(*revision),
            });
        }
    }
    result.sort_by(|a, b| (&a.kind, &a.external_key, a.id).cmp(&(&b.kind, &b.external_key, b.id)));
    result
}

pub(crate) fn annotate(
    event: &mut EventDraft,
    before: Option<SourceState>,
    after: SourceState,
    changes: Vec<ProjectionChange>,
) -> Result<()> {
    event.payload["source_id"] = serde_json::to_value(after.id)?;
    event.payload["change_schema"] = serde_json::json!(1);
    event.payload["before"] = serde_json::to_value(before)?;
    event.payload["after"] = serde_json::to_value(after)?;
    event.payload["changes"] = serde_json::to_value(changes)?;
    Ok(())
}
