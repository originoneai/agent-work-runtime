use awr_core::{Checkpoint, Error, Freshness, Id, Result, Revision, Session};
use awr_store::{DeltaEvents, Store};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeltaBaseline {
    #[default]
    Auto,
    Checkpoint {
        id: Id,
    },
    Revision {
        revision: Revision,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaRequest {
    pub baseline: DeltaBaseline,
    pub session_id: Option<Id>,
    pub event_limit: usize,
    pub entity_limit_per_source: usize,
}
impl Default for DeltaRequest {
    fn default() -> Self {
        Self {
            baseline: DeltaBaseline::Auto,
            session_id: None,
            event_limit: 12,
            entity_limit_per_source: 24,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentDelta {
    pub project_id: Id,
    pub work_item_id: Id,
    pub work_item_key: String,
    pub branch_id: Option<Id>,
    pub after_revision: Revision,
    pub baseline_origin: String,
    pub checkpoint_id: Option<Id>,
    pub events: DeltaEvents,
    pub gaps: Vec<String>,
    pub history_scope: String,
}

pub(crate) fn context_checkpoint(
    store: &Store,
    project: Id,
    work: Id,
    branch: Option<Id>,
    session: Option<&Session>,
) -> Result<(Option<Checkpoint>, &'static str)> {
    if let Some(session) = session {
        let own = store.latest_checkpoint(project, session.id)?;
        let inherited = store.incoming_handoff(project, session.id)?;
        if let Some(cp) = own
            .into_iter()
            .chain(inherited)
            .max_by_key(|c| (c.project_revision, c.created_at, c.id))
        {
            let origin = if cp.session_id == session.id {
                "session"
            } else {
                "handoff"
            };
            return Ok((Some(cp), origin));
        }
    }
    let cp = store.latest_work_checkpoint(project, work, branch)?;
    let origin = if cp.is_some() {
        "previous_closed_session"
    } else {
        "none"
    };
    Ok((cp, origin))
}

/// Snapshot API: refresh Source through the caller before requesting execution context.
pub fn recent_delta(
    store: &Store,
    project_id: Id,
    work_key: &str,
    branch: Option<Id>,
    request: &DeltaRequest,
) -> Result<RecentDelta> {
    let project = store.project(project_id)?;
    if project.current_branch_id != branch {
        return Err(Error::Unsupported(
            "delta requires the selected project branch; overlays are not yet implemented".into(),
        ));
    }
    let work = store.work_item(project_id, work_key)?;
    let session = request
        .session_id
        .map(|id| store.session(project_id, id))
        .transpose()?;
    if session
        .as_ref()
        .is_some_and(|s| s.work_item_id != Some(work.item.meta.id) || s.branch_id != branch)
    {
        return Err(Error::InvalidInput(
            "delta session belongs to a different work item or branch".into(),
        ));
    }
    let (checkpoint, mut origin) = match request.baseline {
        DeltaBaseline::Checkpoint { id } => (
            Some(store.checkpoint(project_id, id)?),
            "explicit_checkpoint",
        ),
        DeltaBaseline::Revision { .. } => (None, "explicit_revision"),
        DeltaBaseline::Auto => context_checkpoint(
            store,
            project_id,
            work.item.meta.id,
            branch,
            session.as_ref(),
        )?,
    };
    if let Some(cp) = &checkpoint {
        let owner = store.session(project_id, cp.session_id)?;
        if owner.work_item_id != Some(work.item.meta.id) || owner.branch_id != branch {
            return Err(Error::InvalidInput(
                "delta checkpoint belongs to a different work item or branch".into(),
            ));
        }
    }
    let after_revision = match (&checkpoint, &request.baseline) {
        (Some(cp), _) => cp.project_revision,
        (_, DeltaBaseline::Revision { revision }) => *revision,
        _ => {
            if let Some(session) = &session {
                origin = "session_start";
                session.start_project_revision
            } else {
                origin = "project_start";
                0
            }
        }
    };
    let events = store.delta_events(
        project_id,
        project.project_revision,
        work.item.meta.id,
        branch,
        after_revision,
        request.event_limit,
        request.entity_limit_per_source,
    )?;
    let mut gaps = Vec::new();
    if work.source.freshness != Freshness::Fresh {
        gaps.push("current work projection is not fresh".into());
    }
    for source in &events.source_changes {
        if source.legacy_events > 0 {
            gaps.push(format!(
                "source {}: {} legacy events lack historical entity changes",
                source.source_id, source.legacy_events
            ));
        }
        if source
            .after
            .as_ref()
            .is_some_and(|s| s.active && s.freshness != Freshness::Fresh)
        {
            gaps.push(format!(
                "source {}: latest recorded state is not fresh",
                source.source_id
            ));
        }
    }
    Ok(RecentDelta {
        project_id, work_item_id: work.item.meta.id, work_item_key: work_key.into(), branch_id: branch,
        after_revision, baseline_origin: origin.into(), checkpoint_id: checkpoint.map(|cp| cp.id), events, gaps,
        history_scope: "Source changes: all project sources after baseline, including retired sources. Process history: this work or project-global events on the exact branch; high/critical summaries after baseline. Full immutable events remain available through Store.event/project EventQuery; omitted counts are explicit.".into(),
    })
}
