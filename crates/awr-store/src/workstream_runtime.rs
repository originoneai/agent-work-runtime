//! Immutable session attribution and client-local defaults. Authentication and
//! permission loading belong to the service adapter, not these storage methods.
use crate::{
    Store, db_error,
    session::{SESSION_COLUMNS, session_row},
    workstream::{recorded_catalog, require_fresh_catalog},
};
use awr_core::*;
use rusqlite::{Connection, OptionalExtension, params};

fn ownership(conn: &Connection, project: Id, work: Id) -> Result<(Id, Revision)> {
    conn.query_row("SELECT workstream_id,revision FROM workstream_ownership WHERE project_id=?1 AND work_item_id=?2",
        params![project.to_string(),work.to_string()], |row|Ok((crate::catalog::id_at(row,0)?,crate::catalog::revision_at(row,1)?)))
        .optional().map_err(db_error)?.ok_or_else(||Error::SourceConflict("work has no authoritative workstream binding".into()))
}

fn conversation_default(
    conn: &Connection,
    project: Id,
    binding: &McpSessionBinding,
) -> Result<Option<Id>> {
    conn.query_row("SELECT workstream_id FROM conversation_workstreams WHERE project_id=?1 AND client=?2 AND conversation=?3",
        params![project.to_string(),binding.client,binding.conversation], |row|crate::catalog::id_at(row,0)).optional().map_err(db_error)
}

pub(crate) fn recorded(conn: &Connection, project: Id, session: Id) -> Result<SessionWorkstream> {
    conn.query_row("SELECT work_item_id,workstream_id,ownership_revision,authority_version FROM session_workstreams WHERE project_id=?1 AND session_id=?2",
        params![project.to_string(),session.to_string()], |row|Ok(SessionWorkstream {
            project_id: project.to_string(),session_id:session.to_string(),work_item_id:row.get(0)?,
            workstream_id:crate::transaction::optional_id(row,1)?,ownership_revision:row.get::<_,Option<i64>>(2)?.map(|_|crate::catalog::revision_at(row,2)).transpose()?,authority_version:row.get::<_,Option<i64>>(3)?.map(|_|crate::catalog::revision_at(row,3)).transpose()?,
        })).optional().map_err(db_error)?.ok_or_else(||Error::Storage("session workstream binding is missing".into()))
}

fn save(conn: &Connection, scope: &SessionWorkstream) -> Result<()> {
    conn.execute("INSERT INTO session_workstreams(project_id,session_id,work_item_id,workstream_id,ownership_revision,authority_version) VALUES(?1,?2,?3,?4,?5,?6)",
        params![scope.project_id,scope.session_id,scope.work_item_id,scope.workstream_id.map(|id|id.to_string()),
            scope.ownership_revision.map(crate::transaction::sqlite_revision).transpose()?,scope.authority_version.map(crate::transaction::sqlite_revision).transpose()?]).map_err(db_error)?;
    Ok(())
}

pub(crate) fn bind(
    conn: &Connection,
    session: &Session,
    explicit: Option<Id>,
    conversation: Option<&McpSessionBinding>,
) -> Result<SessionWorkstream> {
    let project = session.project_id;
    require_fresh_catalog(conn, &project.to_string())?;
    let catalog = recorded_catalog(conn, &project.to_string())?;
    let owner = session
        .work_item_id
        .map(|work| ownership(conn, project, work))
        .transpose()?;
    let scope = if let Some((id, _)) = owner {
        if explicit.is_some_and(|selected| selected != id) {
            return Err(WorkstreamError::BindingMismatch.into());
        }
        id
    } else if let Some(id) = explicit {
        id
    } else if let Some(id) = conversation
        .map(|binding| conversation_default(conn, project, binding))
        .transpose()?
        .flatten()
    {
        id
    } else if catalog.workstreams.len() == 1 {
        catalog.workstreams[0].id
    } else {
        return Err(WorkstreamError::ScopeRequired.into());
    };
    let definition = catalog.get(scope)?;
    if definition.state != WorkstreamState::Active {
        return Err(WorkstreamError::Inactive.into());
    }
    let binding = SessionWorkstream {
        project_id: project.to_string(),
        session_id: session.id.to_string(),
        work_item_id: session.work_item_id.map(|id| id.to_string()),
        workstream_id: Some(scope),
        ownership_revision: owner.map(|(_, revision)| revision),
        authority_version: Some(definition.authority_version),
    };
    save(conn, &binding)?;
    Ok(binding)
}

