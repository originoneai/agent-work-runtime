//! The append-only event stream is the durable client registry. No second state database.
use crate::{
    Store,
    catalog::{id_at, revision_at},
    db_error,
};
use awr_core::{ClientBinding, Error, Event, EventDraft, Id, Result, Revision};
use rusqlite::{Connection, OptionalExtension, params};

fn binding_at(
    conn: &Connection,
    project: Id,
    client: &str,
    external: &str,
) -> Result<Option<ClientBinding>> {
    let value:Option<String>=conn.query_row("SELECT json_extract(payload_json,'$.binding') FROM events WHERE project_id=?1 AND event_type IN ('client.bound','client.updated','client.checkpointed') AND json_extract(payload_json,'$.binding.client')=?2 AND json_extract(payload_json,'$.binding.external_session')=?3 ORDER BY project_revision DESC LIMIT 1",params![project.to_string(),client,external],|row|row.get(0)).optional().map_err(db_error)?;
    value
        .map(|s| {
            serde_json::from_str(&s)
                .map_err(|_| Error::Storage("invalid stored client binding".into()))
        })
        .transpose()
}
impl Store {
    pub fn client_binding(
        &self,
        project: Id,
        client: &str,
        external: &str,
    ) -> Result<Option<ClientBinding>> {
        binding_at(&self.conn, project, client, external)
    }
    pub fn client_delivery(
        &self,
        project: Id,
        client: &str,
        external: &str,
        key: &str,
    ) -> Result<Option<ClientBinding>> {
        let value:Option<String>=self.conn.query_row("SELECT json_extract(payload_json,'$.binding') FROM events WHERE project_id=?1 AND event_type='client.checkpointed' AND json_extract(payload_json,'$.binding.client')=?2 AND json_extract(payload_json,'$.binding.external_session')=?3 AND json_extract(payload_json,'$.binding.last_delivery_key')=?4 ORDER BY project_revision DESC LIMIT 1",params![project.to_string(),client,external,key],|row|row.get(0)).optional().map_err(db_error)?;
        value
            .map(|s| {
                serde_json::from_str(&s)
                    .map_err(|_| Error::Storage("invalid stored client delivery".into()))
            })
            .transpose()
    }
    pub fn client_delivery_checkpoint(
        &self,
        project: Id,
        session: Id,
        key: &str,
    ) -> Result<Option<awr_core::Checkpoint>> {
        if !awr_core::is_sha256_hash(key) {
            return Err(Error::InvalidInput("invalid client delivery key".into()));
        }
        let pattern = format!("[client-delivery:{key}]%");
        let id=self.conn.query_row("SELECT c.id FROM checkpoints c JOIN sessions s ON s.id=c.session_id WHERE s.project_id=?1 AND c.session_id=?2 AND c.digest LIKE ?3 ORDER BY c.created_at DESC LIMIT 1",params![project.to_string(),session.to_string(),pattern],|r|id_at(r,0)).optional().map_err(db_error)?;
        id.map(|id| self.checkpoint(project, id)).transpose()
    }
    pub fn client_work_revision(&self, project: Id, session: Id) -> Result<Revision> {
        self.conn.query_row("SELECT COALESCE(MAX(project_revision),0) FROM events WHERE project_id=?1 AND session_id=?2 AND event_type NOT LIKE 'client.%' AND event_type NOT LIKE 'checkpoint.%'",params![project.to_string(),session.to_string()],|r|revision_at(r,0)).map_err(db_error)
    }
    pub fn save_client_binding(
        &mut self,
        project: Id,
        expected: Revision,
        mut binding: ClientBinding,
        checkpointed: bool,
    ) -> Result<(ClientBinding, Event)> {
        awr_core::ensure_public_data(&binding)?;
        if !["codex", "kimi", "generic"].contains(&binding.client.as_str())
            || binding.external_session.trim().is_empty()
            || binding.external_session.len() > 512
            || binding.digest.len() > 8192
            || binding.next_action.trim().is_empty()
            || binding.next_action.len() > 8192
            || binding.open_loops.len() > 64
            || binding.open_loops.iter().any(|s| s.len() > 8192)
            || binding
                .context_hash
                .as_ref()
                .is_some_and(|v| !awr_core::is_sha256_hash(v))
        {
            return Err(Error::InvalidInput(
                "invalid or oversized client continuity record".into(),
            ));
        }
        let mut event = EventDraft::new("client.updated", "Saved client work continuity");
        event.session_id = Some(binding.session_id);
        self.runtime_transaction_with_event(project,expected,event,|tx,next,event| {
            let old=binding_at(tx,project,&binding.client,&binding.external_session)?;
            if old.as_ref().map(|v|v.revision).unwrap_or(0)!=binding.revision {return Err(Error::SourceConflict("client binding changed; reload before saving".into()));}
            if old.as_ref().is_some_and(|v|v.session_id!=binding.session_id) {return Err(Error::SourceConflict("client conversation is already bound to another AWR session".into()));}
            let session=crate::session::session_at(tx,project,binding.session_id)?;
            if session.work_item_id.is_none() {return Err(Error::InvalidInput("client continuity needs a work-bound session".into()));}
            if let Some(id)=binding.checkpoint_id {
                let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM checkpoints c JOIN sessions s ON s.id=c.session_id WHERE c.id=?1 AND s.project_id=?2 AND c.session_id=?3)",params![id.to_string(),project.to_string(),binding.session_id.to_string()],|r|r.get(0)).map_err(db_error)?;
                if !owned {return Err(Error::InvalidInput("checkpoint belongs to another client session".into()));}
            }
            event.event_type=if old.is_none(){"client.bound"}else if checkpointed{"client.checkpointed"}else{"client.updated"}.into();
            if checkpointed && (binding.checkpoint_id.is_none()||binding.last_delivery_key.as_ref().is_none_or(|s|!awr_core::is_sha256_hash(s))) {return Err(Error::InvalidInput("client checkpoint receipt requires a checkpoint and delivery key".into()));}
            binding.revision=next;binding.observed_at=awr_core::now_millis()?;
            event.payload=serde_json::json!({"binding":binding});
            Ok(binding)
        })
    }
}
