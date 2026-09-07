use awr_core::*;
use awr_runtime::Runtime;
use awr_source::{Manifest, index_project};
use awr_store::Store;
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        return Err(Error::InvalidInput("usage: session_project <root> start <work> <agent> <provider> <model> | <root> end <session-id>".into()));
    }
    let root = std::path::Path::new(&args[0]).canonicalize()?;
    let mut store = Store::open(&root.join(".awr/state.db"))?;
    let report = index_project(&mut store, &root, &Manifest::load(&root)?, false)?;
    let project = store.project(report.project_id)?;
    let mut runtime = Runtime::attach(&mut store, project.id)?;
    match args[1].as_str() {
        "start" if args.len() == 6 => {
            if !report.ok {
                return Err(Error::SourceStale("source refresh is incomplete".into()));
            }
            let (started, event) = runtime.start_session(
                project.project_revision,
                SessionDraft {
                    work_item_key: Some(args[2].clone()),
                    agent_id: args[3].clone(),
                    provider: args[4].clone(),
                    model: args[5].clone(),
                    branch_id: project.current_branch_id,
                    claim: true,
                    claim_ttl_ms: Some(3_600_000),
                },
            )?;
            println!(
                "session_id={}\nclaim_id={}\nproject_revision={}",
                started.session.id,
                started.claim.unwrap().id,
                event.project_revision
            );
        }
        "end" if args.len() == 3 => {
            let (session, event) = runtime.end_session(
                project.project_revision,
                args[2]
                    .parse::<Id>()
                    .map_err(|e| Error::InvalidInput(e.to_string()))?,
                SessionOutcome::Ended,
            )?;
            println!(
                "session_id={}\nstatus={}\nproject_revision={}",
                session.id, session.status, event.project_revision
            );
        }
        _ => {
            return Err(Error::InvalidInput(
                "invalid operation or argument count".into(),
            ));
        }
    }
    Ok(())
}
