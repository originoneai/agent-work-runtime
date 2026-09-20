//! The exchange plane, as commands.
//!
//! Nothing here is authority: `status` reads, `publish` sends this host's
//! tracked files, `sync` takes the peer's. A conflict is reported as a typed
//! error with exit 1, never resolved by guessing which side is newer.
use awr_core::{Error, Result};
use awr_workspace::{
    config::{self, WorkspaceConfig},
    credentials::{self, CredentialStatus, Credentials},
    sync::{DEFAULT_HANDOFF_DIR, Workspace},
};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    /// The per-machine config; a relative path resolves against the project root.
    #[arg(long, global = true, default_value = DEFAULT_CONFIG)]
    config: PathBuf,
    #[command(subcommand)]
    command: WorkspaceCommand,
}

/// The config file a project has by default, at its root.
pub const DEFAULT_CONFIG: &str = "remote_workspace.toml";

/// How many paths a session-start notice lists before it counts the rest.
const NOTICE_PATHS: usize = 24;

/// How long a session start may spend on the exchange plane.
///
/// The hook runs before the session can read anything, so a store that never
/// answers must not hold it open. The pull runs on its own thread and is
/// abandoned when this budget runs out: what was already fetched stays on disk
/// and the next sync takes the rest. The store's own request timeout is 60s,
/// which is right for an operator who is watching and wrong for a session start.
const NOTICE_BUDGET: Duration = Duration::from_secs(10);

