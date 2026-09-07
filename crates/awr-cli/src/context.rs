use awr_context::{BootstrapRequest, bootstrap};
use awr_core::{Error, Id, Result};
use clap::Subcommand;
use std::path::Path;

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
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
