use awr_context::{BootstrapRequest, ContextRequest, DeltaBaseline, bootstrap, compile_context};
use awr_core::{Error, Id, Result};
use clap::Subcommand;
use std::path::Path;

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Compile current work, hard facts, required context and explicit gaps within a token budget.
    Compile {
        #[arg(long)]
        work: Option<String>,
        #[arg(long)]
        session: Option<Id>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        branch: Option<Id>,
        #[arg(long)]
        goal: Vec<String>,
        #[arg(long)]
        path: Option<Vec<String>>,
        #[arg(long)]
        tag: Option<Vec<String>>,
        #[arg(long)]
        source_sha: Option<String>,
        #[arg(long, default_value = "work")]
        intent: String,
        #[arg(long, default_value_t = 5000)]
        budget: usize,
        #[arg(long, conflicts_with = "after_revision")]
        checkpoint: Option<Id>,
        #[arg(long, conflicts_with = "checkpoint")]
        after_revision: Option<u64>,
    },
    /// Restore current work, critical rules and last checkpoint; compile L1 before execution.
    Bootstrap {
        #[arg(long)]
        work: Option<String>,
        #[arg(long)]
        session: Option<Id>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, default_value_t = 1000)]
        budget: usize,
    },
}

pub fn run(root: &Path, command: &ContextCommand, json_output: bool) -> Result<()> {
    let mut db = crate::session::RuntimeProject::open(root, false)?;
    match command {
        ContextCommand::Compile {
            work,
            session,
            agent,
            branch,
            goal,
            path,
            tag,
            source_sha,
            intent,
            budget,
            checkpoint,
            after_revision,
        } => {
            let report = compile_context(
                &mut db.store,
                root,
                &ContextRequest {
                    work_item_key: work.clone(),
                    session_id: *session,
                    agent_id: agent.clone(),
                    branch_id: *branch,
                    goal_keys: goal.clone(),
                    paths: path.clone(),
                    tags: tag.clone(),
                    source_sha: source_sha.clone(),
                    intent: intent.clone(),
                    token_budget: *budget,
                    delta_baseline: if let Some(id) = checkpoint {
                        DeltaBaseline::Checkpoint { id: *id }
                    } else if let Some(revision) = after_revision {
                        DeltaBaseline::Revision {
                            revision: *revision,
                        }
                    } else {
                        DeltaBaseline::Auto
                    },
                },
            )?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.rendered_context());
            }
            if !report.completeness.complete {
                return Err(Error::ContextIncomplete(
                    "L1 context contains required gaps; inspect completeness and sources".into(),
                ));
            }
            Ok(())
        }
        ContextCommand::Bootstrap {
            work,
            session,
            agent,
            budget,
        } => {
            let pack = bootstrap(
                &mut db.store,
                root,
                &BootstrapRequest {
                    work_item_key: work.clone(),
                    session_id: *session,
                    agent_id: agent.clone(),
                    token_budget: *budget,
                },
            )?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&pack)?);
            } else {
                print!("{}", pack.rendered_context);
            }
            if !pack.context.complete {
                return Err(Error::ContextIncomplete("bootstrap contains explicit gaps; inspect the returned context before proceeding".into()));
            }
            Ok(())
        }
    }
}
