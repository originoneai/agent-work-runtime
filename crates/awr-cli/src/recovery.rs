use awr_core::*;
use clap::Subcommand;
use serde_json::json;
use std::path::Path;

#[derive(Debug, Subcommand)]
pub enum RecoveryCommand {
    /// Inspect saved continuity and verify registered executions; never resumes, reruns or kills.
    Inspect {
        #[arg(long)]
        session: Id,
    },
}
pub fn run(root: &Path, command: &RecoveryCommand, json_output: bool) -> Result<()> {
    let (store, project) = crate::execution::read_state(root)?;
    let RecoveryCommand::Inspect { session } = command;
    let session = store.session(project.id, *session)?;
    let work = session.work_item_id.ok_or_else(|| {
        Error::InvalidInput("recovery inspection requires a work-bound session".into())
    })?;
    let checkpoint = store.recovery_checkpoint(project.id, session.id)?;
    let executions =
        awr_runtime::inspect_work_executions(&store, root, project.id, work, session.branch_id)?;
    let reports = executions
        .iter()
        .map(|e| store.latest_external_report(project.id, e.execution_id))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let findings = store.inspect_runtime(project.id, now_millis()?)?.findings;
    let actual = store.project(project.id)?.project_revision;
    if actual != project.project_revision {
        return Err(Error::RevisionConflict {
            expected: project.project_revision,
            actual,
        });
    }
    let result = json!({"session":session,"checkpoint":checkpoint,"executions":executions,"external_reports":reports,"runtime_findings":findings,"project_revision":project.project_revision,"all_execution_states_verified":executions.iter().all(|e|e.verified),"observed_at":now_millis()?,"source_refresh_performed":false,"side_effects_performed":false,"next_step":"Use doctor for read-only current-source and artifact diagnostics, then session resume to refresh sources and compile the successor context. Unknown executions and incomplete writes require explicit investigation before any retry."});
    if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!(
            "Recovery inspection: session {}\nSaved checkpoint: {}\nSource facts have not been refreshed by this read-only inspection.",
            session.id,
            checkpoint
                .as_ref()
                .map(|c| c.id.to_string())
                .unwrap_or_else(|| "none".into())
        );
        if let Some(cp) = checkpoint {
            println!(
                "Saved next: {}\nOpen loops: {}",
                cp.next_action,
                serde_json::to_string(&cp.open_loops)?
            );
        }
        print!(
            "{}",
            awr_runtime::render_execution_observations(&executions)?
        );
    }
    Ok(())
}
