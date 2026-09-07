use crate::{
    Store,
    catalog::{SOURCE_COLUMNS, id_at, revision_at, source_row},
    db_error,
};
use awr_core::{
    EntityKind, Error, EventDraft, Freshness, Id, ProjectionBatch, Result, Source, SourceRef,
    now_millis,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const KINDS: [EntityKind; 6] = [
    EntityKind::Goal,
    EntityKind::Plan,
    EntityKind::Rule,
    EntityKind::WorkItem,
    EntityKind::Decision,
    EntityKind::Evidence,
];

fn table(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Goal => "goals",
        EntityKind::Plan => "plans",
        EntityKind::Rule => "rules",
        EntityKind::WorkItem => "work_items",
        EntityKind::Decision => "decisions",
        EntityKind::Evidence => "evidence",
    }
}

// Only these static identifiers can enter generated SQL. Values remain bound parameters.
fn fields(kind: EntityKind) -> &'static [(&'static str, &'static str)] {
    match kind {
        EntityKind::Goal => &[
            ("status", "status"),
            ("priority", "priority"),
            ("success_criteria_json", "success_criteria"),
        ],
        EntityKind::Plan => &[
            ("status", "status"),
            ("summary", "summary"),
            ("scope_json", "scope"),
        ],
        EntityKind::Rule => &[
            ("text", "text"),
            ("severity", "severity"),
            ("scope_json", "scope"),
            ("unresolved_json", "unresolved"),
        ],
        EntityKind::WorkItem => &[
            ("kind", "kind"),
            ("required", "required"),
            ("raw_status", "raw_status"),
            ("status", "status"),
            ("priority", "priority"),
            ("milestone", "milestone"),
            ("score", "score"),
            ("evidence_level", "evidence_level"),
            ("summary", "summary"),
            ("next_action", "next_action"),
            ("blocker", "blocker"),
            ("acceptance_json", "acceptance"),
            ("tags_json", "tags"),
            ("paths_json", "paths"),
        ],
        EntityKind::Decision => &[
            ("status", "status"),
            ("decision", "decision"),
            ("rationale", "rationale"),
            ("affected_keys_json", "affected_keys"),
            ("paths_json", "paths"),
        ],
        EntityKind::Evidence => &[
            ("work_item_id", "work_item_id"),
            ("evidence_type", "evidence_type"),
            ("level", "level"),
            ("summary", "summary"),
            ("locator", "locator"),
            ("sha256", "sha256"),
            ("source_sha", "source_sha"),
            ("command", "command"),
            ("scope_json", "scope"),
            ("branch_id", "branch_id"),
            ("verified_at", "verified_at"),
        ],
    }
}

fn checked_source(conn: &Connection, expected: &Source) -> Result<Source> {
    let current = conn
        .query_row(
            &format!(
                "SELECT {SOURCE_COLUMNS} FROM sources WHERE project_id=?1 AND id=?2 AND active=1"
            ),
            params![expected.project_id.to_string(), expected.id.to_string()],
            source_row,
        )
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| Error::NotFound(format!("source {}", expected.id)))?;
    if current.revision != expected.revision || current.fingerprint != expected.fingerprint {
        return Err(Error::SourceConflict(format!(
            "source {} changed during indexing",
            expected.id
        )));
    }
    Ok(current)
}

fn validate_ref(value: &Value, source: &Source, fingerprint: &str) -> Result<()> {
    let reference: SourceRef = serde_json::from_value(value.clone())?;
    if reference.source_id != source.id
        || reference.source_revision != source.revision + 1
        || reference.source_fingerprint != fingerprint
        || reference.locator.is_empty()
    {
        return Err(Error::SourceConflict(
            "projection provenance does not match the indexed snapshot".into(),
        ));
    }
    Ok(())
}

impl Store {
    /// Stale/unavailable observations never replace the last successfully projected fingerprint.
    pub fn mark_source_freshness(
        &mut self,
        expected: &Source,
        freshness: Freshness,
    ) -> Result<Source> {
        if freshness == Freshness::Fresh {
            return Err(Error::InvalidInput(
                "only a successful projection can mark a source fresh".into(),
            ));
        }
        let current = checked_source(&self.conn, expected)?;
        if current.freshness == freshness {
            return Ok(current);
        }
        let revision = self.project(expected.project_id)?.project_revision;
        let mut event = EventDraft::new(
            "source.freshness_changed",
            "Source requires a successful projection",
        );
        event.payload = json!({"source_id":expected.id,"freshness":freshness});
        let (source, _) =
            self.runtime_transaction(expected.project_id, revision, event, |tx, _| {
                let mut source = checked_source(tx, expected)?;
                tx.execute(
                    "UPDATE sources SET freshness=?1 WHERE id=?2",
                    params![
                        serde_json::to_value(freshness)?.as_str(),
                        source.id.to_string()
                    ],
                )
                .map_err(db_error)?;
                source.freshness = freshness;
                Ok(source)
            })?;
        Ok(source)
    }

