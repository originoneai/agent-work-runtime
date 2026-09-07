use crate::{
    Store,
    catalog::{id_at, revision_at},
    db_error,
};
use awr_core::{Error, Event, EventDraft, Id, Result, Revision, now_millis};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

pub(crate) fn insert_event(conn: &Connection, event: &Event) -> Result<()> {
    conn.execute("INSERT INTO events(id,project_id,work_item_id,session_id,branch_id,event_type,importance,summary,payload_json,project_revision,created_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![event.id.to_string(),event.project_id.to_string(),event.work_item_id.map(|id|id.to_string()),
            event.session_id.map(|id|id.to_string()),event.branch_id.map(|id|id.to_string()),event.event_type,
            event.importance,event.summary,serde_json::to_string(&event.payload)?,sqlite_revision(event.project_revision)?,event.created_at]).map_err(db_error)?;
    Ok(())
}

pub(crate) fn sqlite_revision(revision: Revision) -> Result<i64> {
    revision
        .try_into()
        .map_err(|_| Error::InvalidInput("revision exceeds SQLite integer range".into()))
}

pub(crate) fn optional_id(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<Option<Id>> {
    let value: Option<String> = row.get(column)?;
    value.map(|s| s.parse()).transpose().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e))
    })
}

impl Store {
    /// All database changes, event append and revision advance succeed together.
    /// The callback is crate-private and must not perform filesystem side effects.
    pub(crate) fn runtime_transaction<T>(
        &mut self,
        project_id: Id,
        expected_revision: Revision,
        draft: EventDraft,
        apply: impl FnOnce(&Transaction<'_>, Revision) -> Result<T>,
    ) -> Result<(T, Event)> {
        self.runtime_transaction_with_event(
            project_id,
            expected_revision,
            draft,
            |tx, revision, _| apply(tx, revision),
        )
    }

    /// Populate event details from the changes made in this transaction, never a preceding read.
    pub(crate) fn runtime_transaction_with_event<T>(
        &mut self,
        project_id: Id,
        expected_revision: Revision,
        mut draft: EventDraft,
        apply: impl FnOnce(&Transaction<'_>, Revision, &mut EventDraft) -> Result<T>,
    ) -> Result<(T, Event)> {
        if draft.event_type.trim().is_empty()
            || !draft.payload.is_object()
            || !["low", "normal", "high", "critical"].contains(&draft.importance.as_str())
        {
            return Err(Error::InvalidInput(
                "event requires a type, object payload and valid importance".into(),
            ));
        }
        let expected_sql = sqlite_revision(expected_revision)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let actual = tx
            .query_row(
                "SELECT project_revision FROM projects WHERE id=?1",
                [project_id.to_string()],
                |r| revision_at(r, 0),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| Error::NotFound(format!("project {project_id}")))?;
        if actual != expected_revision {
            return Err(Error::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        let next = actual
            .checked_add(1)
            .ok_or_else(|| Error::InvalidInput("revision overflow".into()))?;
        let next_sql = sqlite_revision(next)?;
        let result = apply(&tx, next, &mut draft)?;
        crate::events::bind_event(&tx, project_id, &mut draft, false)?;
        let updated = tx
            .execute(
                "UPDATE projects SET project_revision=?1 WHERE id=?2 AND project_revision=?3",
                params![next_sql, project_id.to_string(), expected_sql],
            )
            .map_err(db_error)?;
        if updated != 1 {
            return Err(Error::MutationConflict(
                "transaction changed project revision outside the domain contract".into(),
            ));
        }
        let event = Event {
            id: Id::new(),
            project_id,
            work_item_id: draft.work_item_id,
            session_id: draft.session_id,
            branch_id: draft.branch_id,
            event_type: draft.event_type,
            importance: draft.importance,
            summary: draft.summary,
            payload: draft.payload,
            project_revision: next,
            created_at: now_millis()?,
        };
        insert_event(&tx, &event)?;
        tx.commit().map_err(db_error)?;
        Ok((result, event))
    }

    pub fn append_event(
        &mut self,
        project_id: Id,
        expected_revision: Revision,
        draft: EventDraft,
    ) -> Result<Event> {
        self.runtime_transaction_with_event(project_id, expected_revision, draft, |tx, _, event| {
            crate::events::bind_event(tx, project_id, event, true)?;
            if matches!(
                event.event_type.as_str(),
                "session.started"
                    | "session.ended"
                    | "session.handoff_received"
                    | "work.claimed"
                    | "work.handoff"
                    | "claim.released"
                    | "claim.expired"
                    | "checkpoint.created"
                    | "artifact.recorded"
                    | "evidence.recorded"
            ) {
                return Err(Error::InvalidInput(
                    "runtime event type is reserved; use the corresponding domain operation".into(),
                ));
            }
            Ok(())
        })
        .map(|(_, event)| event)
    }

    pub fn events_since(
        &self,
        project_id: Id,
        after: Revision,
        limit: usize,
    ) -> Result<Vec<Event>> {
        if limit == 0 || limit > 1000 {
            return Err(Error::InvalidInput("event limit must be 1..1000".into()));
        }
        self.project(project_id)?;
        self.conn.prepare("SELECT id,project_id,work_item_id,session_id,branch_id,event_type,importance,summary,payload_json,project_revision,created_at
            FROM events WHERE project_id=?1 AND project_revision>?2 ORDER BY project_revision,created_at,id LIMIT ?3")
            .map_err(db_error)?.query_map(params![project_id.to_string(),sqlite_revision(after)?,limit as i64],|r|{
                let payload:String=r.get(8)?;
                Ok(Event{id:id_at(r,0)?,project_id:id_at(r,1)?,work_item_id:optional_id(r,2)?,session_id:optional_id(r,3)?,
                    branch_id:optional_id(r,4)?,event_type:r.get(5)?,importance:r.get(6)?,summary:r.get(7)?,
                    payload:serde_json::from_str(&payload).map_err(|e|rusqlite::Error::FromSqlConversionFailure(
                        8,rusqlite::types::Type::Text,Box::new(e)))?,project_revision:revision_at(r,9)?,created_at:r.get(10)?})
            }).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }
}
