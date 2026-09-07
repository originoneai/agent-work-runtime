use awr_core::*;
use awr_runtime::{ArtifactFile, Runtime};
use awr_source::{Manifest, index_project};
use awr_store::Store;
use sha2::{Digest, Sha256};
use std::io::Write;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(Error::InvalidInput(
            "usage: checkpoint_project <root> <current work key>".into(),
        ));
    }
    let root = std::path::Path::new(&args[0]).canonicalize()?;
    let mut store = Store::open(&root.join(".awr/state.db"))?;
    let report = index_project(&mut store, &root, &Manifest::load(&root)?, false)?;
    if !report.ok {
        return Err(Error::SourceStale("source refresh incomplete".into()));
    }
    let project = store.project(report.project_id)?;
    let (started, started_event) = Runtime::attach(&mut store, project.id)?.start_session(
        project.project_revision,
        SessionDraft {
            work_item_key: Some(args[1].clone()),
            agent_id: "codex-primary".into(),
            provider: "openai".into(),
            model: "gpt-6".into(),
            branch_id: project.current_branch_id,
            claim: true,
            claim_ttl_ms: Some(3_600_000),
        },
    )?;
    // A concrete caller-supplied state snapshot. Full ContextCompiler output is a later milestone.
    let context = format!(
        "project_revision={}\nsession={}\nwork={:?}\n",
        started_event.project_revision,
        started.session.id,
        store.work_item(project.id, &args[1])?.item
    );
    std::fs::create_dir_all(root.join(".local"))?;
    let path = root.join(format!(
        ".local/checkpoint-input-{}.txt",
        started.session.id
    ));
    let mut input = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    input.write_all(context.as_bytes())?;
    input.sync_all()?;
    drop(input);
    let (checkpoint,checkpoint_event)=Runtime::attach(&mut store,project.id)?.checkpoint(started_event.project_revision,started.session.id,CheckpointDraft {context_hash:format!("{:x}",Sha256::digest(context.as_bytes())),digest:"Checkpoint persistence and bounded artifact import implemented; direct checks passed.".into(),next_action:"Wire session, history and handoff CLI commands.".into(),open_loops:vec!["Session CLI and full ContextCompiler remain scheduled.".into()],changed_entities:vec!["runtime/checkpoint".into(),"runtime/artifact".into()]})?;
    let (artifact, artifact_event) = Runtime::attach(&mut store, project.id)?.import_artifact(
        checkpoint_event.project_revision,
        ArtifactFile {
            path,
            artifact_type: "caller_state_snapshot".into(),
            mime: "text/plain; charset=utf-8".into(),
            source_event_id: checkpoint_event.id,
            max_bytes: 1024 * 1024,
        },
    )?;
    assert_eq!(checkpoint.context_hash, artifact.sha256);
    let (session, ended) = Runtime::attach(&mut store, project.id)?.end_session(
        artifact_event.project_revision,
        started.session.id,
        SessionOutcome::Ended,
    )?;
    let restored = store.latest_checkpoint(project.id, session.id)?.unwrap();
    assert_eq!(restored.id, checkpoint.id);
    println!(
        "session_id={}\ncheckpoint_id={}\nartifact_id={}\ncontext_hash={}\nartifact_locator={}\nproject_revision={}\nnext_action={}",
        session.id,
        checkpoint.id,
        artifact.id,
        checkpoint.context_hash,
        artifact.locator,
        ended.project_revision,
        restored.next_action
    );
    Ok(())
}
