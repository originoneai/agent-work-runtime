use awr_context::{ContextRequest, DeltaBaseline, WorkContextReport, compile_context};
use awr_core::*;
use awr_source::{Manifest, index_project};
use awr_store::Store;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeRequest {
    pub from_session_id: Option<Id>,
    pub work_item_key: Option<String>,
    pub agent_id: String,
    pub provider: String,
    pub model: String,
    pub claim: ResumeClaim,
    pub claim_ttl_ms: Option<u64>,
    pub expected_revision: Revision,
    pub token_budget: usize,
    pub paths: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub goal_keys: Vec<String>,
    pub source_sha: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct ResumeReport {
    pub selection_basis: &'static str,
    pub from_session_id: Id,
    pub checkpoint_id: Option<Id>,
    pub recovery_basis: &'static str,
    pub recovery_gaps: Vec<String>,
    pub checkpoint_save: Option<serde_json::Value>,
    pub resumed: Option<SessionResumed>,
    pub event_id: Option<Id>,
    pub committed_project_revision: Option<Revision>,
    pub context_phase: &'static str,
    pub context: Option<WorkContextReport>,
    pub context_error: Option<ErrorReport>,
    /// Current L1 completeness, not a claim that unrecorded historical memory was recovered.
    pub context_ready: bool,
}

/// Source refresh and target-agent preflight precede the atomic runtime transition.
/// The returned execution context is compiled for the new session after the transition.
pub fn resume_session(
    store: &mut Store,
    root: &Path,
    request: &ResumeRequest,
) -> Result<ResumeReport> {
    if [&request.agent_id, &request.provider, &request.model]
        .iter()
        .any(|s| s.trim().is_empty())
        || request
            .work_item_key
            .as_ref()
            .is_some_and(|s| s.trim().is_empty())
    {
        return Err(Error::InvalidInput(
            "resume selectors and target identity must not be blank".into(),
        ));
    }
    if request.claim != ResumeClaim::Acquire && request.claim_ttl_ms.is_some() {
        return Err(Error::InvalidInput(
            "claim TTL requires explicit acquisition".into(),
        ));
    }
    let root = root.canonicalize()?;
    let refresh = index_project(store, &root, &Manifest::load(&root)?, false)?;
    if !refresh.ok {
        return Err(Error::SourceStale(format!(
            "resume refresh failed: {}",
            serde_json::to_string(&refresh.issues)?
        )));
    }
    let project = store.project_by_root(&root)?;
    if project.project_revision != request.expected_revision {
        return Err(Error::RevisionConflict {
            expected: request.expected_revision,
            actual: project.project_revision,
        });
    }
    let (from, basis) = if let Some(id) = request.from_session_id {
        (store.session(project.id, id)?, "explicit_session")
    } else {
        let candidates = store.resume_candidates(
            project.id,
            request.work_item_key.as_deref(),
            project.current_branch_id,
        )?;
        if candidates.iter().filter(|s| s.status == "active").count() > 1 {
            return Err(Error::InvalidInput(format!(
                "multiple active sessions; specify --from-session: {}",
                candidates
                    .iter()
                    .map(|s| s.id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let from = candidates.into_iter().next().ok_or_else(|| {
            Error::NotFound("recoverable session; specify --from-session or start new work".into())
        })?;
        let basis = if from.status == "active" {
            "active_session"
        } else {
            "latest_closed_session"
        };
        (from, basis)
    };
    if from.branch_id != project.current_branch_id {
        return Err(Error::InvalidInput(
            "resume requires the current project branch".into(),
        ));
    }
    if let Some(successor) = store.resumed_successor(project.id, from.id)? {
        return Err(Error::InvalidTransition(format!(
            "session {} already resumed as {}; inspect that successor",
            from.id, successor.id
        )));
    }
    let work = store.work_item_by_id(
        project.id,
        from.work_item_id
            .ok_or_else(|| Error::InvalidInput("resume requires a work-bound session".into()))?,
    )?;
    if request
        .work_item_key
        .as_ref()
        .is_some_and(|key| key != &work.item.meta.external_key)
    {
        return Err(Error::InvalidInput(
            "resume session belongs to another work item".into(),
        ));
    }
    if matches!(
        work.item.status,
        WorkStatus::Completed | WorkStatus::Cancelled | WorkStatus::Unknown
    ) {
        return Err(Error::InvalidTransition(
            "resume requires known nonterminal work".into(),
        ));
    }
    let checkpoint = store.recovery_checkpoint(project.id, from.id)?;
    let attempts = store.checkpoint_attempts(project.id, from.id, 1)?;
    let mut gaps = Vec::new();
    if checkpoint.is_none() {
        gaps.push("No successful checkpoint: recover current source facts and delta since session start; unrecorded digest/next action/open loops are unavailable.".into());
    }
    if attempts.incomplete_count > 0 {
        gaps.push(format!("{} checkpoint attempts have no completion receipt and are excluded from recovery points; inspect source session history.",attempts.incomplete_count));
    }
    let checkpoint_id = checkpoint.as_ref().map(|cp| cp.id);
    let checkpoint_save = checkpoint_id
        .map(|id| store.checkpoint_save_metadata(project.id, id))
        .transpose()?;
    if checkpoint_save
        .as_ref()
        .is_some_and(|m| m["delta_recorded"] != true)
    {
        gaps.push("Legacy checkpoint has no saved session delta; its recorded next action and loops remain available.".into());
    }
    let mut context_request = ContextRequest {
        work_item_key: Some(work.item.meta.external_key.clone()),
        session_id: None,
        detached: true,
        agent_id: Some(request.agent_id.clone()),
        paths: request.paths.clone(),
        tags: request.tags.clone(),
        goal_keys: request.goal_keys.clone(),
        source_sha: request.source_sha.clone(),
        token_budget: request.token_budget,
        intent: "resume".into(),
        delta_baseline: checkpoint_id
            .map(|id| DeltaBaseline::Checkpoint { id })
            .unwrap_or(DeltaBaseline::Revision {
                revision: store.session_recovery_revision(project.id, from.id)?,
            }),
        ..Default::default()
    };
    let mut report = ResumeReport {
        selection_basis: basis,
        from_session_id: from.id,
        checkpoint_id,
        recovery_basis: if checkpoint_id.is_some() {
            "checkpoint"
        } else {
            "source_and_session_start"
        },
        recovery_gaps: gaps,
        checkpoint_save,
        resumed: None,
        event_id: None,
        committed_project_revision: None,
        context_phase: "preflight",
        context: None,
        context_error: None,
        context_ready: false,
    };
    let preflight = match compile_context(store, &root, &context_request) {
        Ok(context) => context,
        Err(error) => {
            report.context_error = Some(error.report());
            return Ok(report);
        }
    };
    if preflight.completeness.project_revision != request.expected_revision {
        return Err(Error::RevisionConflict {
            expected: request.expected_revision,
            actual: preflight.completeness.project_revision,
        });
    }
    if !preflight.completeness.complete {
        report.context = Some(preflight);
        return Ok(report);
    }
    let hash = preflight
        .work_context
        .as_ref()
        .ok_or_else(|| {
            Error::ContextIncomplete("resume preflight produced no work context".into())
        })?
        .context_hash
        .clone();
    let (resumed, event) = store.resume_session(
        project.id,
        request.expected_revision,
        SessionResumeDraft {
            from_session_id: from.id,
            checkpoint_id,
            agent_id: request.agent_id.clone(),
            provider: request.provider.clone(),
            model: request.model.clone(),
            claim: request.claim,
            claim_ttl_ms: request.claim_ttl_ms,
            prepared_context_hash: hash,
        },
    )?;
    context_request.session_id = Some(resumed.session.id);
    context_request.detached = false;
    report.resumed = Some(resumed);
    report.event_id = Some(event.id);
    report.committed_project_revision = Some(event.project_revision);
    report.context_phase = "resumed_session";
    match compile_context(store, &root, &context_request) {
        Ok(context) => {
            report.context_ready = context.completeness.complete;
            report.context = Some(context);
        }
        Err(error) => {
            report.context_error = Some(error.report());
        }
    }
    Ok(report)
}
