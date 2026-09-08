use crate::{
    Store,
    catalog::{id_at, revision_at},
    db_error,
    session::require_branch,
    transaction::{optional_id, sqlite_revision},
};
use awr_core::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

const COLUMNS: &str =
    "id,project_id,name,parent_branch_id,git_ref,fork_project_revision,status,revision";
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Branch> {
    Ok(Branch {
        id: id_at(r, 0)?,
        project_id: id_at(r, 1)?,
        name: r.get(2)?,
        parent_branch_id: optional_id(r, 3)?,
        git_ref: r.get(4)?,
        fork_project_revision: revision_at(r, 5)?,
        status: r.get(6)?,
        revision: revision_at(r, 7)?,
    })
}
fn branch_at(conn: &Connection, project: Id, id: Id) -> Result<Branch> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM branches WHERE project_id=?1 AND id=?2"),
        params![project.to_string(), id.to_string()],
        row,
    )
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| Error::NotFound(format!("work branch {id}")))
}
fn current_at(conn: &Connection, project: Id) -> Result<Option<Id>> {
    conn.query_row(
        "SELECT current_branch_id FROM projects WHERE id=?1",
        [project.to_string()],
        |r| optional_id(r, 0),
    )
    .map_err(db_error)
}
fn actor_reason(actor: &str, reason: &str) -> Result<()> {
    if actor.trim().is_empty()
        || actor.len() > 256
        || reason.trim().is_empty()
        || reason.len() > 4096
    {
        return Err(Error::InvalidInput(
            "branch mutation requires actor (1..256 bytes) and reason (1..4096 bytes)".into(),
        ));
    }
    Ok(())
}
fn record_at(conn: &Connection, project: Id, id: Id) -> Result<BranchRecord> {
    let branch = branch_at(conn, project, id)?;
    let mut receipts=conn.prepare("SELECT id,payload_json FROM events WHERE project_id=?1 AND branch_id=?2 AND event_type='branch.created' ORDER BY project_revision LIMIT 2").map_err(db_error)?
        .query_map(params![project.to_string(),id.to_string()],|r|Ok((id_at(r,0)?,r.get::<_,String>(1)?))).map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    if receipts.len() > 1 {
        return Err(Error::SourceConflict(
            "branch has multiple creation receipts".into(),
        ));
    }
    let (creation_event_id, git_binding) = if let Some((event, payload)) = receipts.pop() {
        let payload: serde_json::Value = serde_json::from_str(&payload)?;
        let binding: Option<GitRefBinding> =
            serde_json::from_value(payload["git_binding"].clone())?;
        if let Some(binding) = &binding {
            binding.validate()?;
        }
        if payload["name"] != json!(branch.name)
            || payload["branch_id"] != json!(branch.id)
            || payload["fork_project_revision"] != json!(branch.fork_project_revision)
            || payload["parent_branch_id"] != json!(branch.parent_branch_id)
            || branch.git_ref.as_deref() != binding.as_ref().map(|b| b.requested_ref.as_str())
        {
            return Err(Error::SourceConflict(
                "branch row disagrees with its immutable creation receipt".into(),
            ));
        }
        (Some(event), binding)
    } else {
        (None, None)
    };
    Ok(BranchRecord {
        branch,
        git_binding,
        creation_event_id,
    })
}
fn require_usable(
    conn: &Connection,
    project: Id,
    id: Option<Id>,
    revision: Revision,
) -> Result<()> {
    require_branch(conn, project, id)?;
    if let Some(id) = id {
        let branch = record_at(conn, project, id)?.branch;
        if branch.fork_project_revision > revision || branch.parent_branch_id == Some(id) {
            return Err(Error::SourceConflict(
                "work branch has an invalid fork revision or parent".into(),
            ));
        }
    }
    Ok(())
}
impl Store {
    pub fn branch(&self, project: Id, id: Id) -> Result<BranchRecord> {
        record_at(&self.conn, project, id)
    }
    /// main is the existing None-bound baseline. Never relabel historical runtime rows.
    pub fn resolve_branch(&self, project: Id, reference: &str) -> Result<Option<Id>> {
        self.project(project)?;
        if reference == "main" {
            return Ok(None);
        }
        if reference.trim().is_empty() {
            return Err(Error::InvalidInput("branch selector is empty".into()));
        }
        let ids = self
            .conn
            .prepare("SELECT id FROM branches WHERE project_id=?1 AND (name=?2 OR id=?2) LIMIT 2")
            .map_err(db_error)?
            .query_map(params![project.to_string(), reference], |r| id_at(r, 0))
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?;
        match ids.as_slice() {
            [id] => Ok(Some(*id)),
            [] => Err(Error::NotFound(format!("work branch {reference}"))),
            _ => Err(Error::InvalidInput(
                "branch ID conflicts with another branch name; use an unambiguous name".into(),
            )),
        }
    }
    pub fn branches(
        &self,
        project: Id,
        status: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<BranchPage> {
        if limit == 0
            || limit > 1000
            || offset > i64::MAX as usize
            || status.is_some_and(|s| !matches!(s, "active" | "merged" | "abandoned"))
        {
            return Err(Error::InvalidInput(
                "branch list needs a valid status, offset and limit of 1..1000".into(),
            ));
        }
        let tx = self.conn.unchecked_transaction().map_err(db_error)?;
        let (project_revision, current_branch_id) = tx
            .query_row(
                "SELECT project_revision,current_branch_id FROM projects WHERE id=?1",
                [project.to_string()],
                |r| Ok((revision_at(r, 0)?, optional_id(r, 1)?)),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| Error::NotFound(format!("project {project}")))?;
        let total = tx
            .query_row(
                "SELECT count(*) FROM branches WHERE project_id=?1 AND (?2 IS NULL OR status=?2)",
                params![project.to_string(), status],
                |r| r.get::<_, i64>(0),
            )
            .map_err(db_error)? as usize;
        let branches=tx.prepare(&format!("SELECT {COLUMNS} FROM branches WHERE project_id=?1 AND (?2 IS NULL OR status=?2) ORDER BY name,id LIMIT ?3 OFFSET ?4")).map_err(db_error)?
            .query_map(params![project.to_string(),status,limit as i64,offset as i64],row).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
        let has_more = offset.saturating_add(branches.len()) < total;
        tx.commit().map_err(db_error)?;
        Ok(BranchPage {
            project_id: project,
            project_revision,
            current_branch_id,
            branches,
            total,
            offset,
            limit,
            has_more,
        })
    }
    /// Fork only runtime identity at the exact current project revision. Source/Git are shared.
    pub fn create_branch(
        &mut self,
        project: Id,
        expected: Revision,
        draft: BranchDraft,
    ) -> Result<(Branch, Event)> {
        validate_branch_name(&draft.name)?;
        actor_reason(&draft.actor, &draft.reason)?;
        if let Some(binding) = &draft.git_binding {
            binding.validate()?;
            if binding.observed_at > now_millis()? {
                return Err(Error::InvalidInput(
                    "Git binding observation is in the future".into(),
                ));
            }
        }
        self.runtime_transaction_with_event(project,expected,EventDraft::new("branch.created","Created agent work branch"),|tx,_,event| {
            require_usable(tx,project,draft.parent_branch_id,expected)?;
            let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM branches WHERE project_id=?1 AND name=?2)",params![project.to_string(),draft.name],|r|r.get(0)).map_err(db_error)?;
            if exists {return Err(Error::InvalidInput("work branch name already exists in this project".into()));}
            let branch=Branch{id:Id::new(),project_id:project,name:draft.name,parent_branch_id:draft.parent_branch_id,git_ref:draft.git_binding.as_ref().map(|b|b.requested_ref.clone()),fork_project_revision:expected,status:"active".into(),revision:1};
            tx.execute("INSERT INTO branches(id,project_id,name,parent_branch_id,git_ref,fork_project_revision,status,revision) VALUES(?1,?2,?3,?4,?5,?6,'active',1)",params![branch.id.to_string(),project.to_string(),branch.name,branch.parent_branch_id.map(|id|id.to_string()),branch.git_ref,sqlite_revision(expected)?]).map_err(db_error)?;
            event.branch_id=Some(branch.id);event.summary=format!("Created work branch {}",branch.name);
            event.payload=json!({"branch_id":branch.id,"name":branch.name,"parent_branch_id":branch.parent_branch_id,"fork_project_revision":expected,"git_binding":draft.git_binding,"actor":draft.actor,"reason":draft.reason,"source_copied":false,"git_write_performed":false});
            Ok(branch)
        })
    }
    /// Change only the default selection; existing sessions/claims/evidence retain their branch.
    pub fn switch_branch(
        &mut self,
        project: Id,
        expected: Revision,
        branch: Option<Id>,
        actor: &str,
        reason: &str,
    ) -> Result<(BranchSwitched, Event)> {
        actor_reason(actor, reason)?;
        self.runtime_transaction_with_event(project,expected,EventDraft::new("branch.switched","Changed current work branch"),|tx,_,event| {
            require_usable(tx,project,branch,expected)?;
            let previous=current_at(tx,project)?;
            if previous==branch {return Err(Error::InvalidTransition("work branch is already selected".into()));}
            let at=now_millis()?;
            let sessions=tx.query_row("SELECT count(*) FROM sessions WHERE project_id=?1 AND branch_id IS ?2 AND status='active'",params![project.to_string(),previous.map(|id|id.to_string())],|r|r.get::<_,i64>(0)).map_err(db_error)? as usize;
            let claims=tx.query_row("SELECT count(*) FROM claims WHERE project_id=?1 AND branch_id IS ?2 AND status='active' AND released_at IS NULL AND (expires_at IS NULL OR expires_at>?3)",params![project.to_string(),previous.map(|id|id.to_string()),at],|r|r.get::<_,i64>(0)).map_err(db_error)? as usize;
            tx.execute("UPDATE projects SET current_branch_id=?1 WHERE id=?2",params![branch.map(|id|id.to_string()),project.to_string()]).map_err(db_error)?;
            let result=BranchSwitched{previous_branch_id:previous,current_branch_id:branch,retained_active_sessions:sessions,retained_active_claims:claims};
            event.branch_id=branch;event.payload=json!({"selection":result,"actor":actor,"reason":reason,"git_write_performed":false,"source_write_performed":false});
            Ok(result)
        })
    }
}
