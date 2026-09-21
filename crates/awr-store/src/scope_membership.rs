//! Derived membership on a private read snapshot. Authoritative rows are never
//! deleted or redacted to create isolation. Unknown/unbound objects stay private.
use crate::{Store, db_error, scoped_read::visible};
use awr_core::{Id, Result, Workstream};
use rusqlite::{Connection, params};
use std::collections::BTreeSet;

fn insert(conn: &Connection, kind: &str, id: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO temp.awr_read_visibility(kind,id) VALUES(?1,?2)",
        params![kind, id],
    )
    .map_err(db_error)?;
    Ok(())
}
fn keys_allowed(keys: &[String], works: &BTreeSet<String>, goals: &BTreeSet<String>) -> bool {
    !keys.is_empty()
        && keys
            .iter()
            .all(|key| key == "*" || works.contains(key) || goals.contains(key))
}

pub(crate) fn install(store: &Store, project: Id, stream: &Workstream) -> Result<()> {
    store.require_memory()?;
    let conn = &store.conn;
    let pid = project.to_string();
    let sid = stream.id.to_string();
    conn.execute_batch("CREATE TEMP TABLE awr_read_visibility(kind TEXT NOT NULL,id TEXT NOT NULL,PRIMARY KEY(kind,id)) WITHOUT ROWID;
        INSERT INTO search_fts(search_fts) VALUES('delete-all');
        DELETE FROM search_documents; DELETE FROM search_state;").map_err(db_error)?;
    let legacy: bool = conn
        .query_row(
            "SELECT mode='legacy' FROM workstream_catalogs WHERE project_id=?1",
            [&pid],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    if legacy {
        for table in [
            "goals",
            "plans",
            "rules",
            "work_items",
            "decisions",
            "evidence",
            "sources",
            "edges",
            "sessions",
            "events",
            "artifacts",
            "branches",
        ] {
            conn.execute(&format!("INSERT INTO temp.awr_read_visibility SELECT '{table}',id FROM {table} WHERE project_id=?1"),[&pid]).map_err(db_error)?;
        }
        conn.execute("INSERT INTO temp.awr_read_visibility SELECT 'checkpoints',c.id FROM checkpoints c JOIN sessions s ON s.id=c.session_id WHERE s.project_id=?1",[&pid]).map_err(db_error)?;
        return Ok(());
    }
    conn.execute("INSERT INTO temp.awr_read_visibility SELECT 'work_items',work_item_id FROM workstream_ownership WHERE project_id=?1 AND workstream_id=?2",params![pid,sid]).map_err(db_error)?;
    conn.execute("INSERT INTO temp.awr_read_visibility SELECT 'sessions',session_id FROM session_workstreams WHERE project_id=?1 AND workstream_id=?2",params![pid,sid]).map_err(db_error)?;
    conn.execute(&format!("INSERT INTO temp.awr_read_visibility SELECT 'checkpoints',c.id FROM checkpoints c WHERE {}",visible("sessions","c.session_id")),[]).map_err(db_error)?;
    let work_keys = conn
        .prepare(&format!(
            "SELECT external_key FROM work_items w WHERE w.project_id=?1 AND {}",
            visible("work_items", "w.id")
        ))
        .map_err(db_error)?
        .query_map([&pid], |r| r.get::<_, String>(0))
        .map_err(db_error)?
        .collect::<rusqlite::Result<BTreeSet<_>>>()
        .map_err(db_error)?;
    let goal_keys = stream.goal_keys.iter().cloned().collect::<BTreeSet<_>>();
    for key in &goal_keys {
        conn.execute("INSERT INTO temp.awr_read_visibility SELECT 'goals',id FROM goals WHERE project_id=?1 AND external_key=?2",params![pid,key]).map_err(db_error)?;
    }
    // Rule scopes express applicability, not confidentiality. The existing rule
    // source is a project policy surface; retain hard AND unknown applicability.
    conn.execute(
        "INSERT INTO temp.awr_read_visibility SELECT 'rules',id FROM rules WHERE project_id=?1",
        [&pid],
    )
    .map_err(db_error)?;
    for (table, field) in [("plans", "scope"), ("decisions", "affected_keys")] {
        let rows = conn
            .prepare(&format!(
                "SELECT id,json_extract(payload_json,'$.{field}') FROM {table} WHERE project_id=?1"
            ))
            .map_err(db_error)?
            .query_map([&pid], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?;
        for (id, raw) in rows {
            let keys: Vec<String> = serde_json::from_str(&raw)?;
            if keys_allowed(&keys, &work_keys, &goal_keys) {
                insert(conn, table, &id)?;
            }
        }
    }
    // An event with a session keeps that immutable attribution. Work-only
    // events use ownership at the event revision, including before a move.
    // A source event can describe several streams; it is not a private event.
    // Index the sparse move history once, rather than scanning all later
    // project events for every event in a long-running project.
    conn.execute_batch("CREATE TEMP TABLE awr_read_moves(work_item_id TEXT NOT NULL,at_revision INTEGER NOT NULL,from_scope TEXT NOT NULL,PRIMARY KEY(work_item_id,at_revision)) WITHOUT ROWID;").map_err(db_error)?;
    conn.execute("INSERT INTO temp.awr_read_moves SELECT json_extract(m.value,'$.work_item_id'),e.project_revision,json_extract(m.value,'$.from')
        FROM events e,json_each(e.payload_json,'$.workstream_moves') m WHERE e.project_id=?1 AND e.event_type='source.projected'",[&pid]).map_err(db_error)?;
    conn.execute("INSERT INTO temp.awr_read_visibility
        SELECT 'events',e.id FROM events e WHERE e.project_id=?1 AND e.event_type!='artifact.recorded' AND
        CASE WHEN e.session_id IS NOT NULL THEN
            (SELECT b.workstream_id FROM session_workstreams b WHERE b.project_id=e.project_id AND b.session_id=e.session_id AND b.work_item_id IS e.work_item_id)
        WHEN e.work_item_id IS NOT NULL THEN coalesce(
            (SELECT m.from_scope FROM temp.awr_read_moves m
             WHERE m.work_item_id=e.work_item_id AND m.at_revision>e.project_revision
             ORDER BY m.at_revision LIMIT 1),
            (SELECT o.workstream_id FROM workstream_ownership o WHERE o.project_id=e.project_id AND o.work_item_id=e.work_item_id))
        ELSE NULL END = ?2",params![pid,sid]).map_err(db_error)?;
    // Late artifact registration follows its original event, not whichever
    // workstream owns that work at registration time. Prior-event references
    // also allow chained artifact registrations without relabeling history.
    let artifacts = conn.prepare("SELECT id,json_extract(payload_json,'$.source_event_id') FROM events WHERE project_id=?1 AND event_type='artifact.recorded' ORDER BY project_revision,created_at,id")
        .map_err(db_error)?.query_map([&pid],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?)))
        .map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    for (event, source) in artifacts {
        conn.execute("INSERT OR IGNORE INTO temp.awr_read_visibility SELECT 'events',?1 WHERE EXISTS(SELECT 1 FROM temp.awr_read_visibility WHERE kind='events' AND id=?2)",params![event,source]).map_err(db_error)?;
    }
    conn.execute(&format!("INSERT INTO temp.awr_read_visibility SELECT 'artifacts',a.id FROM artifacts a WHERE a.project_id=?1 AND {}",visible("events","a.source_event_id")),[&pid]).map_err(db_error)?;
    // Runtime evidence is attributed by its immutable creation event. Source
    // evidence follows its explicit source ownership; mixed/unknown scope is
    // withheld until an authorized export exists.
    conn.execute(&format!("INSERT INTO temp.awr_read_visibility SELECT 'evidence',d.id FROM evidence d WHERE d.project_id=?1 AND d.source_id IS NULL AND EXISTS(SELECT 1 FROM events e WHERE e.project_id=d.project_id AND e.event_type='evidence.recorded' AND json_extract(e.payload_json,'$.evidence_id')=d.id AND {})",visible("events","e.id")),[&pid]).map_err(db_error)?;
    let rows = conn.prepare("SELECT id,work_item_id,scope_json FROM evidence WHERE project_id=?1 AND source_id IS NOT NULL").map_err(db_error)?
        .query_map([&pid],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?))).map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    for (id, work, raw) in rows {
        let keys: Vec<String> = serde_json::from_str(&raw)?;
        let own:bool = work.map(|id| conn.query_row("SELECT EXISTS(SELECT 1 FROM temp.awr_read_visibility WHERE kind='work_items' AND id=?1)",[id],|r|r.get(0)).map_err(db_error)).transpose()?.unwrap_or(true);
        if own && keys_allowed(&keys, &work_keys, &goal_keys) {
            insert(conn, "evidence", &id)?;
        }
    }
    // Only expose edges whose two endpoints are visible. Cross-stream exports
    // are a separate contract; an edge alone never authorizes its target body.
    conn.execute("CREATE TEMP VIEW awr_visible_keys AS
        SELECT 'work_item' kind,external_key FROM work_items WHERE id IN (SELECT id FROM awr_read_visibility WHERE kind='work_items')
        UNION ALL SELECT 'goal',external_key FROM goals WHERE id IN (SELECT id FROM awr_read_visibility WHERE kind='goals')
        UNION ALL SELECT 'plan',external_key FROM plans WHERE id IN (SELECT id FROM awr_read_visibility WHERE kind='plans')
        UNION ALL SELECT 'rule',external_key FROM rules WHERE id IN (SELECT id FROM awr_read_visibility WHERE kind='rules')
        UNION ALL SELECT 'decision',external_key FROM decisions WHERE id IN (SELECT id FROM awr_read_visibility WHERE kind='decisions')
        UNION ALL SELECT 'evidence',external_key FROM evidence WHERE id IN (SELECT id FROM awr_read_visibility WHERE kind='evidence')",[]).map_err(db_error)?;
    conn.execute("INSERT INTO temp.awr_read_visibility SELECT 'edges',e.id FROM edges e WHERE e.project_id=?1
        AND EXISTS(SELECT 1 FROM temp.awr_visible_keys v WHERE v.kind=e.from_kind AND v.external_key=e.from_key)
        AND EXISTS(SELECT 1 FROM temp.awr_visible_keys v WHERE v.kind=e.to_kind AND v.external_key=e.to_key)",[&pid]).map_err(db_error)?;
    // Branch names and whole source catalogs have project-wide metadata. A
    // scoped caller may use a branch referenced by its own immutable sessions;
    // whole-source inspection stays on the authorized project-admin surface.
    conn.execute(&format!("INSERT OR IGNORE INTO temp.awr_read_visibility SELECT 'branches',s.branch_id FROM sessions s WHERE s.project_id=?1 AND s.branch_id IS NOT NULL AND {}",visible("sessions","s.id")),[&pid]).map_err(db_error)?;
    Ok(())
}
