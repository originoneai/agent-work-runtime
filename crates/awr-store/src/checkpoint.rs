use crate::{
    Store,
    catalog::{id_at, revision_at},
    db_error,
    session::{require_branch, session_at},
    transaction::{optional_id, sqlite_revision},
};
use awr_core::*;
use rusqlite::{OptionalExtension, params};

pub(crate) fn checkpoint_at(
    conn: &rusqlite::Connection,
    project: Id,
    id: Id,
) -> Result<Checkpoint> {
    conn.query_row("SELECT c.id,c.session_id,c.project_revision,c.context_hash,c.digest,c.next_action,c.open_loops_json,c.changed_entities_json,c.revision,c.created_at
            FROM checkpoints c JOIN sessions s ON s.id=c.session_id WHERE s.project_id=?1 AND c.id=?2",params![project.to_string(),id.to_string()],|r| {
                let strings=|column|->rusqlite::Result<Vec<String>> {serde_json::from_str(&r.get::<_,String>(column)?).map_err(|e|rusqlite::Error::FromSqlConversionFailure(column,rusqlite::types::Type::Text,Box::new(e)))};
                Ok(Checkpoint {id:id_at(r,0)?,session_id:id_at(r,1)?,project_revision:revision_at(r,2)?,context_hash:r.get(3)?,digest:r.get(4)?,next_action:r.get(5)?,open_loops:strings(6)?,changed_entities:strings(7)?,revision:revision_at(r,8)?,created_at:r.get(9)?})
            }).optional().map_err(db_error)?.ok_or_else(||Error::NotFound(format!("checkpoint {id}")))
}

