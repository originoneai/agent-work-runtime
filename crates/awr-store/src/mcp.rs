//! MCP identities are indexed from atomic session events, not a second database.
use crate::{Store, catalog::id_at, db_error, session::session_at};
use awr_core::*;
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) fn binding_session(
    conn: &Connection,
    project: Id,
    binding: &McpSessionBinding,
) -> Result<Option<Session>> {
    let id = conn.query_row("SELECT session_id FROM events WHERE project_id=?1 AND event_type IN ('session.started','session.resumed') AND json_extract(payload_json,'$.mcp_binding.client')=?2 AND json_extract(payload_json,'$.mcp_binding.conversation')=?3 ORDER BY project_revision DESC,id DESC LIMIT 1",
        params![project.to_string(),binding.client,binding.conversation], |row|id_at(row,0)).optional().map_err(db_error)?;
    id.map(|id| session_at(conn, project, id)).transpose()
}
pub(crate) fn session_binding(
    conn: &Connection,
    project: Id,
    session: Id,
) -> Result<Option<McpSessionBinding>> {
    let value: Option<String> = conn.query_row("SELECT json_extract(payload_json,'$.mcp_binding') FROM events WHERE project_id=?1 AND session_id=?2 AND event_type IN ('session.started','session.resumed') ORDER BY project_revision DESC,id DESC LIMIT 1",
        params![project.to_string(),session.to_string()], |row|row.get(0)).optional().map_err(db_error)?.flatten();
    value
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|_| Error::Storage("invalid stored MCP binding".into()))
        })
        .transpose()
}
impl Store {
    pub fn mcp_bound_session(
        &self,
        project: Id,
        binding: &McpSessionBinding,
    ) -> Result<Option<Session>> {
        binding.validate()?;
        binding_session(&self.conn, project, binding)
    }
    pub fn mcp_session_binding(
        &self,
        project: Id,
        session: Id,
    ) -> Result<Option<McpSessionBinding>> {
        self.session(project, session)?;
        session_binding(&self.conn, project, session)
    }
    pub fn mcp_sessions(
        &self,
        project: Id,
        client: &str,
        limit: usize,
        before: Option<Revision>,
    ) -> Result<Vec<Session>> {
        if !(1..=100).contains(&limit) {
            return Err(Error::InvalidInput("session limit must be 1..100".into()));
        }
        let ids = self.conn.prepare("SELECT session_id FROM events WHERE project_id=?1 AND event_type IN ('session.started','session.resumed') AND json_extract(payload_json,'$.mcp_binding.client')=?2 AND (?3 IS NULL OR project_revision<?3) ORDER BY project_revision DESC,id DESC LIMIT ?4")
            .map_err(db_error)?.query_map(params![project.to_string(),client,before.map(crate::transaction::sqlite_revision).transpose()?,limit as i64],|row|id_at(row,0)).map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
        ids.into_iter()
            .map(|id| self.session(project, id))
            .collect()
    }
}
