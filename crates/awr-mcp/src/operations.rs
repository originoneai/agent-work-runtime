use crate::{
    arguments::*,
    project::{ReadProject, database, write_project},
};
use awr_context::{ContextRequest, DeltaBaseline, compile_branch_context, compile_context};
use awr_core::*;
use awr_runtime::{CompleteWorkRequest, Runtime, WorkActionRequest};
use awr_store::{SearchQuery, Store};
use rmcp::model::{CallToolResult, JsonObject};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn parse<T: DeserializeOwned>(args: Value) -> Result<T> {
    awr_core::ensure_public_value(&args)?;
    serde_json::from_value(args)
        .map_err(|_| Error::InvalidInput("tool arguments do not match the tool schema".into()))
}
fn check_sha(sha: Option<&str>) -> Result<()> {
    if sha.is_some_and(|s| !is_source_sha(s)) {
        return Err(Error::InvalidInput(
            "source_sha must be a full source hash".into(),
        ));
    }
    Ok(())
}
fn branch(store: &Store, project: &Project, reference: Option<&str>) -> Result<Option<Id>> {
    match reference {
        Some(reference) => store.resolve_branch(project.id, reference),
        None => Ok(project.current_branch_id),
    }
}
fn short(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > 240 {
        format!("{}…", text.chars().take(240).collect::<String>())
    } else {
        text
    }
}
fn brief(work: &Projected<WorkItem>) -> Value {
    json!({"id":work.item.meta.id,"external_key":work.item.meta.external_key,"title":work.item.title,
        "status":work.item.status,"raw_status":work.item.raw_status,"summary":short(if work.item.summary.is_empty(){&work.item.title}else{&work.item.summary}),
        "priority":work.item.priority,"milestone":work.item.milestone,"owner":work.item.owner,"next_action":work.item.next_action,
        "blocker":work.item.blocker,"revision":work.item.meta.revision,"source_revision":work.item.meta.source_ref.source_revision,"freshness":work.source.freshness})
}
fn ready_brief(work: &WorkReadiness) -> Value {
    let mut value = brief(&work.work);
    value["ready"] = json!(work.ready);
    value["diagnostics"] = json!(work.diagnostics);
    value["active_claims"] = json!(work.active_claims.iter().map(|c| json!({"id":c.id,"agent_id":c.agent_id,"session_id":c.session_id,"expires_at":c.expires_at})).collect::<Vec<_>>());
    value
}

pub(crate) fn call(root: &Path, name: &str, args: JsonObject) -> Result<CallToolResult> {
    let args = Value::Object(args);
    if serde_json::to_vec(&args)?.len() > 1024 * 1024 {
        return Err(Error::InvalidInput("tool arguments exceed 1 MiB".into()));
    }
    match name {
        "awr_work_transition" => return transition(root, parse(args)?),
        "awr_event_append" => return append_event(root, parse(args)?),
        "awr_evidence_record" => return evidence(root, parse(args)?),
        _ => (),
    }
    let mut view = ReadProject::open(root)?;
    let mut value = match name {
        "awr_project_status" => status(&view, parse(args)?)?,
        "awr_work_ready" => ready(&view, parse(args)?)?,
        "awr_work_get" => work(&view, parse(args)?)?,
        "awr_context_compile" => context(&mut view, root, parse(args)?)?,
        "awr_search" => search(&mut view, parse(args)?)?,
        _ => return Err(Error::Unsupported(name.into())),
    };
    view.finish(root)?;
    let incomplete = name == "awr_context_compile" && value["completeness"]["complete"] == false;
    for (key, item) in view.metadata().as_object().expect("metadata object") {
        // Context packs carry their own required gaps and source provenance.
        if name == "awr_context_compile"
            && matches!(key.as_str(), "source_issues" | "source_warnings")
        {
            continue;
        }
        value[key] = item.clone();
    }
    ensure_public_value(&value)?;
    if incomplete {
        value["ok"] = json!(false);
        value["error"] = json!(
            Error::ContextIncomplete(
                "L1 has required gaps; inspect completeness before execution".into()
            )
            .report()
        );
        Ok(CallToolResult::structured_error(value))
    } else {
        Ok(CallToolResult::structured(value))
    }
}