/// The workspace exchange plane: the files a project shares between machines
/// that do not share a filesystem.
#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// Store and inspect the object-store credentials this machine uses.
    Credential {
        #[command(subcommand)]
        command: CredentialCommand,
    },
    /// Per-file state: what this host has, what the workspace has, what differs.
    Status {
        /// Read the per-file pointers instead of the manifest.
        #[arg(long)]
        pointers: bool,
    },
    /// Send this host's tracked files to the workspace.
    Publish {
        /// Report what would be sent without sending it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Take the peer's tracked files and inbound handoffs.
    Sync {
        /// Where inbound handoffs are written; defaults to <root>/infra/handoffs.
        #[arg(long)]
        handoff_outdir: Option<PathBuf>,
    },
    /// Remove paths from the workspace index. Local files stay; remove them
    /// from project.track first.
    Drop {
        /// A project-relative path to stop sharing. Repeatable.
        #[arg(long = "path", required = true)]
        paths: Vec<String>,
    },
    /// Compare the manifest with the per-file pointer mirror.
    VerifyIndex,
    /// Publish or collect a handoff between agents.
    Handoff {
        #[command(subcommand)]
        command: HandoffCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum HandoffCommand {
    /// Publish a handoff under this host's name; handoffs are write-once.
    Push {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        name: String,
    },
    /// Collect handoffs written by other hosts, skipping ones already read.
    Pull {
        #[arg(long)]
        outdir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum CredentialCommand {
    /// Read credentials from a file or stdin; values are never command arguments.
    Set {
        /// A JSON file holding access_key, secret_key and an optional session_token.
        #[arg(long, conflicts_with = "stdin")]
        input: Option<PathBuf>,
        /// Read the same JSON from stdin instead.
        #[arg(long)]
        stdin: bool,
    },
    /// Report which fields are stored and how the file is protected, never their values.
    Status,
    /// Remove the stored credentials from this machine.
    Clear,
}

pub fn run(project: &Path, args: &WorkspaceArgs, json_output: bool) -> Result<()> {
    let root = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    match &args.command {
        WorkspaceCommand::Credential { command } => credential(&root, command, json_output),
        _ => {
            let config_path = resolve(&root, &args.config);
            let config = config::load(&config_path, &root)?;
            exchange(&config, &args.command, json_output)
        }
    }
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn open(config: &WorkspaceConfig) -> Result<Workspace> {
    let backend = awr_workspace::open_backend(config)?;
    Workspace::open(config, backend)
}

/// The exchange plane's automatic side: what a starting session should know
/// about the other machines.
///
/// A lifecycle hook runs before the session can react to anything, so this
/// never fails it. A project with no config says nothing at all; a project with
/// one but no reachable store, no credentials, or a conflict comes back as text
/// for the agent instead of an error. Conflicts are reported and left alone -
/// the same rule the commands follow - because a hook that resolved them would
/// be guessing on the operator's behalf.
///
/// `None` means there is nothing worth adding to the context.
pub fn session_start(root: &Path) -> Option<String> {
    let path = resolve(root, Path::new(DEFAULT_CONFIG));
    if !path.exists() {
        return None;
    }
    let root = root.to_path_buf();
    match within_budget(NOTICE_BUDGET, move |cancel| notice(&root, &path, cancel)) {
        Some(Ok(notice)) => notice,
        Some(Err(error)) => Some(format!(
            "Workspace exchange is configured but did not run: {error}\nThe session continues. \
             Run `awr workspace status` once the store is reachable, and re-publish what this \
             host changed while it was down."
        )),
        None => Some(format!(
            "Workspace exchange was still answering after {}s and has been left for later; the \
             session continues. Run `awr workspace sync` when the store is reachable.",
            NOTICE_BUDGET.as_secs()
        )),
    }
}

/// Run `job` on its own thread and stop waiting for it after `budget`.
///
/// When the budget elapses the shared cancel flag is set so the worker stops
/// before further local writes. A blocked network call cannot be preempted, but
/// pull/sync check the flag before each filesystem mutation and before saving
/// workspace state, so a timed-out SessionStart does not keep rewriting sources
/// after the agent has already been told the exchange was left for later.
fn within_budget<F>(budget: Duration, job: F) -> Option<Result<Option<String>>>
where
    F: FnOnce(Arc<AtomicBool>) -> Result<Option<String>> + Send + 'static,
{
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker_cancel = Arc::clone(&cancel);
    std::thread::spawn(move || {
        let _ = sender.send(job(worker_cancel));
    });
    match receiver.recv_timeout(budget) {
        Ok(result) => Some(result),
        Err(_) => {
            cancel.store(true, Ordering::Release);
            None
        }
    }
}

/// Take the peer's files and handoffs, and describe what moved.
fn notice(root: &Path, path: &Path, cancel: Arc<AtomicBool>) -> Result<Option<String>> {
    let config = config::load(path, root)?;
    let mut workspace = open(&config)?.with_cancel(cancel);
    let outdir = workspace.root().join(DEFAULT_HANDOFF_DIR);
    let report = workspace.sync(&outdir)?;
    // An entry this host published can be dropped by a peer committing from a
    // stale read - the loss a store without compare-and-swap cannot prevent,
    // only report. Registering it again here is what makes "accept and heal"
    // automatic instead of a command someone has to remember. It costs one read
    // of the index when there is nothing to heal.
    let repaired = workspace.repair_index()?;
    Ok(notice_lines(&report, &repaired, &outdir))
}

/// What a session start has to say about the exchange plane, or `None` when
/// there is nothing worth saying.
///
/// A repair that lost the race to a peer is said out loud rather than passed
/// over: those entries are still missing from the index, so this time the
/// automatic heal did not happen, and the session is the only place left that
/// can still ask for the one publish that fixes it.
fn notice_lines(
    report: &awr_workspace::sync::SyncReport,
    repaired: &awr_workspace::sync::PublishReport,
    outdir: &Path,
) -> Option<String> {
    if !report.changed
        && report.handoffs.is_empty()
        && report.conflicts.is_empty()
        && repaired.pushed.is_empty()
        && repaired.contended.is_empty()
    {
        return None;
    }
    let mut lines = Vec::new();
    if !repaired.pushed.is_empty() {
        lines.push(format!(
            "{} file(s) this host had published had dropped out of the index and were registered \
             again; the bytes were already in the store, so nothing was uploaded:",
            repaired.pushed.len()
        ));
        lines.extend(list(
            &repaired
                .pushed
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
        ));
    }
    if !repaired.contended.is_empty() {
        lines.push(format!(
            "{} entry/entries this host had published are still missing from the index: another \
             host committed it first on every attempt, so this session could not register them \
             again. Nothing was overwritten - run `awr workspace publish` to send them:",
            repaired.contended.len()
        ));
        lines.extend(list(
            &repaired
                .contended
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        ));
    }
    if report.changed {
        lines.push(format!(
            "The workspace moved {} file(s) while this session was not running; they are on disk \
             now and this reading of them is current, but anything already read into this \
             conversation may not be:",
            report.pulled.len()
        ));
        lines.extend(list(
            &report
                .pulled
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
        ));
    }
    if !report.handoffs.is_empty() {
        lines.push(format!(
            "{} inbound handoff(s) were written under {}:",
            report.handoffs.len(),
            outdir.display()
        ));
        // Under the directory that was just named, so a handoff reads like the
        // tracked paths do - relative to this project, not to this machine.
        let paths: Vec<String> = report
            .handoffs
            .iter()
            .map(|item| {
                Path::new(&item.path)
                    .strip_prefix(&outdir)
                    .unwrap_or_else(|_| Path::new(&item.path))
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect();
        lines.extend(list(&paths.iter().map(String::as_str).collect::<Vec<_>>()));
    }
    if !report.conflicts.is_empty() {
        lines.push(format!(
            "{} file(s) changed on this host and on another one, so neither side was touched. \
             Decide by hand - publish and sync both refuse to guess:",
            report.conflicts.len()
        ));
        lines.extend(list(
            &report
                .conflicts
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
        ));
    }
    lines.push(
        "Run `awr workspace status` for the per-file picture, `awr workspace publish` to send \
         this host's changes."
            .to_string(),
    );
    Some(lines.join("\n"))
}

fn list(paths: &[&str]) -> Vec<String> {
    let mut lines: Vec<String> = paths
        .iter()
        .take(NOTICE_PATHS)
        .map(|path| format!("  {path}"))
        .collect();
    if paths.len() > NOTICE_PATHS {
        lines.push(format!("  ... and {} more", paths.len() - NOTICE_PATHS));
    }
    lines
}

fn exchange(config: &WorkspaceConfig, command: &WorkspaceCommand, json_output: bool) -> Result<()> {
    let started = Instant::now();
    match command {
        WorkspaceCommand::Credential { .. } => unreachable!("credential is handled separately"),
        WorkspaceCommand::Status { pointers } => {
            let workspace = open(config)?;
            let report = workspace.status(*pointers)?;
            emit(
                serde_json::to_value(&report)?,
                &workspace,
                started,
                json_output,
                &render_status(&workspace, &report),
            )
        }
        WorkspaceCommand::VerifyIndex => {
            let workspace = open(config)?;
            let report = workspace.verify_index()?;
            emit(
                serde_json::to_value(&report)?,
                &workspace,
                started,
                json_output,
                &render_verify(&report),
            )
        }
        WorkspaceCommand::Publish { dry_run } => {
            let mut workspace = open(config)?;
            let report = if *dry_run {
                workspace.publish_preview()?
            } else {
                workspace.publish()?
            };
            let summary = render_publish(&report);
            emit(
                serde_json::to_value(&report)?,
                &workspace,
                started,
                json_output,
                &summary,
            )?;
            if *dry_run {
                return Ok(());
            }
            if !report.conflicts.is_empty() {
                return Err(conflict(&report.conflicts));
            }
            if !report.contended.is_empty() {
                return Err(contended(&report.contended));
            }
            Ok(())
        }
        WorkspaceCommand::Drop { paths } => {
            let mut workspace = open(config)?;
            let report = workspace.drop_paths(paths)?;
            let summary = render_drop(&report);
            emit(
                serde_json::to_value(&report)?,
                &workspace,
                started,
                json_output,
                &summary,
            )?;
            if !report.contended.is_empty() {
                return Err(contended(&report.contended));
            }
            Ok(())
        }
        WorkspaceCommand::Sync { handoff_outdir } => {
            let mut workspace = open(config)?;
            let outdir = handoff_outdir
                .clone()
                .map(|path| resolve(workspace.root(), &path))
                .unwrap_or_else(|| workspace.root().join(DEFAULT_HANDOFF_DIR));
            let report = workspace.sync(&outdir)?;
            let summary = render_sync(&report);
            emit(
                serde_json::to_value(&report)?,
                &workspace,
                started,
                json_output,
                &summary,
            )?;
            if !report.conflicts.is_empty() {
                // The body above already lists them, so the error only has to
                // make the exit code fail rather than explain from scratch.
                return Err(conflict(&report.conflicts));
            }
            Ok(())
        }
        WorkspaceCommand::Handoff { command } => {
            let mut workspace = open(config)?;
            match command {
                HandoffCommand::Push { file, name } => {
                    let file = resolve(workspace.root(), file);
                    let report = workspace.handoff_push(&file, name)?;
                    emit(
                        serde_json::to_value(&report)?,
                        &workspace,
                        started,
                        json_output,
                        &if report.idempotent_replay {
                            format!(
                                "Handoff {} was already published; nothing changed",
                                report.key
                            )
                        } else {
                            format!("Handoff published: {}", report.key)
                        },
                    )
                }
                HandoffCommand::Pull { outdir } => {
                    let outdir = resolve(workspace.root(), outdir);
                    let report = workspace.handoff_pull(&outdir)?;
                    let body = json!({"handoffs": report, "inbound_count": report.len()});
                    let summary = if report.is_empty() {
                        "No new handoffs".to_string()
                    } else {
                        let mut lines = format!("{} inbound handoff(s):", report.len());
                        for item in &report {
                            lines.push_str(&format!("\n  {}", item.path));
                        }
                        lines
                    };
                    emit(body, &workspace, started, json_output, &summary)
                }
            }
        }
    }
}

/// Print the report, then the two numbers that make its cost observable.
fn emit(
    report: Value,
    workspace: &Workspace,
    started: Instant,
    json_output: bool,
    human: &str,
) -> Result<()> {
    let elapsed = started.elapsed().as_millis() as u64;
    if json_output {
        let mut report = report;
        if let Some(object) = report.as_object_mut() {
            object.insert("backend".into(), json!(workspace.backend_label()));
            object.insert("backend_requests".into(), json!(workspace.requests()));
            object.insert("elapsed_ms".into(), json!(elapsed));
        }
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{human}");
        println!(
            "  backend {} | {} request(s) | {}ms",
            workspace.backend_label(),
            workspace.requests(),
            elapsed
        );
    }
    Ok(())
}

fn conflict(conflicts: &[awr_workspace::sync::Conflict]) -> Error {
    let paths: Vec<&str> = conflicts.iter().map(|item| item.path.as_str()).collect();
    Error::WorkspaceConflict(format!(
        "{} tracked file(s) changed on both sides, so nothing was overwritten: {}",
        conflicts.len(),
        paths.join(", ")
    ))
}

/// Losing every commit attempt is not a conflict: nobody else touched these
/// paths, so there is nothing to resolve, only another publish. Saying
/// "changed on both sides" here would send an operator to mediate a change that
/// does not exist.
fn contended(paths: &[String]) -> Error {
    Error::WorkspaceContended(format!(
        "{} file(s) were left unpublished because another host committed the index every time: {}. \
         Nothing was overwritten and nothing is lost - publish again.",
        paths.len(),
        paths.join(", ")
    ))
}

fn render_status(workspace: &Workspace, report: &awr_workspace::sync::StatusReport) -> String {
    let mut out = format!(
        "Workspace {} on {}: {} file(s), index {}, {} conflicted",
        report.project_key,
        workspace.host(),
        report.files.len(),
        report.index_source,
        report.conflicted.len()
    );
    for row in &report.files {
        if row.state == "in_sync" {
            continue;
        }
        out.push_str(&format!(
            "\n  {:<22} {} (local {} remote {})",
            row.state, row.path, row.local, row.remote
        ));
    }
    out
}

fn render_publish(report: &awr_workspace::sync::PublishReport) -> String {
    let mut out = format!(
        "{} {} file(s), {} unchanged, {} conflict(s), commit {}",
        if report.dry_run {
            "Would publish"
        } else {
            "Published"
        },
        report.pushed.len(),
        report.unchanged.len(),
        report.conflicts.len(),
        report.commit_mode
    );
    for file in &report.pushed {
        out.push_str(&format!(
            "\n  {} {} ({} bytes{})",
            &file.sha256[..12.min(file.sha256.len())],
            file.path,
            file.size,
            if file.content_uploaded {
                ""
            } else {
                ", content already present"
            }
        ));
    }
    if !report.republished_after_index_drop.is_empty() {
        out.push_str(&format!(
            "\n  re-published after the index lost {} entry/entries",
            report.republished_after_index_drop.len()
        ));
    }
    if !report.contended.is_empty() {
        out.push_str(&format!(
            "\n  left unpublished, another host committed the index first: {}",
            report.contended.join(", ")
        ));
    }
    out
}

fn render_drop(report: &awr_workspace::sync::DropReport) -> String {
    let mut out = format!(
        "Dropped {} path(s) from the workspace index, {} absent, commit {}",
        report.dropped.len(),
        report.skipped_absent.len(),
        report.commit_mode
    );
    for path in &report.dropped {
        out.push_str(&format!("\n  {path}"));
    }
    if !report.skipped_absent.is_empty() {
        out.push_str(&format!(
            "\n  not in the index: {}",
            report.skipped_absent.join(", ")
        ));
    }
    if !report.contended.is_empty() {
        out.push_str(&format!(
            "\n  left in the index, another host committed first: {}",
            report.contended.join(", ")
        ));
    }
    out
}

fn render_sync(report: &awr_workspace::sync::SyncReport) -> String {
    let mut out = format!(
        "Synced {} file(s) and {} handoff(s)",
        report.pulled.len(),
        report.inbound_count
    );
    for file in &report.pulled {
        out.push_str(&format!(
            "\n  {} {}",
            &file.sha256[..12.min(file.sha256.len())],
            file.path
        ));
    }
    for handoff in &report.handoffs {
        out.push_str(&format!("\n  handoff {}", handoff.path));
    }
    out
}

fn render_verify(report: &awr_workspace::sync::VerifyIndexReport) -> String {
    let mut out = format!(
        "Index: manifest {} file(s), pointer mirror {} file(s), {} drift",
        report.manifest_files, report.pointer_files, report.drift_count
    );
    for item in &report.drift {
        out.push_str(&format!(
            "\n  {} manifest {} pointer {}",
            item.path, item.manifest, item.pointer
        ));
    }
    out
}

fn credentials_path(project: &Path) -> PathBuf {
    project.join(credentials::DEFAULT_PATH)
}

fn read_input(input: Option<&PathBuf>, from_stdin: bool, project: &Path) -> Result<Credentials> {
    let body = match (input, from_stdin) {
        (Some(path), _) => std::fs::read(resolve(project, path))?,
        (None, true) => {
            let mut body = Vec::new();
            std::io::stdin().read_to_end(&mut body)?;
            body
        }
        (None, false) => {
            return Err(Error::InvalidInput(
                "credential set reads from --input <path> or --stdin; \
                 values are not accepted as command arguments"
                    .into(),
            ));
        }
    };
    serde_json::from_slice(&body).map_err(|_| {
        Error::InvalidInput(
            "credentials require JSON with access_key, secret_key and an optional session_token"
                .into(),
        )
    })
}

fn render_credentials(status: &CredentialStatus, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(status)?);
        return Ok(());
    }
    if !status.present {
        println!(
            "Workspace credentials: none at {} (run `awr workspace credential set --stdin`)",
            status.path
        );
        return Ok(());
    }
    println!("Workspace credentials: {}", status.path);
    for (label, present) in [
        ("access_key", status.access_key),
        ("secret_key", status.secret_key),
        ("session_token", status.session_token),
    ] {
        println!(
            "  {label:<14}{}",
            if present { "present" } else { "absent" }
        );
    }
    if let Some(mode) = &status.mode {
        println!("  {:<14}{mode}", "file mode");
    }
    if !status.complete {
        println!("  incomplete: both access_key and secret_key are required");
    }
    Ok(())
}

fn credential(project: &Path, command: &CredentialCommand, json_output: bool) -> Result<()> {
    let path = credentials_path(project);
    match command {
        CredentialCommand::Set { input, stdin } => {
            let credentials = read_input(input.as_ref(), *stdin, project)?;
            credentials::save(&path, &credentials)?;
            let status = credentials::status(&path)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!("Workspace credentials stored: {}", status.path);
                println!("  Nothing was echoed; inspect with `awr workspace credential status`.");
            }
            Ok(())
        }
        CredentialCommand::Status => render_credentials(&credentials::status(&path)?, json_output),
        CredentialCommand::Clear => {
            let removed = credentials::clear(&path)?;
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "path": path.display().to_string(),
                        "removed": removed,
                    }))?
                );
            } else if removed {
                println!("Workspace credentials removed: {}", path.display());
            } else {
                println!("Workspace credentials: nothing to remove");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::sync::atomic::AtomicUsize;

    #[derive(Debug, Parser)]
    #[command(name = "awr")]
    struct Harness {
        #[command(subcommand)]
        command: Top,
    }

    #[derive(Debug, Subcommand)]
    enum Top {
        Workspace(WorkspaceArgs),
    }

    /// `--config` has to be accepted on either side of the subcommand, because
    /// half the team will type one and half the other.
    #[test]
    fn the_config_flag_is_accepted_before_and_after_the_subcommand() {
        for args in [
            vec!["awr", "workspace", "--config", "w.toml", "status"],
            vec!["awr", "workspace", "status", "--config", "w.toml"],
        ] {
            let parsed = Harness::try_parse_from(&args).expect("parses");
            let Top::Workspace(workspace) = parsed.command;
            assert_eq!(workspace.config, PathBuf::from("w.toml"));
        }
        let parsed = Harness::try_parse_from(["awr", "workspace", "status"]).expect("parses");
        let Top::Workspace(workspace) = parsed.command;
        assert_eq!(workspace.config, PathBuf::from("remote_workspace.toml"));
    }

    #[test]
    fn drop_takes_repeated_paths() {
        let parsed = Harness::try_parse_from([
            "awr",
            "workspace",
            "drop",
            "--path",
            "a.yaml",
            "--path",
            "infra/b.json",
        ])
        .expect("parses");
        let Top::Workspace(workspace) = parsed.command;
        match workspace.command {
            WorkspaceCommand::Drop { paths } => {
                assert_eq!(
                    paths,
                    vec!["a.yaml".to_string(), "infra/b.json".to_string()]
                );
            }
            other => panic!("expected drop, got {other:?}"),
        }
    }

    /// A store that never answers must not hold a session start: the budget is
    /// what ends the wait, and the answer is simply "not this time".
    #[test]
    fn the_budget_stops_waiting_for_a_job_that_never_finishes() {
        let started = Instant::now();
        let saw_cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&saw_cancel);
        let answer = within_budget(Duration::from_millis(50), move |cancel| {
            // Wait until the parent marks us cancelled, then report it.
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if cancel.load(Ordering::Acquire) {
                    flag.store(true, Ordering::Release);
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(Some("too late".to_string()))
        });
        assert!(answer.is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
        // Give the worker a moment to observe the flag.
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline && !saw_cancel.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            saw_cancel.load(Ordering::Acquire),
            "timed-out budget must signal cancel so workers stop writing"
        );
    }

    fn publish_report(contended: Vec<String>) -> awr_workspace::sync::PublishReport {
        awr_workspace::sync::PublishReport {
            pushed: Vec::new(),
            unchanged: Vec::new(),
            conflicts: Vec::new(),
            index_source: "manifest",
            commit_mode: "guarded",
            republished_after_index_drop: Vec::new(),
            contended,
            pointer_mirror_failures: Vec::new(),
            dry_run: false,
        }
    }

    /// A publish that lost every commit attempt to a peer is work still owed,
    /// not a conflict. The typed code is WorkspaceContended so an operator does
    /// not have to parse the message: nothing was overwritten, publish again.
    #[test]
    fn a_publish_that_lost_every_commit_says_publish_again() {
        let report = publish_report(vec!["infra/a.yaml".to_string()]);

        let error = contended(&report.contended);
        assert_eq!(error.code(), "WorkspaceContended");
        let message = error.to_string();
        assert!(message.contains("publish again"), "{message}");
        assert!(message.contains("infra/a.yaml"), "{message}");
        assert!(!message.contains("changed on both sides"), "{message}");

        // The human line reads as what happened, and says nothing about a
        // conflict, because there is none.
        let rendered = render_publish(&report);
        assert!(rendered.contains("left unpublished"), "{rendered}");
        assert!(rendered.contains("infra/a.yaml"), "{rendered}");
        assert!(rendered.contains("0 conflict(s)"), "{rendered}");

        // The field is a difference from the healthy shape, not a permanent
        // addition to it: a publish with nothing contended is byte-for-byte the
        // report it was before the field existed.
        let healthy = serde_json::to_value(publish_report(Vec::new())).unwrap();
        assert!(healthy.get("contended").is_none(), "{healthy}");
        assert!(healthy.get("dry_run").is_none(), "{healthy}");
        let lossy = serde_json::to_value(&report).unwrap();
        assert_eq!(lossy["contended"], serde_json::json!(["infra/a.yaml"]));
    }

    fn sync_report() -> awr_workspace::sync::SyncReport {
        awr_workspace::sync::SyncReport {
            host: "host-a".to_string(),
            changed: false,
            index_source: "manifest",
            pulled: Vec::new(),
            conflicts: Vec::new(),
            handoffs: Vec::new(),
            inbound_count: 0,
        }
    }

    /// A session start stays quiet about a plane with nothing to say, and says
    /// the one thing that matters when its own repair could not land: those
    /// entries are still missing, so the heal did not happen this time and the
    /// session has to ask for it.
    #[test]
    fn a_session_start_names_a_repair_it_could_not_land() {
        let outdir = Path::new("infra/handoffs");
        assert_eq!(
            notice_lines(&sync_report(), &publish_report(Vec::new()), outdir),
            None
        );

        let lost = notice_lines(
            &sync_report(),
            &publish_report(vec!["infra/a.yaml".to_string()]),
            outdir,
        )
        .expect("a repair that did not land is worth saying");
        assert!(lost.contains("another host committed it first"), "{lost}");
        assert!(lost.contains("infra/a.yaml"), "{lost}");
        assert!(lost.contains("awr workspace publish"), "{lost}");
        // Nobody else touched that path, so it must not read like a conflict.
        assert!(
            !lost.contains("changed on this host and on another one"),
            "{lost}"
        );
    }

    /// A slow job that checks cancel must observe the flag after the budget
    /// elapses, and must not be treated as a successful notice.
    #[test]
    fn cancel_stops_further_work_after_budget() {
        let writes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&writes);
        let answer = within_budget(Duration::from_millis(40), move |cancel| {
            // Simulate a multi-step sync: keep going until cancelled.
            for _ in 0..200 {
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
                counter.fetch_add(1, Ordering::SeqCst);
            }
            // After cancel, refuse to "write".
            if cancel.load(Ordering::Acquire) {
                return Err(Error::SourceUnavailable(
                    "workspace exchange cancelled: the session-start deadline elapsed before the store answered"
                        .into(),
                ));
            }
            Ok(Some("should-not-land".into()))
        });
        assert!(answer.is_none(), "budget must elapse");
        // Worker may still be finishing the current sleep; give it a moment.
        std::thread::sleep(Duration::from_millis(80));
        let steps = writes.load(Ordering::SeqCst);
        assert!(
            steps < 200,
            "cancel should stop the loop early, steps={steps}"
        );
    }

    /// A job that finishes inside the budget is reported as it finished,
    /// including the "nothing worth saying" answer.
    #[test]
    fn a_job_inside_the_budget_is_reported() {
        assert_eq!(
            within_budget(Duration::from_secs(5), |_| Ok(None))
                .expect("the job answered")
                .expect("the job succeeded"),
            None
        );
        assert_eq!(
            within_budget(Duration::from_secs(5), |_| Ok(Some("moved".to_string())))
                .expect("the job answered")
                .expect("the job succeeded"),
            Some("moved".to_string())
        );
    }
}
