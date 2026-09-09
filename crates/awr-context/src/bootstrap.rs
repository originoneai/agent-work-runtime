use crate::{RuleScopeInput, SourceVersion, select_rules};
use awr_core::*;
use awr_source::{Manifest, index_project};
use awr_store::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapRequest {
    pub work_item_key: Option<String>,
    pub session_id: Option<Id>,
    pub agent_id: Option<String>,
    pub token_budget: usize,
}
impl Default for BootstrapRequest {
    fn default() -> Self {
        Self {
            work_item_key: None,
            session_id: None,
            agent_id: None,
            token_budget: 1000,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct BootstrapWork {
    pub id: Id,
    pub external_key: String,
    pub title: String,
    pub phase: Option<String>,
    pub status: WorkStatus,
    pub raw_status: String,
    pub next_action: String,
    pub blocker: Option<String>,
    pub revision: Revision,
    pub source_ref: SourceRef,
}
#[derive(Debug, Clone, Serialize)]
pub struct BootstrapGap {
    pub code: String,
    pub reference: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct BootstrapContext {
    pub project_id: Id,
    pub project_key: String,
    pub project_name: String,
    pub project_revision: Revision,
    pub branch_id: Option<Id>,
    pub work: Option<BootstrapWork>,
    pub selection_basis: String,
    pub session: Option<Session>,
    pub checkpoint: Option<Checkpoint>,
    pub checkpoint_origin: &'static str,
    pub executions: Vec<Execution>,
    pub critical_rules: Vec<Rule>,
    pub source_revisions: Vec<SourceVersion>,
    pub gaps: Vec<BootstrapGap>,
    pub complete: bool,
    pub execution_context_complete: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct BootstrapPack {
    pub level: &'static str,
    pub context: BootstrapContext,
    pub rendered_context: String,
    pub context_hash: String,
    pub token_estimate: usize,
    pub token_budget: usize,
    pub tokenizer: &'static str,
    pub token_scope: &'static str,
}

fn gap(
    gaps: &mut Vec<BootstrapGap>,
    code: &str,
    reference: impl Into<String>,
    reason: impl Into<String>,
) {
    gaps.push(BootstrapGap {
        code: code.into(),
        reference: reference.into(),
        reason: reason.into(),
    });
}
fn reference(meta: &ProjectionMeta) -> String {
    format!(
        "{}@r{} source={}{}",
        meta.id,
        meta.revision,
        meta.source_ref.source_id,
        meta.source_ref.pointer.as_deref().unwrap_or_default()
    )
}

fn render(context: &BootstrapContext) -> String {
    let mut text = format!(
        "AWR L0 Bootstrap\nProject: {} [{}] {}\nRevision: {} | Branch: {}\n",
        context.project_name,
        context.project_key,
        context.project_id,
        context.project_revision,
        context
            .branch_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "main".into())
    );
    if let Some(work) = &context.work {
        text.push_str(&format!("Work: {} [{}] r{} — {}\nPhase: {} | Status: {} ({:?})\nNext: {}\nBlocker: {}\nWork source: {}{}@r{}\n",work.external_key,work.id,work.revision,work.title,work.phase.as_deref().unwrap_or("unspecified"),work.raw_status,work.status,work.next_action,work.blocker.as_deref().unwrap_or("none"),work.source_ref.source_id,work.source_ref.pointer.as_deref().unwrap_or_default(),work.source_ref.source_revision));
    } else {
        text.push_str("Work: unselected\n");
    }
    if let Some(session) = &context.session {
        text.push_str(&format!(
            "Session: {} r{} {} agent={} provider={} model={}\n",
            session.id,
            session.revision,
            session.status,
            session.agent_id,
            session.provider,
            session.model
        ));
    } else {
        text.push_str("Session: none\n");
    }
    // Share each source-version heading while retaining every exact pointer
    // and rule text. The structured envelope also retains full entity IDs.
    let mut rule_groups = BTreeMap::new();
    for rule in &context.critical_rules {
        let source = &rule.meta.source_ref;
        rule_groups
            .entry((source.source_id, source.source_revision))
            .or_insert_with(Vec::new)
            .push(rule);
    }
    for ((source_id, revision), rules) in rule_groups {
        text.push_str(&format!("Hard rules [{source_id}@r{revision}]:\n"));
        for rule in rules {
            text.push_str(&format!(
                "[{}]\n{}\n",
                rule.meta.source_ref.pointer.as_deref().unwrap_or("/"),
                rule.text
            ));
        }
    }
    if let Some(cp) = &context.checkpoint {
        text.push_str(&format!(
            "Checkpoint origin: {}\n",
            context.checkpoint_origin
        ));
        text.push_str(&format!(
            "Checkpoint: {} session={} r{} base={} hash={}\nDigest: {}\nCheckpoint next: {}\n",
            cp.id,
            cp.session_id,
            cp.revision,
            cp.project_revision,
            cp.context_hash,
            cp.digest,
            cp.next_action
        ));
        for item in &cp.open_loops {
            text.push_str(&format!("Open loop: {item}\n"));
        }
        for item in &cp.changed_entities {
            text.push_str(&format!("Changed: {item}\n"));
        }
    } else {
        text.push_str("Checkpoint: none for selected work\n");
    }
    for execution in &context.executions {
        text.push_str(
            &execution
                .continuity_text()
                .expect("execution text serializes"),
        );
        text.push('\n');
    }
    // L1 carries the source revision inventory. Keep that complete inventory in
    // the structured L0 envelope and hash binding without repeating it in the
    // orientation text. Unavailable/stale source gaps below remain explicit.
    if context.complete {
        text.push_str("Bootstrap: complete\n");
    } else {
        text.push_str("CONTEXT INCOMPLETE\n");
    }
    for issue in &context.gaps {
        text.push_str(&format!(
            "Gap {} [{}]: {}\n",
            issue.code, issue.reference, issue.reason
        ));
    }
    text.push_str("Compile L1 before execution.\n");
    text
}

/// Refresh source fingerprints, resolve one work/session, and preserve all selected hard facts.
/// The budget applies to rendered_context; the JSON envelope is a diagnostic representation.
pub fn bootstrap(
    store: &mut Store,
    root: &Path,
    request: &BootstrapRequest,
) -> Result<BootstrapPack> {
    awr_core::ensure_public_data(request)?;
    crate::public_context(bootstrap_selected(store, root, request))
}

fn bootstrap_selected(
    store: &mut Store,
    root: &Path,
    request: &BootstrapRequest,
) -> Result<BootstrapPack> {
    if request.token_budget == 0 || request.token_budget > 100_000 {
        return Err(Error::InvalidInput(
            "bootstrap budget must be 1..100000 tokens".into(),
        ));
    }
    let root = root.canonicalize()?;
    let manifest = Manifest::load(&root)?;
    let refresh = index_project(store, &root, &manifest, false)?;
    let project = store.project(refresh.project_id)?;
    let sources = store.sources(project.id)?;
    let mut gaps = Vec::new();
    for issue in &refresh.issues {
        gap(
            &mut gaps,
            &issue.code,
            issue.locator.as_deref().unwrap_or(&issue.mapping),
            &issue.message,
        );
    }
    let mut selected = request
        .work_item_key
        .as_deref()
        .map(|key| store.work_item(project.id, key))
        .transpose()?;
    let session = if let Some(id) = request.session_id {
        let session = store.session(project.id, id)?;
        if request
            .agent_id
            .as_ref()
            .is_some_and(|agent| agent != &session.agent_id)
            || selected
                .as_ref()
                .is_some_and(|work| session.work_item_id != Some(work.item.meta.id))
        {
            return Err(Error::InvalidInput(
                "session does not match requested agent or work".into(),
            ));
        }
        Some(session)
    } else {
        match store.select_active_session(
            project.id,
            None,
            selected.as_ref().map(|w| w.item.meta.id),
            request.agent_id.as_deref(),
            project.current_branch_id,
        ) {
            Ok(session) => Some(session),
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        }
    };
    let branch = session
        .as_ref()
        .map(|s| s.branch_id)
        .unwrap_or(project.current_branch_id);
    let mut selection_basis = if request.work_item_key.is_some() {
        "explicit_work"
    } else {
        "none"
    };
    if selected.is_none() {
        if let Some(id) = session.as_ref().and_then(|s| s.work_item_id) {
            match store.work_item_by_id(project.id, id) {
                Ok(work) => {
                    selected = Some(work);
                    selection_basis = "session";
                }
                Err(Error::NotFound(_)) => gap(
                    &mut gaps,
                    "retired_session_work",
                    id.to_string(),
                    "session work is no longer projected from current sources",
                ),
                Err(error) => return Err(error),
            }
        } else {
            let mut candidates = store
                .work_items(project.id)?
                .into_iter()
                .filter(|w| matches!(w.item.status, WorkStatus::Claimed | WorkStatus::InProgress))
                .collect::<Vec<_>>();
            if candidates.len() > 1 {
                return Err(Error::InvalidInput(format!(
                    "multiple current work items; specify --work: {}",
                    candidates
                        .iter()
                        .map(|w| w.item.meta.external_key.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            selected = candidates.pop();
            if selected.is_some() {
                selection_basis = "source_current";
            }
        }
    }
    if selected.is_none() {
        gap(
            &mut gaps,
            "work_selection_required",
            "project",
            "no unambiguous current work; supply --work",
        );
    }
    let mut used_sources = BTreeSet::new();
    if let Some(work) = &selected {
        used_sources.insert(work.source.id);
        if work.source.freshness != Freshness::Fresh {
            gap(
                &mut gaps,
                "work_source_not_fresh",
                work.source.id.to_string(),
                "work facts are retained observations, not current source facts",
            );
        }
        if work.item.status == WorkStatus::Unknown {
            gap(
                &mut gaps,
                "unknown_work_status",
                work.item.meta.id.to_string(),
                &work.item.raw_status,
            );
        }
        if work.item.next_action.trim().is_empty() {
            gap(
                &mut gaps,
                "missing_next_action",
                work.item.meta.id.to_string(),
                "next action is not declared",
            );
        }
        if !awr_source::minimal_context(&sources)
            && work
                .item
                .milestone
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
        {
            gap(
                &mut gaps,
                "missing_phase",
                work.item.meta.id.to_string(),
                "current phase is not declared",
            );
        }
        if work.item.status == WorkStatus::Blocked
            && work
                .item
                .blocker
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
        {
            gap(
                &mut gaps,
                "missing_blocker",
                work.item.meta.id.to_string(),
                "work is blocked without a declared reason",
            );
        }
    }
    if !sources.iter().any(|s| s.domain == "rules") && !awr_source::minimal_context(&sources) {
        gap(
            &mut gaps,
            "rules_source_missing",
            "rules",
            "no rules source is configured or available",
        );
    }
    let selection = select_rules(
        store,
        &project,
        selected.as_ref(),
        &RuleScopeInput {
            agent_id: request
                .agent_id
                .clone()
                .or_else(|| session.as_ref().map(|s| s.agent_id.clone())),
            ..Default::default()
        },
    )?;
    let mut critical_rules = Vec::new();
    for matched in selection.unknown {
        used_sources.insert(matched.rule.source.id);
        gap(
            &mut gaps,
            "rule_applicability_unknown",
            reference(&matched.rule.item.meta),
            matched.reasons.join("; "),
        );
    }
    for rule in selection.hard {
        used_sources.insert(rule.source.id);
        critical_rules.push(rule.item);
    }
    for source in sources.iter().filter(|s| s.domain == "rules") {
        used_sources.insert(source.id);
        if source.freshness != Freshness::Fresh {
            gap(
                &mut gaps,
                "rules_source_not_fresh",
                source.id.to_string(),
                "critical rules cannot be certified current",
            );
        }
    }
    critical_rules.sort_by_key(|r| r.meta.id);
    let mut checkpoint = if let Some(session) = &session {
        let own = store.latest_checkpoint(project.id, session.id)?;
        let inherited = store.incoming_handoff(project.id, session.id)?;
        own.into_iter()
            .chain(inherited)
            .max_by_key(|c| (c.project_revision, c.created_at, c.id))
    } else {
        None
    };
    let mut checkpoint_origin = if let Some(cp) = &checkpoint {
        if session.as_ref().is_some_and(|s| s.id == cp.session_id) {
            "session"
        } else {
            "handoff"
        }
    } else {
        "none"
    };
    if checkpoint.is_none() {
        if let Some(work) = &selected {
            checkpoint = store.latest_work_checkpoint(project.id, work.item.meta.id, branch)?;
            if checkpoint.is_some() {
                checkpoint_origin = "previous_closed_session";
            }
        }
    }
    let source_revisions = sources
        .into_iter()
        .filter(|s| used_sources.contains(&s.id))
        .map(|s| SourceVersion {
            id: s.id,
            revision: s.revision,
            fingerprint: s.fingerprint,
            freshness: s.freshness,
            locator: s.locator,
        })
        .collect();
    let executions = if let Some(work) = &selected {
        store
            .executions(project.id, Some(work.item.meta.id))?
            .into_iter()
            .filter(|e| e.branch_id == branch)
            .collect()
    } else {
        Vec::new()
    };
    let context = BootstrapContext {
        project_id: project.id,
        project_key: project.external_key,
        project_name: project.name,
        project_revision: project.project_revision,
        branch_id: branch,
        work: selected.map(|w| BootstrapWork {
            id: w.item.meta.id,
            external_key: w.item.meta.external_key,
            title: w.item.title,
            phase: w.item.milestone,
            status: w.item.status,
            raw_status: w.item.raw_status,
            next_action: w.item.next_action,
            blocker: w.item.blocker,
            revision: w.item.meta.revision,
            source_ref: w.item.meta.source_ref,
        }),
        selection_basis: selection_basis.into(),
        session,
        checkpoint,
        checkpoint_origin,
        executions,
        critical_rules,
        source_revisions,
        complete: gaps.is_empty(),
        gaps,
        execution_context_complete: false,
    };
    let rendered_context = render(&context);
    let token_estimate = crate::token_count(&rendered_context);
    if token_estimate > request.token_budget {
        return Err(Error::BudgetExceeded {
            required: token_estimate,
            budget: request.token_budget,
        });
    }
    let actual = store.project(project.id)?.project_revision;
    if actual != project.project_revision {
        return Err(Error::RevisionConflict {
            expected: project.project_revision,
            actual,
        });
    }
    let mut hasher = Sha256::new();
    hasher.update(b"awr.bootstrap.v2\0o200k_base\0");
    hasher.update(serde_json::to_vec(request)?);
    hasher.update(serde_json::to_vec(&context)?);
    hasher.update(rendered_context.as_bytes());
    Ok(BootstrapPack {
        level: "L0",
        context,
        rendered_context,
        context_hash: format!("{:x}", hasher.finalize()),
        token_estimate,
        token_budget: request.token_budget,
        tokenizer: "o200k_base",
        token_scope: "rendered_context; excludes transport envelope and surrounding conversation",
    })
}