fn status(view: &ReadProject, args: StatusArgs) -> Result<Value> {
    let branch = branch(&view.store, &view.project, args.branch.as_deref())?;
    let works = view.store.work_items(view.project.id)?;
    let ready = view
        .store
        .ready_work(view.project.id, branch, now_millis()?)?;
    let mut counts = BTreeMap::<String, usize>::new();
    for work in &works {
        *counts
            .entry(
                serde_json::to_value(work.item.status)?
                    .as_str()
                    .unwrap()
                    .into(),
            )
            .or_default() += 1;
    }
    let mut current = works
        .iter()
        .filter(|w| matches!(w.item.status, WorkStatus::Claimed | WorkStatus::InProgress))
        .collect::<Vec<_>>();
    current.sort_by_key(|w| (&w.item.priority, &w.item.meta.external_key));
    let focus = current
        .first()
        .copied()
        .or_else(|| ready.ready.first().map(|w| &w.work));
    let next = focus
        .map(|w| w.item.next_action.clone())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if ready.blocked.is_empty() {
                "No nonterminal work remains in current source projections.".into()
            } else {
                "Inspect readiness diagnostics and resolve the blocking prerequisite.".into()
            }
        });
    Ok(
        json!({"project":view.project.name,"project_id":view.project.id,"branch_id":branch,
        "counts":counts,"total":works.len(),"current":current.iter().take(5).map(|w| brief(w)).collect::<Vec<_>>(),
        "current_total":current.len(),"ready_count":ready.ready.len(),"blocked_count":ready.blocked.len(),
        "next_action":next,"suggested_work":focus.map(brief)}),
    )
}
fn ready(view: &ReadProject, args: ReadyArgs) -> Result<Value> {
    if args.limit == 0 || args.limit > 100 {
        return Err(Error::InvalidInput("ready limit must be 1..100".into()));
    }
    let branch = branch(&view.store, &view.project, args.branch.as_deref())?;
    let report = view
        .store
        .ready_work(view.project.id, branch, now_millis()?)?;
    let mut counts = BTreeMap::<String, usize>::new();
    for work in &report.blocked {
        for code in work
            .diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<BTreeSet<_>>()
        {
            *counts.entry(code).or_default() += 1;
        }
    }
    Ok(
        json!({"branch_id":branch,"ready":report.ready.iter().take(args.limit).map(ready_brief).collect::<Vec<_>>(),
        "ready_total":report.ready.len(),"truncated":report.ready.len()>args.limit,"blocked_total":report.blocked.len(),
        "diagnostic_counts":counts,"blocked_sample":report.blocked.iter().take(3).map(ready_brief).collect::<Vec<_>>()}),
    )
}
fn work(view: &ReadProject, args: WorkArgs) -> Result<Value> {
    check_sha(args.source_sha.as_deref())?;
    let branch = branch(&view.store, &view.project, args.branch.as_deref())?;
    let work = view
        .store
        .work_readiness(view.project.id, &args.work, branch, now_millis()?)?;
    let decisions = view.store.decisions_for_work(view.project.id, &args.work)?;
    let evidence = view.store.evidence_for_work(
        view.project.id,
        &args.work,
        args.source_sha.as_deref(),
        branch,
    )?;
    Ok(
        json!({"work":ready_brief(&work),"acceptance":work.work.item.acceptance,"source_ref":work.work.item.meta.source_ref,
        "required_dependencies":work.dependencies.dependencies.iter().map(|d| json!({"external_key":d.item.meta.external_key,"status":d.item.status,"revision":d.item.meta.revision,"source_revision":d.item.meta.source_ref.source_revision,"freshness":d.source.freshness})).collect::<Vec<_>>(),
        "missing_dependencies":work.dependencies.missing_keys,"dependency_cycles":work.dependencies.cycle_keys,
        "decisions":decisions.iter().map(|d| json!({"external_key":d.decision.item.meta.external_key,"summary":short(&d.decision.item.decision),"relevance":d.relevance,"reasons":d.reasons,"source_ref":d.decision.item.meta.source_ref})).collect::<Vec<_>>(),
        "evidence":evidence.iter().map(|e| json!({"external_key":e.evidence.item.external_key,"summary":short(&e.evidence.item.summary),"level":e.evidence.item.level,"currency":e.currency,"missing_bindings":e.missing_bindings,"locator":e.evidence.item.locator,"source_sha":e.evidence.item.source_sha,"reasons":e.reasons})).collect::<Vec<_>>(),
        "evidence_currency_basis":{"requested_source_sha":args.source_sha,"branch_id":branch}}),
    )
}
fn context(view: &mut ReadProject, root: &Path, args: ContextArgs) -> Result<Value> {
    check_sha(args.source_sha.as_deref())?;
    if args.checkpoint.is_some() && args.after_revision.is_some() {
        return Err(Error::InvalidInput(
            "use checkpoint or after_revision, not both".into(),
        ));
    }
    let request = ContextRequest {
        work_item_key: args.work,
        session_id: args.session,
        detached: args.detached,
        agent_id: args.agent,
        branch_id: None,
        goal_keys: args.goals,
        paths: args.paths,
        tags: args.tags,
        source_sha: args.source_sha,
        intent: args.intent.unwrap_or_else(|| "work".into()),
        token_budget: args.budget.unwrap_or(5000),
        delta_baseline: if let Some(id) = args.checkpoint {
            DeltaBaseline::Checkpoint { id }
        } else if let Some(revision) = args.after_revision {
            DeltaBaseline::Revision { revision }
        } else {
            DeltaBaseline::Auto
        },
    };
    let report = match args.branch {
        Some(branch) => compile_branch_context(&mut view.store, root, &branch, &request)?,
        None => compile_context(&mut view.store, root, &request)?,
    };
    Ok(serde_json::to_value(report)?)
}
fn search(view: &mut ReadProject, args: SearchArgs) -> Result<Value> {
    let query = SearchQuery {
        text: args.text,
        kind: args
            .kind
            .map(|k| if k == "work" { "work_item".into() } else { k }),
        status: args.status,
        work_item_key: args.work,
        limit: args.limit,
    };
    let report = view.store.search(view.project.id, &query)?;
    Ok(
        json!({"hits":report.hits,"truncated":report.truncated,"index_policy_version":report.index_policy_version,"query":query}),
    )
}
fn transition(root: &Path, args: TransitionArgs) -> Result<CallToolResult> {
    let mut store = Store::open_existing(&database(root)?)?;
    let result = if args.action == WorkAction::Complete {
        if args.next_action.is_some() || args.summary.is_some() || args.blocker.is_some() {
            return Err(Error::InvalidInput(
                "complete uses only reason and completion bindings, not progress fields".into(),
            ));
        }
        let input = args.completion.ok_or_else(|| {
            Error::EvidenceMissing("complete requires an acceptance-to-evidence mapping".into())
        })?;
        if serde_json::to_vec(&input)?.len() > 64 * 1024 {
            return Err(Error::InvalidInput(
                "completion mapping exceeds 64 KiB".into(),
            ));
        }
        awr_runtime::complete_work(
            &mut store,
            root,
            &CompleteWorkRequest {
                target: args.work,
                session_id: args.session,
                expected_revision: args.expected_revision,
                reason: args.reason,
                input,
            },
        )?
    } else {
        if args.completion.is_some() {
            return Err(Error::InvalidInput(
                "only complete accepts completion bindings".into(),
            ));
        }
        awr_runtime::perform_work_action(
            &mut store,
            root,
            &WorkActionRequest {
                target: args.work,
                session_id: args.session,
                expected_revision: args.expected_revision,
                input: WorkActionInput {
                    action: args.action,
                    reason: args.reason,
                    next_action: args.next_action,
                    summary: args.summary,
                    blocker: args.blocker,
                },
            },
        )?
    };
    let mut value = serde_json::to_value(&result)?;
    if let Some(error) = result.failure {
        value["error"] = json!(error.report());
        Ok(CallToolResult::structured_error(value))
    } else {
        Ok(CallToolResult::structured(value))
    }
}
fn append_event(root: &Path, args: EventArgs) -> Result<CallToolResult> {
    let (mut store, project) = write_project(root, args.expected_revision)?;
    let branch = branch(&store, &project, args.branch.as_deref())?;
    let work_item_id = args
        .work
        .map(|w| store.work_item(project.id, &w).map(|w| w.item.meta.id))
        .transpose()?;
    let draft = EventDraft {
        work_item_id,
        session_id: args.session,
        branch_id: branch,
        event_type: args.event_type,
        importance: args.importance.unwrap_or_else(|| "normal".into()),
        summary: args.summary,
        payload: args.payload.unwrap_or_else(|| json!({})),
    };
    let mut runtime = Runtime::attach(&mut store, project.id)?;
    let event = if args.branch.is_some() {
        runtime.append_event_in_branch(args.expected_revision, draft)
    } else {
        runtime.append_event(args.expected_revision, draft)
    }?;
    Ok(CallToolResult::structured(
        json!({"ok":true,"project_revision":event.project_revision,"event":event,"freshness_basis":"source_refresh","source_refresh_performed":true}),
    ))
}
fn evidence(root: &Path, args: EvidenceArgs) -> Result<CallToolResult> {
    let (mut store, project) = write_project(root, args.expected_revision)?;
    let branch_id = branch(&store, &project, args.branch.as_deref())?;
    let (evidence, event) = Runtime::attach(&mut store, project.id)?.record_evidence(
        args.expected_revision,
        EvidenceDraft {
            external_key: args.external_key,
            work_item_key: args.work,
            evidence_type: args.evidence_type,
            level: args.level,
            summary: args.summary,
            locator: args.locator,
            sha256: args.sha256,
            source_sha: args.source_sha,
            command: args.command,
            scope: args.scope,
            branch_id,
            verified_at: args.verified_at,
        },
    )?;
    Ok(CallToolResult::structured(
        json!({"ok":true,"project_revision":event.project_revision,"evidence":evidence,"event_id":event.id,"validation_basis":"caller_supplied_bindings","freshness_basis":"source_refresh","source_refresh_performed":true}),
    ))
}
