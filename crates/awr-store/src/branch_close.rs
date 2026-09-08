use crate::{
    Store,
    branch::{actor_reason, branch_at, current_at, require_usable},
    catalog::{id_at, revision_at},
    checkpoint::checkpoint_at,
    db_error,
    query::projection,
    session::{CLAIM_COLUMNS, SESSION_COLUMNS, claim_row, session_row},
    transaction::{insert_event, sqlite_revision},
};
use awr_core::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use std::collections::BTreeMap;

pub(crate) fn closure_at(
    conn: &Connection,
    project: Id,
    branch: &Branch,
) -> Result<Option<BranchClosureRecord>> {
    let records = conn.prepare("SELECT id,project_revision,payload_json FROM events WHERE project_id=?1 AND branch_id=?2 AND event_type='branch.closed' ORDER BY project_revision LIMIT 2").map_err(db_error)?
        .query_map(params![project.to_string(),branch.id.to_string()], |r| Ok((id_at(r,0)?,revision_at(r,1)?,r.get::<_,String>(2)?))).map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    if records.len() > 1 {
        return Err(Error::SourceConflict(
            "branch has multiple closure receipts".into(),
        ));
    }
    let Some((event_id, project_revision, payload)) = records.into_iter().next() else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(&payload)?;
    let receipt: BranchCloseReceipt = serde_json::from_value(value["closure"].clone())?;
    if receipt.version != 1
        || receipt.branch_id != branch.id
        || receipt.branch_revision != branch.revision
        || receipt.outcome.as_str() != branch.status
        || receipt.source_project_revision.checked_add(1) != Some(project_revision)
    {
        return Err(Error::SourceConflict(
            "branch row disagrees with its closure receipt".into(),
        ));
    }
    let summary: Option<(String,Revision,String)>=conn.query_row("SELECT event_type,project_revision,payload_json FROM events WHERE project_id=?1 AND id=?2", params![project.to_string(),receipt.summary_event_id.to_string()], |r| Ok((r.get(0)?,revision_at(r,1)?,r.get(2)?))).optional().map_err(db_error)?;
    let expected_type = if receipt.outcome == BranchCloseOutcome::Merged {
        "branch.merged"
    } else {
        "branch.abandoned"
    };
    if !summary.is_some_and(|(kind, revision, payload)| {
        kind == expected_type
            && revision == project_revision
            && serde_json::from_str::<serde_json::Value>(&payload).ok() == Some(value)
    }) {
        return Err(Error::SourceConflict(
            "branch closure has no matching immutable summary".into(),
        ));
    }
    Ok(Some(BranchClosureRecord {
        event_id,
        project_revision,
        receipt,
    }))
}