    /// Include inactive IDs so removal and later reappearance retain identity and runtime links.
    pub fn projection_ids(&self, source: &Source) -> Result<BTreeMap<(EntityKind, String), Id>> {
        let mut ids = BTreeMap::new();
        for kind in KINDS {
            let mut query = self
                .conn
                .prepare(&format!(
                    "SELECT external_key,id FROM {} WHERE project_id=?1 AND source_id=?2",
                    table(kind)
                ))
                .map_err(db_error)?;
            let rows = query
                .query_map(
                    params![source.project_id.to_string(), source.id.to_string()],
                    |r| Ok((r.get::<_, String>(0)?, id_at(r, 1)?)),
                )
                .map_err(db_error)?;
            for row in rows {
                let (key, id) = row.map_err(db_error)?;
                ids.insert((kind, key), id);
            }
        }
        Ok(ids)
    }

    pub fn source_projection_payloads(
        &self,
        source: &Source,
        kind: EntityKind,
    ) -> Result<Vec<Value>> {
        let mut query=self.conn.prepare(&format!("SELECT payload_json FROM {} WHERE project_id=?1 AND source_id=?2 AND active=1 ORDER BY external_key",table(kind))).map_err(db_error)?;
        query
            .query_map(
                params![source.project_id.to_string(), source.id.to_string()],
                |r| r.get::<_, String>(0),
            )
            .map_err(db_error)?
            .map(|r| Ok(serde_json::from_str(&r.map_err(db_error)?)?))
            .collect()
    }

    /// Replace one source's derived facts and commit provenance + event in the same transaction.
    /// A failed attempt retains the old facts, explicitly stale, for diagnosis only.
    pub fn commit_source_projection(
        &mut self,
        expected: &Source,
        fingerprint: &str,
        batch: ProjectionBatch,
    ) -> Result<Source> {
        if fingerprint.is_empty() || expected.revision >= i64::MAX as u64 {
            return Err(Error::InvalidInput(
                "invalid source fingerprint or revision overflow".into(),
            ));
        }
        let current = checked_source(&self.conn, expected)?;
        if current.freshness == Freshness::Fresh && current.fingerprint == fingerprint {
            return Ok(current);
        }
        self.mark_source_freshness(expected, Freshness::Stale)?;
        let revision = self.project(expected.project_id)?.project_revision;
        let payload = serde_json::to_value(batch)?;
        let mut event = EventDraft::new("source.projected", "Source projection committed");
        event.payload = json!({"source_id":expected.id,"source_revision":expected.revision+1,
            "fingerprint":fingerprint,"warnings":payload["warnings"]});
        let (source,_)=self.runtime_transaction(expected.project_id,revision,event,|tx,_| {
            let mut source=checked_source(tx,expected)?;
            for name in KINDS.into_iter().map(table).chain(["edges"]) {
                tx.execute(&format!("UPDATE {name} SET active=0 WHERE project_id=?1 AND source_id=?2"),
                    params![source.project_id.to_string(),source.id.to_string()]).map_err(db_error)?;
            }
            for kind in KINDS {
                let rows=payload[table(kind)].as_array().ok_or_else(||Error::InvalidInput("missing projection batch".into()))?;
                let mut keys=BTreeSet::new();
                for row in rows {
                    let key=row["external_key"].as_str().filter(|s|!s.trim().is_empty())
                        .ok_or_else(||Error::InvalidInput("projection requires external_key".into()))?;
                    if !keys.insert(key) { return Err(Error::SourceConflict(format!("duplicate {} key {key}",table(kind)))); }
                    upsert_projection(tx,kind,&source,fingerprint,row.clone())?;
                }
            }
            upsert_edges(tx,&source,fingerprint,&payload["edges"])?;
            tx.execute("UPDATE sources SET revision=revision+1,fingerprint=?1,freshness='fresh' WHERE id=?2",
                params![fingerprint,source.id.to_string()]).map_err(db_error)?;
            source.revision+=1;
            source.fingerprint=fingerprint.into();
            source.freshness=Freshness::Fresh;
            Ok(source)
        })?;
        Ok(source)
    }
}