/// Resolve against current work ownership before acquiring or transferring an
/// execution right. Historical binding remains readable after movement.
pub(crate) fn require_current(conn: &Connection, session: &Session) -> Result<SessionWorkstream> {
    let saved = recorded(conn, session.project_id, session.id)?;
    if saved.work_item_id != session.work_item_id.map(|id| id.to_string()) {
        return Err(WorkstreamError::BindingMismatch.into());
    }
    let scope = saved.workstream_id.ok_or(WorkstreamError::ScopeRequired)?;
    require_fresh_catalog(conn, &session.project_id.to_string())?;
    let catalog = recorded_catalog(conn, &session.project_id.to_string())?;
    if let Some(work) = session.work_item_id {
        let (current, revision) = ownership(conn, session.project_id, work)?;
        if current != scope || Some(revision) != saved.ownership_revision {
            return Err(WorkstreamError::BindingMismatch.into());
        }
    }
    if catalog.get(scope)?.state != WorkstreamState::Active {
        return Err(WorkstreamError::Inactive.into());
    }
    Ok(saved)
}

pub(crate) fn require_same(conn: &Connection, project: Id, first: Id, second: Id) -> Result<()> {
    let a = recorded(conn, project, first)?;
    let b = recorded(conn, project, second)?;
    if a.workstream_id.is_none()
        || a.workstream_id != b.workstream_id
        || a.work_item_id != b.work_item_id
        || a.ownership_revision != b.ownership_revision
    {
        return Err(WorkstreamError::BindingMismatch.into());
    }
    Ok(())
}

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let ambiguous: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM claims WHERE status='active' AND released_at IS NULL
        AND (expires_at IS NULL OR expires_at>?1) GROUP BY project_id,work_item_id HAVING count(*)>1)",
        [now_millis()?],|row|row.get(0)).map_err(db_error)?;
    if ambiguous {
        return Err(Error::ClaimConflict(
            "resolve simultaneous claims for one work across branches before upgrading".into(),
        ));
    }
    let sessions = conn
        .prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions ORDER BY project_id,id"
        ))
        .map_err(db_error)?
        .query_map([], session_row)
        .map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_error)?;
    for session in sessions {
        let catalog = recorded_catalog(conn, &session.project_id.to_string())?;
        let owner = session
            .work_item_id
            .map(|work| ownership(conn, session.project_id, work))
            .transpose()?;
        let scope = owner
            .map(|(id, _)| id)
            .or_else(|| (catalog.workstreams.len() == 1).then(|| catalog.workstreams[0].id));
        // Ambiguous workless history retains an explicit unknown scope, never a guessed one.
        let authority = scope
            .map(|id| catalog.get(id).map(|s| s.authority_version))
            .transpose()?;
        save(
            conn,
            &SessionWorkstream {
                project_id: session.project_id.to_string(),
                session_id: session.id.to_string(),
                work_item_id: session.work_item_id.map(|id| id.to_string()),
                workstream_id: scope,
                ownership_revision: owner.map(|(_, revision)| revision),
                authority_version: authority,
            },
        )?;
    }
    Ok(())
}