fn ids(conn: &Connection, sql: &str, project: Id, branch: Id) -> Result<Vec<Id>> {
    let found = conn
        .prepare(sql)
        .map_err(db_error)?
        .query_map(params![project.to_string(), branch.to_string()], |r| {
            id_at(r, 0)
        })
        .map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_error)?;
    if found.len() > 1000 {
        return Err(Error::InvalidInput("branch close inventory exceeds 1000 records; inspect and settle the outstanding work first".into()));
    }
    Ok(found)
}
fn plan_at(
    conn: &Connection,
    project: Id,
    branch: Id,
    revision: Revision,
) -> Result<BranchClosePlan> {
    let row = branch_at(conn, project, branch)?;
    let sessions=conn.prepare(&format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE project_id=?1 AND branch_id=?2 ORDER BY id LIMIT 1001")).map_err(db_error)?
        .query_map(params![project.to_string(),branch.to_string()],session_row).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    let claims=conn.prepare(&format!("SELECT {CLAIM_COLUMNS} FROM claims WHERE project_id=?1 AND branch_id=?2 AND status='active' ORDER BY id LIMIT 1001")).map_err(db_error)?
        .query_map(params![project.to_string(),branch.to_string()],claim_row).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    if sessions.len() > 1000 || claims.len() > 1000 {
        return Err(Error::InvalidInput(
            "branch close inventory exceeds 1000 sessions or claims".into(),
        ));
    }
    let mut open_loops = Vec::new();
    for session in &sessions {
        let newest=conn.query_row("SELECT id FROM checkpoints WHERE session_id=?1 ORDER BY project_revision DESC,created_at DESC,id DESC LIMIT 1",[session.id.to_string()],|r|id_at(r,0)).optional().map_err(db_error)?;
        if newest != session.last_checkpoint_id {
            return Err(Error::SourceConflict("branch session checkpoint pointer is inconsistent; inspect retained checkpoints before closing".into()));
        }
        if let Some(id) = newest {
            let cp = checkpoint_at(conn, project, id)?;
            if cp.session_id != session.id
                || cp.project_revision < row.fork_project_revision
                || cp.project_revision > revision
            {
                return Err(Error::SourceConflict(
                    "branch checkpoint ownership or revision is inconsistent".into(),
                ));
            }
            open_loops.extend(cp.open_loops.into_iter().enumerate().map(|(index, text)| {
                BranchOpenLoop {
                    checkpoint_id: id,
                    checkpoint_revision: cp.revision,
                    session_id: session.id,
                    work_item_id: session.work_item_id,
                    index,
                    text,
                }
            }));
        }
    }
    if open_loops.len() > 1000 {
        return Err(Error::InvalidInput(
            "branch has more than 1000 current checkpoint open loops".into(),
        ));
    }
    let pending_checkpoint_attempts=ids(conn,"SELECT e.id FROM events e JOIN sessions s ON s.project_id=e.project_id AND s.id=e.session_id WHERE e.project_id=?1 AND s.branch_id=?2 AND e.event_type='checkpoint.started'
        AND NOT EXISTS(SELECT 1 FROM events c WHERE c.project_id=e.project_id AND c.event_type IN ('checkpoint.created','checkpoint.abandoned') AND json_extract(c.payload_json,'$.attempt_id')=e.id) ORDER BY e.id LIMIT 1001",project,branch)?;
    let pending_proposals = ids(
        conn,
        "SELECT p.id FROM mutation_proposals p JOIN sessions s ON s.project_id=p.project_id AND s.id=p.created_by_session WHERE p.project_id=?1 AND s.branch_id=?2 AND p.status NOT IN ('applied','rejected','failed','conflict') ORDER BY p.id LIMIT 1001",
        project,
        branch,
    )?;
    let mut blockers = Vec::new();
    if row.status != "active" {
        blockers.push(format!("branch is {}", row.status));
    }
    if sessions.iter().any(|s| s.status == "active") {
        blockers.push("end or hand off active sessions explicitly before closing".into());
    }
    if !claims.is_empty() {
        blockers.push("release or expire all active-status claims through their owning sessions before closing".into());
    }
    if !pending_checkpoint_attempts.is_empty() {
        blockers.push("finish or explicitly abandon pending checkpoint saves".into());
    }
    if !pending_proposals.is_empty() {
        blockers.push("apply or explicitly reject outstanding source proposals".into());
    }
    Ok(BranchClosePlan {project_id:project,project_revision:revision,current_branch_id:current_at(conn,project)?,branch:row,sessions,unsettled_claims:claims,open_loops,pending_checkpoint_attempts,pending_proposals,blockers,
        history_scope:"Open loops from each branch session's latest successful checkpoint; older checkpoints remain historical and are not inferred resolved. Every listed loop requires an exact disposition. Closing does not complete source work items.".into()})
}

fn source_versions(conn: &Connection, project: Id) -> Result<Vec<BranchSourceVersion>> {
    let rows=conn.prepare("SELECT id,revision,fingerprint,locator,freshness FROM sources WHERE project_id=?1 AND active=1 ORDER BY id").map_err(db_error)?
        .query_map([project.to_string()],|r|Ok((BranchSourceVersion {source_id:id_at(r,0)?,revision:revision_at(r,1)?,fingerprint:r.get(2)?,locator:r.get(3)?},r.get::<_,String>(4)?))).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    if rows.is_empty() || rows.iter().any(|(_, freshness)| freshness != "fresh") {
        return Err(Error::SourceStale(
            "branch closure requires refreshed source projections".into(),
        ));
    }
    Ok(rows.into_iter().map(|(s, _)| s).collect())
}

fn validate_merge(
    input: &Option<BranchMergeInput>,
    observed: &Option<BranchMergeObservation>,
) -> Result<()> {
    let valid = match (input, observed) {
        (None, None) => true,
        (
            Some(BranchMergeInput::Git {
                source_ref,
                target_ref,
            }),
            Some(BranchMergeObservation::Git {
                source,
                target,
                head_sha,
            }),
        ) => {
            source.validate()?;
            target.validate()?;
            source.requested_ref == *source_ref
                && target.requested_ref == *target_ref
                && source.repository_root == target.repository_root
                && head_sha == &target.commit_sha
                && source.observed_at <= now_millis()?
                && target.observed_at <= now_millis()?
        }
        (
            Some(BranchMergeInput::Source { locator, sha256 }),
            Some(BranchMergeObservation::Source {
                locator: seen,
                sha256: hash,
                size,
                observed_at,
            }),
        ) => {
            locator == seen
                && sha256.eq_ignore_ascii_case(hash)
                && is_sha256_hash(hash)
                && *size > 0
                && *size <= 1048576
                && *observed_at >= 0
                && *observed_at <= now_millis()?
        }
        _ => false,
    };
    if !valid {
        return Err(Error::InvalidInput(
            "merge observation does not match the requested closure evidence".into(),
        ));
    }
    Ok(())
}

impl Store {
    /// One read-only snapshot; no source refresh, claim cleanup or implicit loop resolution.
    pub fn branch_close_plan(&self, project: Id, branch: Id) -> Result<BranchClosePlan> {
        let tx = self.conn.unchecked_transaction().map_err(db_error)?;
        let revision = tx
            .query_row(
                "SELECT project_revision FROM projects WHERE id=?1",
                [project.to_string()],
                |r| revision_at(r, 0),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| Error::NotFound(format!("project {project}")))?;
        let plan = plan_at(&tx, project, branch, revision)?;
        closure_at(&tx, project, &plan.branch)?;
        tx.commit().map_err(db_error)?;
        Ok(plan)
    }
    /// The Runtime verifies physical Source/Git observations; Store binds that snapshot and
    /// atomically records the summary, explicit carry-forward receipts and final branch state.
    pub fn close_branch(
        &mut self,
        project: Id,
        branch: Id,
        expected: Revision,
        draft: CloseBranchDraft,
    ) -> Result<(BranchClosed, Event)> {
        draft.input.validate()?;
        actor_reason(&draft.actor, &draft.reason)?;
        validate_merge(&draft.input.merge, &draft.merge)?;
        if draft.target_branch_id == Some(branch) {
            return Err(Error::InvalidInput(
                "branch cannot close into itself".into(),
            ));
        }
        self.runtime_transaction_with_event(project,expected,EventDraft::new("branch.closed","Closed agent work branch"),|tx,next,event| {
            require_usable(tx,project,Some(branch),expected)?;require_usable(tx,project,draft.target_branch_id,expected)?;
            let plan=plan_at(tx,project,branch,expected)?;
            if !plan.blockers.is_empty() { return Err(Error::InvalidTransition(plan.blockers.join("; "))); }
            let versions=source_versions(tx,project)?;
            let mut supplied=draft.source_versions;supplied.sort_by_key(|s|s.source_id);
            if supplied!=versions { return Err(Error::SourceConflict("source versions changed before branch closure".into())); }
            let mut dispositions=BTreeMap::new();
            for item in draft.input.open_loops {
                if dispositions.insert((item.checkpoint_id,item.index),item).is_some() { return Err(Error::InvalidInput("duplicate open-loop disposition".into())); }
            }
            let mut loops=Vec::new();let at=now_millis()?;
            for original in plan.open_loops {
                let item=dispositions.remove(&(original.checkpoint_id,original.index)).ok_or_else(||Error::InvalidInput(format!("missing disposition for checkpoint {} loop {}",original.checkpoint_id,original.index)))?;
                if item.text!=original.text { return Err(Error::SourceConflict("open-loop text changed or does not match the selected checkpoint".into())); }
                let (carried_to,carry_event_id)=if let BranchLoopResolution::CarryForward {work_item_key,reason}=&item.resolution {
                    let work:Projected<WorkItem>=projection(tx,project,EntityKind::WorkItem,work_item_key)?;
                    if work.source.freshness!=Freshness::Fresh || matches!(work.item.status,WorkStatus::Completed|WorkStatus::Cancelled|WorkStatus::Unknown) { return Err(Error::InvalidInput("carry-forward work must be current and nonterminal".into())); }
                    let id=Id::new();
                    insert_event(tx,&Event {id,project_id:project,work_item_id:Some(work.item.meta.id),session_id:None,branch_id:draft.target_branch_id,event_type:"branch.loop_carried".into(),importance:"high".into(),summary:format!("Carry forward from {}: {}",plan.branch.name,original.text),payload:json!({"from_branch_id":branch,"original":original,"target_work":work.item.meta,"reason":reason}),project_revision:next,created_at:at})?;
                    (Some(work.item.meta),Some(id))
                } else {(None,None)};
                loops.push(BranchLoopReceipt {original,resolution:item.resolution,carried_to,carry_event_id});
            }
            if !dispositions.is_empty() { return Err(Error::InvalidInput("disposition references a foreign or superseded checkpoint loop".into())); }
            let current=if plan.current_branch_id==Some(branch) {draft.target_branch_id} else {plan.current_branch_id};
            let mut row=plan.branch;row.status=draft.input.outcome.as_str().into();row.revision=row.revision.checked_add(1).ok_or_else(||Error::InvalidInput("branch revision overflow".into()))?;
            let receipt=BranchCloseReceipt {version:1,branch_id:branch,branch_revision:row.revision,outcome:draft.input.outcome,target_branch_id:draft.target_branch_id,actor:draft.actor,reason:draft.reason,summary:draft.input.summary,source_project_revision:expected,source_versions:versions,merge:draft.merge,open_loops:loops,retained_session_ids:plan.sessions.iter().map(|s|s.id).collect(),previous_branch_id:plan.current_branch_id,current_branch_id:current,summary_event_id:Id::new()};
            let payload=json!({"closure":receipt});
            insert_event(tx,&Event {id:receipt.summary_event_id,project_id:project,work_item_id:None,session_id:None,branch_id:if receipt.outcome==BranchCloseOutcome::Merged {receipt.target_branch_id} else {Some(branch)},event_type:format!("branch.{}",receipt.outcome.as_str()),importance:"high".into(),summary:format!("{} work branch {}: {}",receipt.outcome.as_str(),row.name,receipt.summary),payload:payload.clone(),project_revision:next,created_at:at})?;
            tx.execute("UPDATE branches SET status=?1,revision=?2 WHERE project_id=?3 AND id=?4",params![row.status,sqlite_revision(row.revision)?,project.to_string(),branch.to_string()]).map_err(db_error)?;
            tx.execute("UPDATE projects SET current_branch_id=?1 WHERE id=?2",params![current.map(|id|id.to_string()),project.to_string()]).map_err(db_error)?;
            event.branch_id=Some(branch);event.summary=format!("Closed work branch {} ({})",row.name,row.status);event.payload=payload;
            Ok(BranchClosed {branch:row,receipt})
        })
    }
}
