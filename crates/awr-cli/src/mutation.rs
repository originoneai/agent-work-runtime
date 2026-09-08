use awr_core::*;
use awr_runtime::{CreateProposalRequest, ReviewProposalAction, ReviewProposalRequest};
use awr_store::Store;
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TargetKind {
    Goal,
    Plan,
    Rule,
    #[value(alias = "work_item", alias = "work-item")]
    Work,
    Decision,
    Evidence,
}
impl From<TargetKind> for EntityKind {
    fn from(value: TargetKind) -> Self {
        match value {
            TargetKind::Goal => Self::Goal,
            TargetKind::Plan => Self::Plan,
            TargetKind::Rule => Self::Rule,
            TargetKind::Work => Self::WorkItem,
            TargetKind::Decision => Self::Decision,
            TargetKind::Evidence => Self::Evidence,
        }
    }
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Status {
    Draft,
    Ready,
    Approved,
    Applied,
    Conflict,
    Rejected,
    Failed,
}
impl From<Status> for ProposalStatus {
    fn from(value: Status) -> Self {
        match value {
            Status::Draft => Self::Draft,
            Status::Ready => Self::Ready,
            Status::Approved => Self::Approved,
            Status::Applied => Self::Applied,
            Status::Conflict => Self::Conflict,
            Status::Rejected => Self::Rejected,
            Status::Failed => Self::Failed,
        }
    }
}
#[derive(Debug, Args)]
pub struct CreateArgs {
    #[arg(long, value_enum)]
    kind: TargetKind,
    #[arg(long)]
    target: String,
    #[arg(long)]
    intent: String,
    /// JSON object of proposed field replacements, e.g. {"next_action":"Review the report"}.
    #[arg(
        long,
        conflicts_with = "patch_file",
        required_unless_present = "patch_file"
    )]
    patch: Option<String>,
    /// Read proposed field replacements from a JSON file, capped at 64 KiB.
    #[arg(long, conflicts_with = "patch", required_unless_present = "patch")]
    patch_file: Option<PathBuf>,
    #[arg(long)]
    session: Option<Id>,
    #[arg(long)]
    expected_revision: Revision,
}
#[derive(Debug, Args)]
pub struct ReviewArgs {
    id: Id,
    /// Caller-reported reviewer identity; this is not an authentication or independent-review proof.
    #[arg(long)]
    actor: String,
    #[arg(long)]
    reason: String,
    #[arg(long)]
    expected_revision: Revision,
}
#[derive(Debug, Subcommand)]
pub enum ProposalCommand {
    /// Verify current source bytes and bind one immutable draft to an exact indexed target.
    Create(CreateArgs),
    /// Read retained proposal summaries without refreshing or requiring source files.
    List {
        #[arg(long, value_enum)]
        status: Option<Status>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Inspect the immutable binding; --full includes all proposed field values.
    Show {
        id: Id,
        #[arg(long)]
        full: bool,
    },
    /// Recheck the bound source and submit a draft for review.
    Submit(ReviewArgs),
    /// Recheck the bound source and approve a ready proposal.
    Approve(ReviewArgs),
    /// Reject an open proposal, including when its source is unavailable.
    Reject(ReviewArgs),
    /// Check an approved proposal; unavailable writers return proposal_required without changing files.
    Apply(ReviewArgs),
}
fn summary(p: &MutationProposal) -> Value {
    let binding = p.bound_patch().ok();
    json!({"id":p.id,"project_id":p.project_id,"source_id":p.source_id,"work_item_id":p.work_item_id,
        "base_fingerprint":p.base_fingerprint,"expected_revision":p.expected_revision,"revision":p.revision,
        "status":p.status,"mutation_type":p.mutation_type,"created_by_session":p.created_by_session,
        "binding_valid":binding.is_some(),"target":binding.as_ref().map(|b|&b.target),
        "intent":binding.as_ref().map(|b|crate::query::short(&b.intent)),
        "fields":binding.as_ref().and_then(|b|b.changes.as_object()).map(|c|c.keys().collect::<Vec<_>>()),
        "patch_included":false})
}
fn print(value: &Value, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else if let Some(rows) = value.get("proposals").and_then(Value::as_array) {
        for row in rows {
            println!(
                "{} {} {}",
                row["id"].as_str().unwrap_or(""),
                row["status"].as_str().unwrap_or(""),
                row["intent"].as_str().unwrap_or("")
            );
        }
        println!("Revision: {}", value["project_revision"]);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}
pub fn run(root: &Path, command: &ProposalCommand, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let database = crate::source::runtime_dir(&root, false)?.join("state.db");
    let read_only = matches!(
        command,
        ProposalCommand::List { .. } | ProposalCommand::Show { .. }
    );
    let mut store = if read_only {
        Store::open_readonly(&database)?
    } else {
        Store::open_existing(&database)?
    };
    let project = store.project_by_root(&root)?;
    let result = match command {
        ProposalCommand::Create(args) => {
            let bytes = if let Some(path) = &args.patch_file {
                // Relative patch files follow the selected project, like source command paths.
                awr_source::read_capped(&root.join(path), 64 * 1024)?
            } else {
                let value = args.patch.as_deref().unwrap_or("");
                if value.len() > 64 * 1024 {
                    return Err(Error::InvalidInput("proposal patch exceeds 64 KiB".into()));
                }
                value.as_bytes().to_vec()
            };
            awr_runtime::create_proposal(
                &mut store,
                &root,
                &CreateProposalRequest {
                    kind: args.kind.into(),
                    target: args.target.clone(),
                    intent: args.intent.clone(),
                    changes: serde_json::from_slice(&bytes)?,
                    session_id: args.session,
                    expected_revision: args.expected_revision,
                },
            )?
        }
        ProposalCommand::List { status, limit } => {
            let proposals = store.proposals(project.id, status.map(Into::into), *limit)?;
            let actual = store.project(project.id)?.project_revision;
            if actual != project.project_revision {
                return Err(Error::RevisionConflict {
                    expected: project.project_revision,
                    actual,
                });
            }
            return print(
                &json!({"ok":true,"read_only":true,"source_refresh_performed":false,
                "project_revision":actual,"proposals":proposals.iter().map(summary).collect::<Vec<_>>(),
                "limit":limit,"may_have_more":proposals.len()==*limit}),
                json_output,
            );
        }
        ProposalCommand::Show { id, full } => {
            let proposal = store.proposal(project.id, *id)?;
            let actual = store.project(project.id)?.project_revision;
            if actual != project.project_revision {
                return Err(Error::RevisionConflict {
                    expected: project.project_revision,
                    actual,
                });
            }
            return print(
                &json!({"ok":true,"read_only":true,"source_refresh_performed":false,
                "project_revision":actual,"patch_included":full,"proposal":if *full {serde_json::to_value(proposal)?} else {summary(&proposal)}}),
                json_output,
            );
        }
        ProposalCommand::Submit(args)
        | ProposalCommand::Approve(args)
        | ProposalCommand::Reject(args)
        | ProposalCommand::Apply(args) => {
            let action = match command {
                ProposalCommand::Submit(_) => ReviewProposalAction::Submit,
                ProposalCommand::Approve(_) => ReviewProposalAction::Approve,
                ProposalCommand::Reject(_) => ReviewProposalAction::Reject,
                _ => ReviewProposalAction::Apply,
            };
            awr_runtime::review_proposal(
                &mut store,
                &root,
                &ReviewProposalRequest {
                    proposal_id: args.id,
                    expected_revision: args.expected_revision,
                    action,
                    actor: args.actor.clone(),
                    reason: args.reason.clone(),
                },
            )?
        }
    };
    print(&serde_json::to_value(&result)?, json_output)?;
    match result.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
