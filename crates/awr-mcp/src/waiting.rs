use crate::{
    operations::parse,
    project::{database, write_project},
};
use awr_core::*;
use awr_runtime::Runtime;
use awr_store::Store;
use rmcp::model::CallToolResult;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wait {
    session: Id,
    expected_revision: Revision,
    question: String,
    context_hash: String,
    digest: String,
    next_action: String,
    #[serde(default)]
    open_loops: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    wait: Id,
    expected_revision: Revision,
    reply: String,
    #[serde(default)]
    cancel: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reindex {
    expected_revision: Revision,
}

pub(crate) fn require_resolved(store: &Store, project: Id, session: Id) -> Result<()> {
    if store
        .mcp_waits(project, session)?
        .iter()
        .any(|wait| wait.status == "waiting_user")
    {
        return Err(Error::InvalidTransition("session is waiting for user input; record a reply or explicit cancellation before continuing".into()));
    }
    Ok(())
}
pub(crate) fn call(root: &Path, name: &str, args: Value, client: &str) -> Result<CallToolResult> {
    let value = match name {
        "awr_session_wait" => {
            let args: Wait = parse(args)?;
            if args.question.trim().is_empty() || args.question.len() > 8192 {
                return Err(Error::InvalidInput(
                    "wait question must contain 1..8192 bytes".into(),
                ));
            }
            let (mut store, project) = write_project(root, args.expected_revision)?;
            require_resolved(&store, project.id, args.session)?;
            let (checkpoint, event) = Runtime::attach(&mut store, project.id)?.checkpoint(
                args.expected_revision,
                args.session,
                CheckpointDraft {
                    context_hash: args.context_hash,
                    digest: args.digest,
                    next_action: args.next_action,
                    open_loops: args.open_loops,
                    changed_entities: vec![],
                },
            )?;
            let (wait, event) = store.create_mcp_wait(
                project.id,
                event.project_revision,
                client,
                args.session,
                checkpoint.id,
                args.question,
            )?;
            json!({"ok":true,"project_revision":event.project_revision,"wait":wait,"checkpoint":checkpoint,"event_id":event.id,"host_action":"Collect user input and call awr_session_reply; AWR does not schedule or execute the next turn."})
        }
        "awr_session_reply" => {
            let args: Reply = parse(args)?;
            let mut store = Store::open_existing(&database(root)?)?;
            let project = store.project_by_root(root)?;
            let existing = store.mcp_wait(project.id, args.wait)?;
            if existing.client != client {
                return Err(Error::RuleViolation(
                    "wait access denied for this client".into(),
                ));
            }
            let (wait, event) = store.reply_mcp_wait(
                project.id,
                args.expected_revision,
                client,
                args.wait,
                args.reply,
                args.cancel,
            )?;
            json!({"ok":true,"project_revision":event.project_revision,"wait":wait,"event_id":event.id,"host_action":"Inspect the session and compile current context; resume explicitly if a new session is needed."})
        }
        "awr_source_reindex" => {
            let args: Reindex = parse(args)?;
            let mut store = Store::open_existing(&database(root)?)?;
            let project = store.project_by_root(root)?;
            if project.project_revision != args.expected_revision {
                return Err(Error::RevisionConflict {
                    expected: args.expected_revision,
                    actual: project.project_revision,
                });
            }
            let report = awr_source::index_project(
                &mut store,
                root,
                &awr_source::Manifest::load(root)?,
                false,
            )?;
            let ok = report.ok;
            let mut value = serde_json::to_value(report)?;
            value["source_refresh_performed"] = json!(true);
            value["source_write_performed"] = json!(false);
            if !ok {
                return Ok(CallToolResult::structured_error(value));
            }
            value
        }
        _ => return Err(Error::Unsupported(name.into())),
    };
    ensure_public_value(&value)?;
    Ok(CallToolResult::structured(value))
}
