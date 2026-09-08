use awr_core::{Checkpoint, Error, Freshness, Id, Result, Revision, Session};
use awr_store::{DeltaEvents, Store};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeltaContextRequest {
    pub work_item_key: Option<String>,
    pub agent_id: Option<String>,
    #[serde(flatten)]
    pub delta: DeltaRequest,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeltaContextReport {
    pub branch_context: crate::BranchContextBinding,
    pub source_refresh_ok: bool,
    pub source_issues: Vec<awr_source::IndexIssue>,
    pub delta: RecentDelta,
}

/// Refresh authoritative sources and use the same work/session selection as L1 compilation.
pub fn context_delta(
    store: &mut Store,
    root: &Path,
    request: &DeltaContextRequest,
) -> Result<DeltaContextReport> {
    context_delta_selected(store, root, request, None)
}

/// Read the exact named branch since fork; source refresh is shared and defaults stay unchanged.
pub fn branch_delta(
    store: &mut Store,
    root: &Path,
    reference: &str,
    request: &DeltaContextRequest,
) -> Result<DeltaContextReport> {
    crate::branch::require_fork_request(&request.delta.baseline)?;
    let mut request = request.clone();
    request.delta.baseline = DeltaBaseline::BranchFork;
    context_delta_selected(store, root, &request, Some(reference))
}

fn context_delta_selected(
    store: &mut Store,
    root: &Path,
    request: &DeltaContextRequest,
    reference: Option<&str>,
) -> Result<DeltaContextReport> {
    if [&request.work_item_key, &request.agent_id]
        .into_iter()
        .flatten()
        .any(|s| s.trim().is_empty())
    {
        return Err(Error::InvalidInput(
            "work and agent selectors must not be empty".into(),
        ));
    }
    let refresh =
        awr_source::index_project(store, root, &awr_source::Manifest::load(root)?, false)?;
    let project = store.project_by_root(&root.canonicalize()?)?;
    let branch = match reference {
        Some(reference) => store.resolve_branch(project.id, reference)?,
        None => project.current_branch_id,
    };
    let branch_context = crate::branch::branch_binding(store, &project, branch)?;
    let selected = crate::compile::select_work(
        store,
        &project,
        branch,
        &crate::ContextRequest {
            work_item_key: request.work_item_key.clone(),
            session_id: request.delta.session_id,
            agent_id: request.agent_id.clone(),
            ..Default::default()
        },
    )?;
    let work = selected.work.ok_or_else(|| {
        Error::ContextIncomplete(
            "no current work item; use event/source history to inspect retained records".into(),
        )
    })?;
    let delta = recent_delta(
        store,
        project.id,
        &work.item.meta.external_key,
        branch,
        &DeltaRequest {
            session_id: selected.session.map(|s| s.id),
            ..request.delta.clone()
        },
    )?;
    if delta.events.project_revision != refresh.project_revision {
        return Err(Error::RevisionConflict {
            expected: refresh.project_revision,
            actual: delta.events.project_revision,
        });
    }
    Ok(DeltaContextReport {
        branch_context,
        source_refresh_ok: refresh.ok,
        source_issues: refresh.issues,
        delta,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeltaBaseline {
    #[default]
    Auto,
    /// Exact branch runtime window starts at its recorded fork; main starts at revision zero.
    BranchFork,
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
    #[serde(default)]
    pub fork_project_revision: Revision,
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
    let binding = crate::branch::branch_binding(store, &project, branch)?;
    let fork = binding.fork_project_revision;
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
        DeltaBaseline::Auto | DeltaBaseline::BranchFork => context_checkpoint(
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
        if cp.project_revision < fork || cp.project_revision > project.project_revision {
            return Err(Error::InvalidInput(
                "checkpoint revision is outside the branch lifetime".into(),
            ));
        }
    }
    let mut after_revision = match (&checkpoint, &request.baseline) {
        (_, DeltaBaseline::BranchFork) => {
            origin = if branch.is_some() {
                "branch_fork"
            } else {
                "main_project_start"
            };
            fork
        }
        (Some(cp), _) => cp.project_revision,
        (_, DeltaBaseline::Revision { revision }) => *revision,
        _ => {
            if let Some(session) = &session {
                let revision = store.session_recovery_revision(project_id, session.id)?;
                origin = if revision < session.start_project_revision {
                    "resumed_session_start"
                } else {
                    "session_start"
                };
                revision
            } else {
                origin = "project_start";
                0
            }
        }
    };
    if after_revision < fork {
        if matches!(
            request.baseline,
            DeltaBaseline::Revision { .. } | DeltaBaseline::Checkpoint { .. }
        ) {
            return Err(Error::InvalidInput(
                "delta baseline precedes branch fork".into(),
            ));
        }
        after_revision = fork;
        origin = "branch_fork";
    }
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
        after_revision, fork_project_revision: fork, baseline_origin: origin.into(), checkpoint_id: checkpoint.map(|cp| cp.id), events, gaps,
        history_scope: "Source changes: all project sources after baseline, including retired sources. Process history: this work or project-global events on the exact branch; high/critical summaries after baseline. Full immutable events remain available through Store.event/project EventQuery; omitted counts are explicit.".into(),
    })
}
