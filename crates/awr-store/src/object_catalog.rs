//! Bounded, revision-bound traversal of the retained project catalog.
use crate::{
    Store,
    catalog::{SOURCE_COLUMNS, revision_at, source_row},
    db_error,
};
use awr_core::{Error, Id, Result, Revision, Source};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    Goal,
    Plan,
    Rule,
    Work,
    Decision,
    Source,
    Relation,
    Artifact,
    Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogScope {
    Active,
    Retired,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogCursor {
    pub version: u32,
    pub project_id: Id,
    pub project_revision: Revision,
    pub kind: CatalogKind,
    pub scope: CatalogScope,
    pub after_id: Id,
}

#[derive(Debug, Serialize)]
pub struct CatalogRow {
    pub item: Value,
    pub source: Option<Source>,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct CatalogPage {
    pub project_id: Id,
    pub project_revision: Revision,
    pub kind: CatalogKind,
    pub scope: CatalogScope,
    pub total: u64,
    pub active_total: u64,
    pub retired_total: u64,
    pub items: Vec<CatalogRow>,
    pub has_more: bool,
    pub next_cursor: Option<CatalogCursor>,
}

impl Store {
    /// The caller refreshes sources first. Any intervening runtime/source revision
    /// invalidates the cursor; this API never labels mixed pages as one snapshot.
    pub fn catalog_page(
        &self,
        project: Id,
        revision: Revision,
        kind: CatalogKind,
        scope: CatalogScope,
        cursor: Option<&CatalogCursor>,
        limit: usize,
    ) -> Result<CatalogPage> {
        if !(1..=200).contains(&limit) {
            return Err(Error::InvalidInput("catalog limit must be 1..200".into()));
        }
        let check = || -> Result<()> {
            let actual = self.project(project)?.project_revision;
            if actual != revision {
                return Err(Error::RevisionConflict {
                    expected: revision,
                    actual,
                });
            }
            Ok(())
        };
        check()?;
        if let Some(c) = cursor {
            if c.version != 1 || c.project_id != project || c.kind != kind || c.scope != scope {
                return Err(Error::InvalidInput(
                    "catalog cursor belongs to another version, project, kind or scope".into(),
                ));
            }
            if c.project_revision != revision {
                return Err(Error::RevisionConflict {
                    expected: c.project_revision,
                    actual: revision,
                });
            }
        }
        let table = match kind {
            CatalogKind::Goal => "goals",
            CatalogKind::Plan => "plans",
            CatalogKind::Rule => "rules",
            CatalogKind::Work => "work_items",
            CatalogKind::Decision => "decisions",
            CatalogKind::Source => "sources",
            CatalogKind::Relation => "edges",
            CatalogKind::Artifact => "artifacts",
            CatalogKind::Evidence => "evidence",
        };
        let has_source = !matches!(kind, CatalogKind::Source | CatalogKind::Artifact);
        let from = if has_source {
            format!(
                "{table} e LEFT JOIN sources s ON s.id=e.source_id AND s.project_id=e.project_id"
            )
        } else {
            format!("{table} e")
        };
        let active = match kind {
            CatalogKind::Source => "e.active=1",
            CatalogKind::Artifact => "1",
            CatalogKind::Evidence => "e.active=1 AND (e.source_id IS NULL OR s.active=1)",
            _ => "e.active=1 AND s.active=1",
        };
        let filter = match scope {
            CatalogScope::Active => format!("({active})"),
            CatalogScope::Retired => format!("NOT ({active})"),
            CatalogScope::All => "1".into(),
        };
        let (active_total, retired_total): (u64, u64) = self.conn.query_row(
            &format!("SELECT coalesce(sum(CASE WHEN {active} THEN 1 ELSE 0 END),0), coalesce(sum(CASE WHEN {active} THEN 0 ELSE 1 END),0) FROM {from} WHERE e.project_id=?1"),
            [project.to_string()], |r| Ok((revision_at(r,0)?, revision_at(r,1)?))).map_err(db_error)?;
        let payload = match kind {
            CatalogKind::Source => {
                "json_object('id',e.id,'domain',e.domain,'role',e.role,'locator',e.locator,'format',e.format,'adapter',e.adapter,'revision',e.revision,'fingerprint',e.fingerprint,'freshness',e.freshness)"
            }
            CatalogKind::Relation => {
                "json_object('id',e.id,'from_kind',e.from_kind,'from_key',e.from_key,'relation',e.relation,'to_kind',e.to_kind,'to_key',e.to_key,'required',json(CASE WHEN e.required THEN 'true' ELSE 'false' END),'revision',e.revision,'source_ref',json(e.source_ref_json))"
            }
            CatalogKind::Artifact => {
                "json_object('id',e.id,'project_id',e.project_id,'artifact_type',e.artifact_type,'locator',e.locator,'sha256',e.sha256,'size',e.size,'mime',e.mime,'source_event_id',e.source_event_id,'revision',e.revision)"
            }
            _ => "e.payload_json",
        };
        let source_columns = if has_source {
            SOURCE_COLUMNS
                .split(',')
                .map(|c| format!("s.{c}"))
                .collect::<Vec<_>>()
                .join(",")
        } else {
            std::iter::repeat_n("NULL", 11)
                .collect::<Vec<_>>()
                .join(",")
        };
        let sql = format!(
            "SELECT {source_columns},{payload},({active}) FROM {from} WHERE e.project_id=?1 AND {filter} AND (?2 IS NULL OR e.id>?2) ORDER BY e.id LIMIT ?3"
        );
        let mut rows = self
            .conn
            .prepare(&sql)
            .map_err(db_error)?
            .query_map(
                params![
                    project.to_string(),
                    cursor.map(|c| c.after_id.to_string()),
                    limit as i64 + 1
                ],
                |r| {
                    let item = serde_json::from_str(&r.get::<_, String>(11)?).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            11,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                    Ok(CatalogRow {
                        item,
                        source: if r.get::<_, Option<String>>(0)?.is_some() {
                            Some(source_row(r)?)
                        } else {
                            None
                        },
                        active: r.get(12)?,
                    })
                },
            )
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?;
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        let next_cursor = if has_more {
            let after_id = rows
                .last()
                .and_then(|r| r.item["id"].as_str())
                .ok_or_else(|| Error::Storage("catalog row has no identity".into()))?
                .parse()
                .map_err(|e| Error::Storage(format!("catalog identity: {e}")))?;
            Some(CatalogCursor {
                version: 1,
                project_id: project,
                project_revision: revision,
                kind,
                scope,
                after_id,
            })
        } else {
            None
        };
        check()?;
        Ok(CatalogPage {
            project_id: project,
            project_revision: revision,
            kind,
            scope,
            total: match scope {
                CatalogScope::Active => active_total,
                CatalogScope::Retired => retired_total,
                CatalogScope::All => active_total + retired_total,
            },
            active_total,
            retired_total,
            items: rows,
            has_more,
            next_cursor,
        })
    }
}