fn upsert_projection(
    conn: &Connection,
    kind: EntityKind,
    source: &Source,
    fingerprint: &str,
    mut value: Value,
) -> Result<()> {
    validate_ref(&value["source_ref"], source, fingerprint)?;
    if kind == EntityKind::Evidence && value["project_id"] != json!(source.project_id) {
        return Err(Error::SourceConflict(
            "evidence belongs to another project".into(),
        ));
    }
    let name = table(kind);
    let existing = conn
        .query_row(
            &format!(
                "SELECT id,source_id,revision FROM {name} WHERE project_id=?1 AND external_key=?2"
            ),
            params![
                source.project_id.to_string(),
                value["external_key"].as_str()
            ],
            |r| {
                Ok((
                    id_at(r, 0)?,
                    r.get::<_, Option<String>>(1)?,
                    revision_at(r, 2)?,
                ))
            },
        )
        .optional()
        .map_err(db_error)?;
    value["revision"] = if let Some((id, owner, revision)) = existing {
        if owner.as_deref() != Some(&source.id.to_string()) || value["id"] != json!(id) {
            return Err(Error::SourceConflict(format!(
                "{name} {} already has a different source or identity",
                value["external_key"]
            )));
        }
        if revision >= i64::MAX as u64 {
            return Err(Error::InvalidInput("entity revision overflow".into()));
        }
        json!(revision + 1)
    } else {
        json!(1)
    };
    let mut columns = vec![
        "id",
        "project_id",
        "external_key",
        "source_id",
        "source_ref_json",
        "source_revision",
        "revision",
        "active",
        "payload_json",
    ];
    let mut values = vec![
        "json_extract(?1,'$.id')".into(),
        "?2".into(),
        "json_extract(?1,'$.external_key')".into(),
        "?3".into(),
        "json_extract(?1,'$.source_ref')".into(),
        "?4".into(),
        "json_extract(?1,'$.revision')".into(),
        "1".into(),
        "?1".into(),
    ];
    if kind != EntityKind::Evidence {
        columns.extend(["title", "updated_at"]);
        values.extend([
            "coalesce(json_extract(?1,'$.title'),'')".into(),
            "?5".into(),
        ]);
    }
    for (column, key) in fields(kind) {
        columns.push(column);
        values.push(format!("json_extract(?1,'$.{key}')"));
    }
    let updates = columns
        .iter()
        .filter(|c| !["id", "project_id", "external_key"].contains(c))
        .map(|c| format!("{c}=excluded.{c}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "INSERT INTO {name}({}) VALUES({}) ON CONFLICT(project_id,external_key) DO UPDATE SET {updates}",
        columns.join(","),
        values.join(",")
    );
    let body = serde_json::to_string(&value)?;
    let revision = (source.revision + 1) as i64;
    if kind == EntityKind::Evidence {
        conn.execute(
            &sql,
            params![
                body,
                source.project_id.to_string(),
                source.id.to_string(),
                revision
            ],
        )
        .map_err(db_error)?;
    } else {
        conn.execute(
            &sql,
            params![
                body,
                source.project_id.to_string(),
                source.id.to_string(),
                revision,
                now_millis()?
            ],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

fn upsert_edges(conn: &Connection, source: &Source, fingerprint: &str, rows: &Value) -> Result<()> {
    let mut keys = BTreeSet::new();
    for row in rows
        .as_array()
        .ok_or_else(|| Error::InvalidInput("missing edges".into()))?
    {
        validate_ref(&row["source_ref"], source, fingerprint)?;
        if row["project_id"] != json!(source.project_id) {
            return Err(Error::SourceConflict(
                "edge belongs to another project".into(),
            ));
        }
        let key = serde_json::to_string(&[
            &row["from_kind"],
            &row["from_key"],
            &row["relation"],
            &row["to_kind"],
            &row["to_key"],
        ])?;
        if !keys.insert(key) {
            return Err(Error::SourceConflict("duplicate source edge".into()));
        }
        conn.execute("INSERT INTO edges(id,project_id,from_kind,from_key,relation,to_kind,to_key,required,source_id,source_ref_json,source_revision,revision,active)
            VALUES(json_extract(?1,'$.id'),?2,json_extract(?1,'$.from_kind'),json_extract(?1,'$.from_key'),json_extract(?1,'$.relation'),
                json_extract(?1,'$.to_kind'),json_extract(?1,'$.to_key'),json_extract(?1,'$.required'),?3,json_extract(?1,'$.source_ref'),?4,1,1)
            ON CONFLICT(project_id,source_id,from_kind,from_key,relation,to_kind,to_key) DO UPDATE SET
                required=excluded.required,source_ref_json=excluded.source_ref_json,source_revision=excluded.source_revision,revision=edges.revision+1,active=1",
            params![serde_json::to_string(row)?,source.project_id.to_string(),source.id.to_string(),(source.revision+1) as i64]).map_err(db_error)?;
    }
    Ok(())
}