impl Store {
    /// Latest checkpoint from a closed session for this work and exact branch.
    pub fn latest_work_checkpoint(
        &self,
        project: Id,
        work: Id,
        branch: Option<Id>,
    ) -> Result<Option<Checkpoint>> {
        let id=self.conn.query_row("SELECT c.id FROM checkpoints c JOIN sessions s ON s.id=c.session_id WHERE s.project_id=?1 AND s.work_item_id=?2 AND s.branch_id IS ?3 AND s.status!='active' ORDER BY c.project_revision DESC,c.created_at DESC,c.id DESC LIMIT 1",params![project.to_string(),work.to_string(),branch.map(|id|id.to_string())],|r|id_at(r,0)).optional().map_err(db_error)?;
        id.map(|id| self.checkpoint(project, id)).transpose()
    }
    pub fn create_checkpoint(
        &mut self,
        project: Id,
        expected: Revision,
        session: Id,
        draft: CheckpointDraft,
    ) -> Result<(Checkpoint, Event)> {
        if !is_sha256_hash(&draft.context_hash)
            || draft.digest.trim().is_empty()
            || draft.next_action.trim().is_empty()
            || draft
                .open_loops
                .iter()
                .chain(&draft.changed_entities)
                .any(|s| s.trim().is_empty())
        {
            return Err(Error::InvalidInput("checkpoint requires a SHA256 context hash, digest, next action and nonblank list entries".into()));
        }
        self.runtime_transaction_with_event(project,expected,EventDraft::new("checkpoint.created","Saved session checkpoint"),|tx,_,event| {
            let current=session_at(tx,project,session)?;
            if current.status!="active" {return Err(Error::InvalidTransition(format!("session {session} is {}",current.status)));}
            require_branch(tx,project,current.branch_id)?;
            let checkpoint=Checkpoint {id:Id::new(),session_id:session,project_revision:expected,context_hash:draft.context_hash.to_ascii_lowercase(),digest:draft.digest,next_action:draft.next_action,open_loops:draft.open_loops,changed_entities:draft.changed_entities,revision:1,created_at:now_millis()?};
            tx.execute("INSERT INTO checkpoints(id,session_id,project_revision,context_hash,digest,next_action,open_loops_json,changed_entities_json,revision,created_at)
                VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9)",params![checkpoint.id.to_string(),session.to_string(),sqlite_revision(expected)?,checkpoint.context_hash,checkpoint.digest,checkpoint.next_action,serde_json::to_string(&checkpoint.open_loops)?,serde_json::to_string(&checkpoint.changed_entities)?,checkpoint.created_at]).map_err(db_error)?;
            tx.execute("UPDATE sessions SET last_checkpoint_id=?1,revision=revision+1 WHERE project_id=?2 AND id=?3",params![checkpoint.id.to_string(),project.to_string(),session.to_string()]).map_err(db_error)?;
            event.session_id=Some(session);event.work_item_id=current.work_item_id;event.branch_id=current.branch_id;
            event.payload=serde_json::json!({"checkpoint_id":checkpoint.id,"context_hash":checkpoint.context_hash,"checkpoint_project_revision":expected,"changed_entities":checkpoint.changed_entities,"next_action":checkpoint.next_action});
            Ok(checkpoint)
        })
    }
    pub fn checkpoint(&self, project: Id, id: Id) -> Result<Checkpoint> {
        checkpoint_at(&self.conn, project, id)
    }
    pub fn latest_checkpoint(&self, project: Id, session: Id) -> Result<Option<Checkpoint>> {
        let current = self.session(project, session)?;
        let checkpoint = current
            .last_checkpoint_id
            .map(|id| self.checkpoint(project, id))
            .transpose()?;
        if checkpoint.as_ref().is_some_and(|c| c.session_id != session) {
            return Err(Error::Storage(
                "latest checkpoint belongs to another session".into(),
            ));
        }
        Ok(checkpoint)
    }
    /// Record metadata for an external artifact. Byte copying and hash verification live in Runtime.
    pub fn record_artifact(
        &mut self,
        project: Id,
        expected: Revision,
        draft: ArtifactDraft,
    ) -> Result<(Artifact, Event)> {
        if [&draft.artifact_type, &draft.locator, &draft.mime]
            .iter()
            .any(|s| s.trim().is_empty())
            || !is_sha256_hash(&draft.sha256)
            || draft.size > i64::MAX as u64
        {
            return Err(Error::InvalidInput(
                "artifact requires type, locator, SHA256, size and MIME".into(),
            ));
        }
        self.runtime_transaction_with_event(project,expected,EventDraft::new("artifact.recorded","Registered external artifact"),|tx,_,event| {
            let source=tx.query_row("SELECT work_item_id,session_id,branch_id FROM events WHERE project_id=?1 AND id=?2",params![project.to_string(),draft.source_event_id.to_string()],|r|Ok((optional_id(r,0)?,optional_id(r,1)?,optional_id(r,2)?))).optional().map_err(db_error)?.ok_or_else(||Error::NotFound(format!("source event {}",draft.source_event_id)))?;
            let artifact=Artifact {id:Id::new(),project_id:project,artifact_type:draft.artifact_type,locator:draft.locator,sha256:draft.sha256.to_ascii_lowercase(),size:draft.size,mime:draft.mime,source_event_id:Some(draft.source_event_id),revision:1};
            tx.execute("INSERT INTO artifacts(id,project_id,artifact_type,locator,sha256,size,mime,source_event_id,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1)",params![artifact.id.to_string(),project.to_string(),artifact.artifact_type,artifact.locator,artifact.sha256,artifact.size as i64,artifact.mime,draft.source_event_id.to_string()]).map_err(db_error)?;
            event.work_item_id=source.0;event.session_id=source.1;event.branch_id=source.2;event.payload=serde_json::json!({"artifact_id":artifact.id,"source_event_id":draft.source_event_id,"sha256":artifact.sha256,"size":artifact.size,"locator":artifact.locator});
            Ok(artifact)
        })
    }
    pub fn artifact(&self, project: Id, id: Id) -> Result<Artifact> {
        self.conn.query_row("SELECT id,project_id,artifact_type,locator,sha256,size,mime,source_event_id,revision FROM artifacts WHERE project_id=?1 AND id=?2",params![project.to_string(),id.to_string()],|r|Ok(Artifact {id:id_at(r,0)?,project_id:id_at(r,1)?,artifact_type:r.get(2)?,locator:r.get(3)?,sha256:r.get(4)?,size:revision_at(r,5)?,mime:r.get(6)?,source_event_id:optional_id(r,7)?,revision:revision_at(r,8)?})).optional().map_err(db_error)?.ok_or_else(||Error::NotFound(format!("artifact {id}")))
    }
}
