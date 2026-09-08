use awr_core::*;
use awr_runtime::Runtime;
use clap::Args;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub struct AppendArgs {
    #[arg(long)]
    work: Option<String>,
    #[arg(long)]
    session: Option<Id>,
    #[arg(long)]
    branch: Option<String>,
    #[arg(long = "type")]
    event_type: String,
    #[arg(long, default_value = "normal")]
    importance: String,
    #[arg(long)]
    summary: String,
    /// Optional JSON payload file, relative to the project root; at most 1 MiB.
    #[arg(long)]
    payload: Option<PathBuf>,
    #[arg(long)]
    expected_revision: Revision,
}

pub fn run(root: &Path, args: &AppendArgs, json_output: bool) -> Result<()> {
    let mut db = crate::session::RuntimeProject::for_write(root, args.expected_revision)?;
    let payload = match &args.payload {
        Some(path) => {
            let bytes = awr_source::read_capped(&db.project.root.join(path), 1024 * 1024)?;
            serde_json::from_slice(&bytes)
                .map_err(|e| Error::InvalidInput(format!("event payload: {e}")))?
        }
        None => json!({}),
    };
    let branch_id = crate::query::branch(&db.store, &db.project, args.branch.as_deref())?;
    let work_item_id = args
        .work
        .as_deref()
        .map(|key| {
            db.store
                .work_item(db.project.id, key)
                .map(|w| w.item.meta.id)
        })
        .transpose()?;
    let draft = EventDraft {
        work_item_id,
        session_id: args.session,
        branch_id,
        event_type: args.event_type.clone(),
        importance: args.importance.clone(),
        summary: args.summary.clone(),
        payload,
    };
    let mut runtime = Runtime::attach(&mut db.store, db.project.id)?;
    let event = if args.branch.is_some() {
        runtime.append_event_in_branch(args.expected_revision, draft)
    } else {
        runtime.append_event(args.expected_revision, draft)
    }?;
    if json_output {
        let mut value = db.metadata(event.project_revision);
        value["event"] = serde_json::to_value(&event)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "Event: {} ({})\nSummary: {}\nRevision: {}",
            event.id,
            event.event_type,
            crate::query::short(&event.summary),
            event.project_revision
        );
    }
    Ok(())
}
