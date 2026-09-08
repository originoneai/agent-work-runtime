use crate::{
    Store, db_error,
    session::{require_active, require_branch, session_at},
    work::readiness,
};
use awr_core::*;
use rusqlite::{Connection, params};

pub(crate) fn validate_action(
    conn: &Connection,
    project: Id,
    revision: Revision,
    patch: &MutationPatch,
    target: &serde_json::Value,
    session: Option<Id>,
) -> Result<()> {
    let Some(binding) = &patch.work_action else {
        return Ok(());
    };
    patch.validate()?;
    let work: WorkItem = serde_json::from_value(target.clone())?;
    if work.status != binding.from {
        return Err(Error::SourceConflict(
            "work state differs from the action's bound starting state".into(),
        ));
    }
    let session = session_at(
        conn,
        project,
        session.ok_or_else(|| {
            Error::InvalidInput("work actions require an explicit work-bound session".into())
        })?,
    )?;
    require_active(&session)?;
    require_branch(conn, project, session.branch_id)?;
    if session.work_item_id != Some(work.meta.id) {
        return Err(Error::InvalidInput(
            "work action session must be bound to the exact target work".into(),
        ));
    }
    let at = now_millis()?;
    // Source state is shared within a project, so a live claim on another branch also
    // blocks these source mutations. Source owner remains business metadata.
    let foreign:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM claims WHERE project_id=?1 AND work_item_id=?2 AND status='active' AND released_at IS NULL AND (expires_at IS NULL OR expires_at>?3) AND session_id!=?4)",params![project.to_string(),work.meta.id.to_string(),at,session.id.to_string()],|r|r.get(0)).map_err(db_error)?;
    if foreign {
        return Err(Error::ClaimConflict(
            "another session holds this work; release or hand off that runtime claim first".into(),
        ));
    }
    if binding.action.needs_claim() {
        let owned:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM claims WHERE project_id=?1 AND work_item_id=?2 AND status='active' AND released_at IS NULL AND (expires_at IS NULL OR expires_at>?3) AND session_id=?4 AND branch_id IS ?5 AND agent_id=?6)",params![project.to_string(),work.meta.id.to_string(),at,session.id.to_string(),session.branch_id.map(|id|id.to_string()),session.agent_id],|r|r.get(0)).map_err(db_error)?;
        if !owned {
            return Err(Error::ClaimConflict(
                "this work action requires an unexpired runtime claim held by its creating session"
                    .into(),
            ));
        }
    }
    if binding.action.needs_dependencies() {
        let ready = readiness(
            conn,
            project,
            &work.meta.external_key,
            session.branch_id,
            at,
            revision,
        )?;
        let problems = ready
            .diagnostics
            .iter()
            .filter(|d| {
                !matches!(d.code.as_str(), "status_not_selectable" | "active_claim")
                    && !(binding.action == WorkAction::Unblock
                        && d.code == "active_blocker"
                        && d.work_item_key == work.meta.external_key)
            })
            .collect::<Vec<_>>();
        if problems.iter().any(|d| d.code == "source_not_fresh") {
            return Err(Error::SourceStale(
                "work or a required dependency source is not fresh".into(),
            ));
        }
        if !problems.is_empty() {
            return Err(Error::DependencyBlocked(serde_json::to_string(&problems)?));
        }
    }
    Ok(())
}

/// Cancellation releases only the creating session's occupancy; the source owner is
/// untouched. The release IDs are recorded in the same verified work action receipt.
pub(crate) fn release_cancelled_claims(
    conn: &Connection,
    project: Id,
    proposal: &MutationProposal,
) -> Result<Vec<Id>> {
    let Some(binding) = proposal.bound_patch()?.work_action else {
        return Ok(Vec::new());
    };
    if binding.action != WorkAction::Cancel {
        return Ok(Vec::new());
    }
    let ids=conn.prepare("SELECT id FROM claims WHERE project_id=?1 AND work_item_id=?2 AND session_id=?3 AND status='active'").map_err(db_error)?
        .query_map(params![project.to_string(),proposal.work_item_id.map(|id|id.to_string()),proposal.created_by_session.map(|id|id.to_string())],|r|crate::catalog::id_at(r,0)).map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    conn.execute("UPDATE claims SET status='released',released_at=?1,revision=revision+1 WHERE project_id=?2 AND work_item_id=?3 AND session_id=?4 AND status='active'",params![now_millis()?,project.to_string(),proposal.work_item_id.map(|id|id.to_string()),proposal.created_by_session.map(|id|id.to_string())]).map_err(db_error)?;
    Ok(ids)
}

impl Store {
    /// Read current domain constraints without refreshing source files or mutating state.
    pub fn check_work_proposal(&self, project: Id, proposal: &MutationProposal) -> Result<()> {
        let patch = proposal.bound_patch()?;
        let target = crate::mutation::validate_binding(
            &self.conn,
            project,
            proposal.source_id,
            &proposal.base_fingerprint,
            &proposal.mutation_type,
            &patch,
        )?;
        validate_action(
            &self.conn,
            project,
            self.project(project)?.project_revision,
            &patch,
            &target.item,
            proposal.created_by_session,
        )
    }
}
