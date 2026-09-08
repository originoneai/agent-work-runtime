use awr_context::{
    BootstrapRequest, ContextRequest, DeltaBaseline, DeltaContextRequest, DeltaRequest, bootstrap,
    compile_context, context_delta,
};
use awr_core::{Error, Id, Result};
use clap::Subcommand;
use std::path::Path;

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Refresh sources and summarize changes since a checkpoint, revision or selected session.
    Delta {
        #[arg(long)]
        work: Option<String>,
        #[arg(long)]
        session: Option<Id>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, conflicts_with = "after_revision")]
        checkpoint: Option<Id>,
        #[arg(long, conflicts_with = "checkpoint")]
        after_revision: Option<u64>,
        #[arg(long, default_value_t = 12)]
        event_limit: usize,
        #[arg(long, default_value_t = 24)]
        entity_limit: usize,
    },
    /// Compile current work, hard facts, required context and explicit gaps within a token budget.
    Compile {
        #[arg(long)]
        work: Option<String>,
        #[arg(long)]
        session: Option<Id>,
        /// Compile without selecting an active runtime session.
        #[arg(long)]
        detached: bool,
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
        ContextCommand::Delta {
            work,
            session,
            agent,
            checkpoint,
            after_revision,
            event_limit,
            entity_limit,
        } => {
            let report = context_delta(
                &mut db.store,
                root,
                &DeltaContextRequest {
                    work_item_key: work.clone(),
                    agent_id: agent.clone(),
                    delta: DeltaRequest {
                        session_id: *session,
                        event_limit: *event_limit,
                        entity_limit_per_source: *entity_limit,
                        baseline: if let Some(id) = checkpoint {
                            DeltaBaseline::Checkpoint { id: *id }
                        } else if let Some(revision) = after_revision {
                            DeltaBaseline::Revision {
                                revision: *revision,
                            }
                        } else {
                            DeltaBaseline::Auto
                        },
                    },
                },
            )?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                // This bounded structured delta contains summaries and references, never event bodies.
                println!("{}", serde_json::to_string_pretty(&report)?);
                println!(
                    "Drill down with source history <source-id>, event show <event-id> --full, or object show <kind> <id> --full."
                );
            }
            if !report.source_refresh_ok {
                return Err(Error::SourceStale(
                    "delta contains source refresh issues; inspect source_issues".into(),
                ));
            }
            Ok(())
        }
        ContextCommand::Compile {
            work,
            session,
            detached,
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
                    detached: *detached,
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
            print_l1(
                &report,
                db.store.project(db.project.id)?.project_revision,
                json_output,
            )
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

pub(crate) fn print_l1(
    report: &awr_context::WorkContextReport,
    revision: awr_core::Revision,
    json_output: bool,
) -> Result<()> {
    if revision != report.completeness.project_revision {
        return Err(Error::RevisionConflict {
            expected: report.completeness.project_revision,
            actual: revision,
        });
    }
    let error = (!report.completeness.complete).then(|| {
        Error::ContextIncomplete(
            "L1 has required gaps; inspect completeness before execution".into(),
        )
    });
    if json_output {
        let mut value = serde_json::to_value(report)?;
        value["ok"] = serde_json::json!(error.is_none());
        value["project_revision"] = serde_json::json!(revision);
        value["freshness_basis"] = serde_json::json!("source_refresh");
        value["source_refresh_performed"] = serde_json::json!(true);
        value["read_only"] = serde_json::json!(false);
        if let Some(error) = &error {
            value["error"] = serde_json::json!(error.report());
        }
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        print!("{}", report.rendered_context());
    }
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
