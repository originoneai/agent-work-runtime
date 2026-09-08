use awr_core::*;
use awr_runtime::WorkActionRequest;
use awr_store::Store;
use clap::Args;
use std::path::Path;

#[derive(Debug, Args)]
pub struct ActionArgs {
    work: String,
    #[arg(long)]
    session: Id,
    #[arg(long)]
    reason: String,
    #[arg(long)]
    next_action: Option<String>,
    #[arg(long)]
    summary: Option<String>,
    #[arg(long)]
    blocker: Option<String>,
    #[arg(long)]
    expected_revision: Revision,
}
pub fn run(root: &Path, args: &ActionArgs, action: WorkAction, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let database = crate::source::runtime_dir(&root, false)?.join("state.db");
    let mut store = Store::open_existing(&database)?;
    let result = awr_runtime::perform_work_action(
        &mut store,
        &root,
        &WorkActionRequest {
            target: args.work.clone(),
            session_id: args.session,
            expected_revision: args.expected_revision,
            input: WorkActionInput {
                action,
                reason: args.reason.clone(),
                next_action: args.next_action.clone(),
                summary: args.summary.clone(),
                blocker: args.blocker.clone(),
            },
        },
    )?;
    print_result(result, action, json_output)
}
fn print_result(
    result: awr_runtime::ProposalReport,
    action: WorkAction,
    json_output: bool,
) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!(
            "Work action: {action:?}\nProposal: {} ({:?})\nRevision: {}\nSource outcome: {}\nReceipt: {}",
            result.proposal.id,
            result.proposal.status,
            result.project_revision,
            result.write_outcome,
            result.event.id
        );
        if let Some(binding) = result.proposal.bound_patch()?.work_action {
            println!("Bound transition: {:?} → {:?}", binding.from, binding.to);
        }
    }
    match result.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[derive(Debug, Args)]
pub struct CompleteArgs {
    work: String,
    #[arg(long)]
    session: Id,
    #[arg(long)]
    reason: String,
    /// JSON acceptance-to-evidence mapping (version 1, at most 64 KiB).
    #[arg(long)]
    input: std::path::PathBuf,
    #[arg(long)]
    expected_revision: Revision,
}
pub fn complete(root: &Path, args: &CompleteArgs, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let database = crate::source::runtime_dir(&root, false)?.join("state.db");
    let mut store = Store::open_existing(&database)?;
    let actual = store.project_by_root(&root)?.project_revision;
    if actual != args.expected_revision {
        return Err(Error::RevisionConflict {
            expected: args.expected_revision,
            actual,
        });
    }
    let bytes = awr_source::read_capped(&args.input, 64 * 1024)?;
    let input: CompletionInput = serde_json::from_slice(&bytes)
        .map_err(|e| Error::InvalidInput(format!("completion input: {e}")))?;
    let result = awr_runtime::complete_work(
        &mut store,
        &root,
        &awr_runtime::CompleteWorkRequest {
            target: args.work.clone(),
            session_id: args.session,
            expected_revision: args.expected_revision,
            reason: args.reason.clone(),
            input,
        },
    )?;
    print_result(result, WorkAction::Complete, json_output)
}
