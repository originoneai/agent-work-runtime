//! Thin MCP adapters for existing runtime lifecycle operations.
use crate::{
    operations::parse,
    project::{database, write_project},
};
use awr_core::*;
use awr_runtime::{ResumeRequest, Runtime};
use awr_store::Store;
use rmcp::model::CallToolResult;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

pub(crate) const NAMES: [&str; 7] = [
    "awr_session_start",
    "awr_session_get",
    "awr_session_list",
    "awr_session_checkpoint",
    "awr_session_end",
    "awr_session_resume",
    "awr_session_claim",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    work: String,
    conversation: String,
    agent: String,
    provider: String,
    model: String,
    expected_revision: Revision,
    #[serde(default)]
    claim: bool,
    ttl_ms: Option<u64>,
    branch: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Get {
    session: Id,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    limit: Option<usize>,
    before_revision: Option<Revision>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    session: Id,
    expected_revision: Revision,
    context_hash: String,
    digest: String,
    next_action: String,
    #[serde(default)]
    open_loops: Vec<String>,
    #[serde(default)]
    changed_entities: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct End {
    session: Id,
    expected_revision: Revision,
    outcome: SessionOutcome,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    session: Id,
    expected_revision: Revision,
    action: String,
    claim: Option<Id>,
    ttl_ms: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resume {
    session: Id,
    conversation: String,
    agent: String,
    provider: String,
    model: String,
    expected_revision: Revision,
    #[serde(default)]
    claim: ResumeClaim,
    ttl_ms: Option<u64>,
    budget: Option<usize>,
    source_sha: Option<String>,
    paths: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    #[serde(default)]
    goals: Vec<String>,
}

pub(crate) fn authorize(
    root: &Path,
    name: &str,
    args: &mut rmcp::model::JsonObject,
    principal: Option<&str>,
) -> Result<()> {
    // Host conversations, never a transport-global "current session", resolve implicit selectors.
    let client = principal.unwrap_or("stdio");
    let conversation_selector = !matches!(name, "awr_session_start" | "awr_session_resume")
        && args.contains_key("conversation");
    let explicit = args.get("session").filter(|v| !v.is_null());
    if conversation_selector || (principal.is_some() && explicit.is_some()) {
        let store = Store::open_readonly(&database(root)?)?;
        let project = store.project_by_root(root)?;
        if conversation_selector {
            let conversation: String = serde_json::from_value(args.remove("conversation").unwrap())
                .map_err(|_| Error::InvalidInput("conversation must be a string".into()))?;
            let session = store
                .mcp_bound_session(
                    project.id,
                    &McpSessionBinding {
                        client: client.into(),
                        conversation,
                    },
                )?
                .ok_or_else(|| Error::NotFound("MCP conversation binding".into()))?;
            if args
                .get("session")
                .is_some_and(|id| !id.is_null() && *id != json!(session.id))
            {
                return Err(Error::RuleViolation(
                    "conversation and session selectors disagree".into(),
                ));
            }
            args.insert("session".into(), json!(session.id));
        }
        if principal.is_some() {
            let session: Id =
                serde_json::from_value(args.get("session").cloned().unwrap_or(Value::Null))
                    .map_err(|_| Error::InvalidInput("invalid session identifier".into()))?;
            if store
                .mcp_session_binding(project.id, session)?
                .is_none_or(|binding| binding.client != client)
            {
                return Err(Error::RuleViolation(
                    "session access denied for this client".into(),
                ));
            }
        }
    }
    if principal.is_some()
        && name == "awr_context_compile"
        && args.get("session").is_none_or(Value::is_null)
        && !(args.get("detached") == Some(&json!(true))
            && args.get("work").is_some_and(Value::is_string))
    {
        return Err(Error::InvalidInput(
            "shared context needs a session/conversation, or explicit detached work".into(),
        ));
    }
    Ok(())
}

fn snapshot(root: &Path) -> Result<(Store, Project)> {
    let store = Store::read_snapshot(&database(root)?, 256 * 1024 * 1024)?;
    let project = store.project_by_root(root)?;
    Ok((store, project))
}
fn session_value(store: &Store, project: &Project, session: Id) -> Result<Value> {
    Ok(
        json!({"ok":true,"read_only":true,"freshness_basis":"runtime_database","source_refresh_performed":false,
        "project_revision":project.project_revision,"session":store.session(project.id,session)?,
        "binding":store.mcp_session_binding(project.id,session)?,"checkpoint":store.latest_checkpoint(project.id,session)?,
        "inherited_checkpoint":store.recovery_checkpoint(project.id,session)?,"claims":store.session_claims(project.id,session)?,
        "resumed_successor":store.resumed_successor(project.id,session)?,"checkpoint_saves":store.checkpoint_attempts(project.id,session,20)?}),
    )
}
pub(crate) fn call(root: &Path, name: &str, args: Value, client: &str) -> Result<CallToolResult> {
    let value = match name {
        "awr_session_get" => {
            let args: Get = parse(args)?;
            let (store, project) = snapshot(root)?;
            session_value(&store, &project, args.session)?
        }
        "awr_session_list" => {
            let args: List = parse(args)?;
            let (store, project) = snapshot(root)?;
            let limit = args.limit.unwrap_or(20);
            let sessions = store.mcp_sessions(project.id, client, limit, args.before_revision)?;
            let next = if sessions.len() == limit {
                sessions.last().map(|s| s.start_project_revision + 1)
            } else {
                None
            };
            json!({"ok":true,"read_only":true,"source_refresh_performed":false,"freshness_basis":"runtime_database","project_revision":project.project_revision,"sessions":sessions,"next_before_revision":next,"scope":"current client"})
        }
        "awr_session_start" => {
            let args: Start = parse(args)?;
            let binding = McpSessionBinding {
                client: client.into(),
                conversation: args.conversation,
            };
            binding.validate()?;
            let (store, project) = snapshot(root)?;
            if let Some(session) = store.mcp_bound_session(project.id, &binding)? {
                if session.work_item_id != Some(store.work_identity(project.id, &args.work)?)
                    || session.agent_id != args.agent
                    || session.provider != args.provider
                    || session.model != args.model
                    || args.branch.as_ref().is_some_and(|branch| {
                        store.resolve_branch(project.id, branch).ok() != Some(session.branch_id)
                    })
                {
                    return Err(Error::SourceConflict(
                        "conversation already belongs to different work or identity".into(),
                    ));
                }
                let mut value = session_value(&store, &project, session.id)?;
                value["binding_reused"] = json!(true);
                value["next_action"] = json!(
                    "Compile current context; inspect claims before execution. Closed sessions require explicit resume."
                );
                return Ok(CallToolResult::structured(value));
            }
            drop(store);
            let (mut store, project) = write_project(root, args.expected_revision)?;
            let branch = match args.branch {
                Some(branch) => store.resolve_branch(project.id, &branch)?,
                None => project.current_branch_id,
            };
            let (started, event) = store.start_bound_session(
                project.id,
                args.expected_revision,
                SessionDraft {
                    work_item_key: Some(args.work),
                    agent_id: args.agent,
                    provider: args.provider,
                    model: args.model,
                    branch_id: branch,
                    claim: args.claim,
                    claim_ttl_ms: args.ttl_ms,
                },
                binding.clone(),
            )?;
            json!({"ok":true,"project_revision":event.project_revision,"session":started.session,"claim":started.claim,"binding":binding,"binding_reused":false,"event_id":event.id})
        }
        "awr_session_checkpoint" => {
            let args: Checkpoint = parse(args)?;
            let (mut store, project) = write_project(root, args.expected_revision)?;
            let (checkpoint, event) = Runtime::attach(&mut store, project.id)?.checkpoint(
                args.expected_revision,
                args.session,
                CheckpointDraft {
                    context_hash: args.context_hash,
                    digest: args.digest,
                    next_action: args.next_action,
                    open_loops: args.open_loops,
                    changed_entities: args.changed_entities,
                },
            )?;
            json!({"ok":true,"project_revision":event.project_revision,"checkpoint":checkpoint,"checkpoint_save":store.checkpoint_save_metadata(project.id,checkpoint.id)?,"event_id":event.id})
        }
        "awr_session_end" => {
            let args: End = parse(args)?;
            // Cleanup remains possible with stale or unavailable sources, as with CLI.
            let mut store = Store::open_existing(&database(root)?)?;
            let project = store.project_by_root(root)?;
            let (session, event) = Runtime::attach(&mut store, project.id)?.end_session(
                args.expected_revision,
                args.session,
                args.outcome,
            )?;
            json!({"ok":true,"project_revision":event.project_revision,"session":session,"closed_claim_ids":event.payload["closed_claim_ids"],"event_id":event.id})
        }
        "awr_session_claim" => {
            let args: Claim = parse(args)?;
            let (mut store, project) = if args.action == "release" {
                let store = Store::open_existing(&database(root)?)?;
                let project = store.project_by_root(root)?;
                (store, project)
            } else {
                write_project(root, args.expected_revision)?
            };
            let mut runtime = Runtime::attach(&mut store, project.id)?;
            let (claim, event) =
                match args.action.as_str() {
                    "acquire" if args.claim.is_none() => {
                        runtime.acquire_claim(args.expected_revision, args.session, args.ttl_ms)?
                    }
                    "release" if args.ttl_ms.is_none() => runtime.release_claim(
                        args.expected_revision,
                        args.session,
                        args.claim
                            .ok_or_else(|| Error::InvalidInput("release requires claim".into()))?,
                    )?,
                    _ => return Err(Error::InvalidInput(
                        "claim action requires acquire with optional ttl_ms, or release with claim"
                            .into(),
                    )),
                };
            json!({"ok":true,"project_revision":event.project_revision,"claim":claim,"event_id":event.id})
        }
        "awr_session_resume" => {
            let args: Resume = parse(args)?;
            let (mut store, _) = write_project(root, args.expected_revision)?;
            let report = awr_runtime::resume_bound_session(
                &mut store,
                root,
                &ResumeRequest {
                    from_session_id: Some(args.session),
                    work_item_key: None,
                    agent_id: args.agent,
                    provider: args.provider,
                    model: args.model,
                    claim: args.claim,
                    claim_ttl_ms: args.ttl_ms,
                    expected_revision: args.expected_revision,
                    token_budget: args.budget.unwrap_or(5000),
                    paths: args.paths,
                    tags: args.tags,
                    goal_keys: args.goals,
                    source_sha: args.source_sha,
                },
                McpSessionBinding {
                    client: client.into(),
                    conversation: args.conversation,
                },
            )?;
            let ready = report.context_ready;
            let mut value = serde_json::to_value(report)?;
            value["ok"] = json!(ready);
            value["project_revision"] = json!(store.project_by_root(root)?.project_revision);
            if !ready {
                return Ok(CallToolResult::structured_error(value));
            }
            value
        }
        _ => return Err(Error::Unsupported(name.into())),
    };
    ensure_public_value(&value)?;
    Ok(CallToolResult::structured(value))
}
