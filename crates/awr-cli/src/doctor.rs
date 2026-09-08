use awr_core::{Error, Id, Result, Revision};
use awr_store::{ReconcileAction, Store};
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    database: Option<PathBuf>,
    /// Only inspect SQLite/schema; skip project sources and runtime/file diagnostics.
    #[arg(long)]
    database_only: bool,
    /// Maximum bytes to hash per registered artifact (sources have a separate 16 MiB cap).
    #[arg(long, default_value_t = 16 * 1024 * 1024)]
    max_bytes: u64,
    #[command(subcommand)]
    command: Option<DoctorCommand>,
}
#[derive(Debug, Subcommand)]
enum DoctorCommand {
    /// Apply exactly one named runtime repair, preserving sources and artifact files.
    Repair {
        #[command(subcommand)]
        action: RepairAction,
    },
}
#[derive(Debug, Args)]
struct RepairTarget {
    id: Id,
    #[arg(long)]
    expected_revision: Revision,
    #[arg(long)]
    reason: String,
}
#[derive(Debug, Subcommand)]
enum RepairAction {
    /// Mark a still-active claim expired only if its lease has actually elapsed.
    ExpireClaim(RepairTarget),
    /// Explicitly close a selected active session and release its claims.
    InterruptSession(RepairTarget),
    /// Close an unfinished save attempt; retain its draft and the previous checkpoint.
    AbandonCheckpoint(RepairTarget),
    /// Clear a current branch pointer only when the selected branch is invalid.
    ClearInvalidBranch(RepairTarget),
}

pub fn run(root: &Path, args: &DoctorArgs, json: bool) -> Result<()> {
    let path = args
        .database
        .clone()
        .unwrap_or_else(|| root.join(".awr/state.db"));
    if let Some(DoctorCommand::Repair { action }) = &args.command {
        if args.database_only {
            return Err(Error::InvalidInput(
                "--database-only cannot be combined with a repair".into(),
            ));
        }
        let (target, action) = match action {
            RepairAction::ExpireClaim(target) => (
                target,
                ReconcileAction::ExpireClaim {
                    claim_id: target.id,
                },
            ),
            RepairAction::InterruptSession(target) => (
                target,
                ReconcileAction::InterruptSession {
                    session_id: target.id,
                },
            ),
            RepairAction::AbandonCheckpoint(target) => (
                target,
                ReconcileAction::AbandonCheckpoint {
                    attempt_id: target.id,
                },
            ),
            RepairAction::ClearInvalidBranch(target) => (
                target,
                ReconcileAction::ClearInvalidBranch {
                    branch_id: target.id,
                },
            ),
        };
        if !Store::inspect(&path)?.ok {
            return Err(Error::Storage(
                "repair requires an intact current database; no migration was attempted".into(),
            ));
        }
        let root = root.canonicalize()?;
        let mut store = Store::open_existing(&path)?;
        let project = store.project_by_root(&root)?;
        let receipt =
            store.reconcile(project.id, target.expected_revision, action, &target.reason)?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"ok":true,"repair_applied":true,"read_only":false,"source_refresh_performed":false,"project_id":project.id,"project_revision":receipt.project_revision,"receipt":receipt,"remaining_problems_evaluated":false,"next_action":"Run doctor to inspect remaining findings."})
                )?
            );
        } else {
            println!(
                "Applied selected repair at revision {}. Receipt: {}\nRun doctor to inspect remaining findings.",
                receipt.project_revision, receipt.event.id
            );
        }
        return Ok(());
    }
    if args.database_only {
        let report = Store::inspect(&path)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!(
                "Database only\nSchema: {}\nIntegrity: {}\nForeign-key violations: {}",
                report.schema_version,
                report.integrity.join(", "),
                report.foreign_key_violations
            );
        }
        return if report.ok {
            Ok(())
        } else {
            Err(Error::Storage(
                "doctor reported integrity or schema problems".into(),
            ))
        };
    }
    let report = awr_runtime::diagnose_project(root, &path, args.max_bytes)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "Doctor (read only)\nDatabase intact: {}\nProject revision: {}\nSources checked: {}\nArtifacts checked: {}\nFindings: {}",
            report.database_ok,
            report
                .project_revision
                .map(|r| r.to_string())
                .unwrap_or_else(|| "unavailable".into()),
            report.sources_checked,
            report.artifacts_checked,
            report.findings.len()
        );
        for finding in &report.findings {
            println!(
                "[{}] {} {}:{} — {}",
                finding.severity,
                finding.code,
                finding.object_kind,
                finding.object_id,
                finding.message
            );
            if let Some(action) = &finding.repair {
                println!("  Repair candidate: {}", serde_json::to_string(action)?);
            }
        }
        println!(
            "No repair was applied. Select one named repair with --expected-revision and --reason when appropriate."
        );
    }
    if report.database.ok {
        Ok(())
    } else {
        Err(Error::Storage(
            "doctor found unresolved problems; inspect findings (no repairs applied)".into(),
        ))
    }
}