/// Movement changes current ownership only. Unresolved work must be closed or
/// recovered before a different scope can acquire its execution responsibilities.
pub(crate) fn require_movable(conn: &Connection, project: Id, work: &str) -> Result<()> {
    let busy: bool = conn.query_row("SELECT
        EXISTS(SELECT 1 FROM sessions s WHERE s.project_id=?1 AND s.work_item_id=?2 AND
          (s.status='active' OR (s.status IN ('incomplete','interrupted') AND NOT EXISTS(
            SELECT 1 FROM events e WHERE e.project_id=s.project_id AND
              ((e.event_type='session.resumed' AND json_extract(e.payload_json,'$.from_session_id')=s.id)
               OR (e.event_type='work.handoff' AND e.session_id=s.id AND json_extract(e.payload_json,'$.to_session_id') IS NOT NULL))))))
        OR EXISTS(SELECT 1 FROM claims WHERE project_id=?1 AND work_item_id=?2 AND status='active' AND released_at IS NULL AND (expires_at IS NULL OR expires_at>?3))
        OR EXISTS(SELECT 1 FROM events e JOIN sessions s ON s.project_id=e.project_id AND s.id=e.session_id
          WHERE s.project_id=?1 AND s.work_item_id=?2 AND e.event_type='checkpoint.started'
          AND NOT EXISTS(SELECT 1 FROM events c WHERE c.project_id=e.project_id AND c.event_type IN ('checkpoint.created','checkpoint.abandoned') AND json_extract(c.payload_json,'$.attempt_id')=e.id))",
        params![project.to_string(),work,now_millis()?], |row|row.get(0)).map_err(db_error)?;
    if busy {
        return Err(Error::InvalidTransition(
            "resolve unfinished sessions, claims and pending checkpoint saves before moving work"
                .into(),
        ));
    }
    // Execution records carry their own work identity. Do not infer completion
    // from a released claim, an ended session, or an external success assertion.
    let ids = conn.prepare("SELECT json_extract(payload_json,'$.execution.id') FROM events
        WHERE project_id=?1 AND event_type='execution.registered' AND json_extract(payload_json,'$.execution.work_item_id')=?2")
        .map_err(db_error)?.query_map(params![project.to_string(),work],|row|crate::catalog::id_at(row,0))
        .map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    for id in ids {
        if !crate::execution::at(conn, project, id)?.state.terminal() {
            return Err(Error::InvalidTransition(
                "resolve registered, running or unknown executions before moving work".into(),
            ));
        }
    }
    Ok(())
}

impl Store {
    pub fn session_workstream(&self, project: Id, session: Id) -> Result<SessionWorkstream> {
        recorded(&self.conn, project, session)
    }
    pub fn checkpoint_workstream(&self, project: Id, checkpoint: Id) -> Result<SessionWorkstream> {
        let checkpoint = self.checkpoint(project, checkpoint)?;
        recorded(&self.conn, project, checkpoint.session_id)
    }
    pub fn claim_workstream(&self, project: Id, claim: Id) -> Result<SessionWorkstream> {
        let claim = self.claim(project, claim)?;
        let binding = recorded(&self.conn, project, claim.session_id)?;
        if binding.work_item_id.as_deref() != Some(&claim.work_item_id.to_string()) {
            return Err(WorkstreamError::BindingMismatch.into());
        }
        Ok(binding)
    }
    /// Changes only this client's conversation default. It never rebinds an
    /// existing session or grants permission to a caller.
    pub fn select_conversation_workstream(
        &mut self,
        project: Id,
        expected: Revision,
        binding: McpSessionBinding,
        workstream: Id,
    ) -> Result<((), Event)> {
        binding.validate()?;
        self.runtime_transaction_with_event(project,expected,EventDraft::new("workstream.conversation_selected","Selected conversation workstream"),|tx,_,event| {
            require_fresh_catalog(tx,&project.to_string())?;
            let catalog = recorded_catalog(tx,&project.to_string())?;
            if catalog.get(workstream)?.state != WorkstreamState::Active { return Err(WorkstreamError::Inactive.into()); }
            tx.execute("INSERT INTO conversation_workstreams(project_id,client,conversation,workstream_id,revision) VALUES(?1,?2,?3,?4,1)
                ON CONFLICT(project_id,client,conversation) DO UPDATE SET workstream_id=excluded.workstream_id,revision=conversation_workstreams.revision+1",
                params![project.to_string(),binding.client,binding.conversation,workstream.to_string()]).map_err(db_error)?;
            event.payload=serde_json::json!({"binding":binding,"workstream_id":workstream});
            Ok(())
        })
    }
    pub fn conversation_workstream(
        &self,
        project: Id,
        binding: &McpSessionBinding,
    ) -> Result<Option<Id>> {
        binding.validate()?;
        conversation_default(&self.conn, project, binding)
    }
}
