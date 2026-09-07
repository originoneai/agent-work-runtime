use crate::{
    ProjectionChange, SourceState, Store,
    catalog::{id_at, revision_at},
    db_error,
    transaction::{optional_id, sqlite_revision},
};
use awr_core::{Error, Id, Result, Revision};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReference {
    pub id: Id,
    pub project_revision: Revision,
    pub created_at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportantEvent {
    pub event: EventReference,
    pub event_type: String,
    pub importance: String,
    pub summary: String,
    pub work_item_id: Option<Id>,
    pub session_id: Option<Id>,
    pub branch_id: Option<Id>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDelta {
    pub kind: String,
    pub id: Id,
    pub external_key: String,
    /// Latest transition, not an assertion that intervening edits did not occur.
    pub latest_action: String,
    pub before_revision: Option<Revision>,
    pub after_revision: Option<Revision>,
    pub occurrences: usize,
    pub first_event: EventReference,
    pub last_event: EventReference,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDelta {
    pub source_id: Id,
    pub event_count: usize,
    pub first_event: EventReference,
    pub last_event: EventReference,
    pub latest_operation: String,
    pub before_known: bool,
    pub before: Option<SourceState>,
    pub after: Option<SourceState>,
    /// Older receipts without entity tracking are never backfilled from today's projection.
    pub legacy_events: usize,
    pub changed_entities: Vec<EntityDelta>,
    pub changed_entity_count: usize,
    pub omitted_entities: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryCount {
    pub event_type: String,
    pub importance: String,
    pub before_or_at_baseline: usize,
    pub after_baseline: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaEvents {
    pub project_revision: Revision,
    pub source_changes: Vec<SourceDelta>,
    pub important_events: Vec<ImportantEvent>,
    pub important_event_count: usize,
    pub omitted_important_events: usize,
    pub process_history: Vec<HistoryCount>,
}

impl Store {
    /// One read snapshot; source changes are project-wide, process events are work/global + exact branch.
    /// Queries prune process payloads entirely. Source receipts contain only version/identity changes.
    pub fn delta_events(
        &self,
        project: Id,
        expected: Revision,
        work: Id,
        branch: Option<Id>,
        after: Revision,
        event_limit: usize,
        entity_limit_per_source: usize,
    ) -> Result<DeltaEvents> {
        if event_limit == 0
            || event_limit > 100
            || entity_limit_per_source == 0
            || entity_limit_per_source > 200
        {
            return Err(Error::InvalidInput(
                "delta limits: events 1..100, entities per source 1..200".into(),
            ));
        }
        let tx = self.conn.unchecked_transaction().map_err(db_error)?;
        let actual = tx
            .query_row(
                "SELECT project_revision FROM projects WHERE id=?1",
                [project.to_string()],
                |r| revision_at(r, 0),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| Error::NotFound(format!("project {project}")))?;
        if actual != expected {
            return Err(Error::RevisionConflict { expected, actual });
        }
        if after > actual {
            return Err(Error::InvalidInput(
                "delta baseline is newer than the project".into(),
            ));
        }
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM work_items WHERE project_id=?1 AND id=?2)",
                params![project.to_string(), work.to_string()],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if !exists {
            return Err(Error::NotFound(format!("work item {work}")));
        }
        let mut sources: BTreeMap<Id, (SourceDelta, BTreeMap<(String, Id), EntityDelta>)> =
            BTreeMap::new();
        {
            // Exclude old warnings and other payload fields from the row materialized by SQLite.
            let mut stmt = tx.prepare("SELECT id,project_revision,created_at,event_type,
                json_extract(payload_json,'$.source_id'),json_extract(payload_json,'$.change_schema'),
                json_extract(payload_json,'$.before'),json_extract(payload_json,'$.after'),json_extract(payload_json,'$.changes')
                FROM events WHERE project_id=?1 AND project_revision>?2 AND event_type LIKE 'source.%'
                ORDER BY project_revision,created_at,id").map_err(db_error)?;
            let rows = stmt
                .query_map(params![project.to_string(), sqlite_revision(after)?], |r| {
                    Ok((
                        EventReference {
                            id: id_at(r, 0)?,
                            project_revision: revision_at(r, 1)?,
                            created_at: r.get(2)?,
                        },
                        r.get::<_, String>(3)?,
                        id_at(r, 4)?,
                        r.get::<_, Option<i64>>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, Option<String>>(7)?,
                        r.get::<_, Option<String>>(8)?,
                    ))
                })
                .map_err(db_error)?;
            for row in rows {
                let (event, operation, source, schema, before_json, after_json, changes_json) =
                    row.map_err(db_error)?;
                let known = schema == Some(1);
                let before = if known {
                    before_json
                        .map(|s| serde_json::from_str::<SourceState>(&s))
                        .transpose()?
                } else {
                    None
                };
                let next = if known {
                    Some(serde_json::from_str::<SourceState>(
                        &after_json.ok_or_else(|| {
                            Error::Storage("source receipt missing after state".into())
                        })?,
                    )?)
                } else {
                    None
                };
                let changes =
                    if known {
                        serde_json::from_str::<Vec<ProjectionChange>>(&changes_json.ok_or_else(
                            || Error::Storage("source receipt missing changes".into()),
                        )?)?
                    } else {
                        vec![]
                    };
                let (fold, entities) = sources.entry(source).or_insert_with(|| {
                    (
                        SourceDelta {
                            source_id: source,
                            event_count: 0,
                            first_event: event.clone(),
                            last_event: event.clone(),
                            latest_operation: operation.clone(),
                            before_known: known,
                            before,
                            after: None,
                            legacy_events: 0,
                            changed_entities: vec![],
                            changed_entity_count: 0,
                            omitted_entities: 0,
                        },
                        BTreeMap::new(),
                    )
                });
                fold.event_count += 1;
                fold.last_event = event.clone();
                fold.latest_operation = operation;
                fold.after = next;
                if !known {
                    fold.legacy_events += 1;
                }
                for change in changes {
                    let delta = entities
                        .entry((change.kind.clone(), change.id))
                        .or_insert_with(|| EntityDelta {
                            kind: change.kind,
                            id: change.id,
                            external_key: change.external_key,
                            latest_action: change.action.clone(),
                            before_revision: change.before_revision,
                            after_revision: change.after_revision,
                            occurrences: 0,
                            first_event: event.clone(),
                            last_event: event.clone(),
                        });
                    delta.latest_action = change.action;
                    delta.after_revision = change.after_revision;
                    delta.occurrences += 1;
                    delta.last_event = event.clone();
                }
            }
        }
        let source_changes = sources
            .into_values()
            .map(|(mut source, entities)| {
                let mut changes = entities.into_values().collect::<Vec<_>>();
                changes.sort_by(|a, b| {
                    b.last_event
                        .project_revision
                        .cmp(&a.last_event.project_revision)
                        .then_with(|| {
                            (&a.kind, &a.external_key, a.id).cmp(&(&b.kind, &b.external_key, b.id))
                        })
                });
                source.changed_entity_count = changes.len();
                changes.truncate(entity_limit_per_source);
                source.omitted_entities = source.changed_entity_count - changes.len();
                source.changed_entities = changes;
                source
            })
            .collect();
        let process_history = tx.prepare("SELECT event_type,importance,
            sum(CASE WHEN project_revision<=?4 THEN 1 ELSE 0 END),sum(CASE WHEN project_revision>?4 THEN 1 ELSE 0 END)
            FROM events WHERE project_id=?1 AND (work_item_id IS NULL OR work_item_id=?2) AND branch_id IS ?3
            AND event_type NOT LIKE 'source.%' GROUP BY event_type,importance ORDER BY event_type,importance").map_err(db_error)?
            .query_map(params![project.to_string(), work.to_string(), branch.map(|b| b.to_string()), sqlite_revision(after)?], |r| Ok(HistoryCount {
                event_type: r.get(0)?, importance: r.get(1)?, before_or_at_baseline: r.get::<_, i64>(2)? as usize, after_baseline: r.get::<_, i64>(3)? as usize,
            })).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
        let important_event_count = process_history
            .iter()
            .filter(|g| matches!(g.importance.as_str(), "high" | "critical"))
            .map(|g| g.after_baseline)
            .sum::<usize>();
        let important_events = tx.prepare("SELECT id,project_revision,created_at,event_type,importance,substr(summary,1,240),
            work_item_id,session_id,branch_id,length(summary)>240
            FROM events WHERE project_id=?1 AND (work_item_id IS NULL OR work_item_id=?2) AND branch_id IS ?3
            AND project_revision>?4 AND importance IN ('high','critical') AND event_type NOT LIKE 'source.%'
            ORDER BY CASE importance WHEN 'critical' THEN 0 ELSE 1 END,project_revision DESC,created_at DESC,id DESC LIMIT ?5").map_err(db_error)?
            .query_map(params![project.to_string(),work.to_string(),branch.map(|b| b.to_string()),sqlite_revision(after)?,event_limit as i64], |r| {
                let mut summary = r.get::<_, String>(5)?;
                if r.get::<_, bool>(9)? { summary.push('…'); }
                Ok(ImportantEvent {
                    event: EventReference {id: id_at(r, 0)?, project_revision: revision_at(r, 1)?, created_at: r.get(2)?},
                    event_type: r.get(3)?, importance: r.get(4)?, summary, work_item_id: optional_id(r, 6)?, session_id: optional_id(r, 7)?, branch_id: optional_id(r, 8)?,
                })
            }).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok(DeltaEvents {
            project_revision: actual,
            source_changes,
            omitted_important_events: important_event_count - important_events.len(),
            important_event_count,
            important_events,
            process_history,
        })
    }
}
