use awr_core::{Error, Result};
use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;
mod query;
mod source;

#[derive(Debug, Parser)]
#[command(
    name = "awr",
    version,
    about = "Persistent work state and minimal context for long-running agents"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".")]
    project: PathBuf,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Preview source authority mapping; --accept initializes using the reviewed mapping.
    Init {
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long)]
        accept: bool,
    },
    /// List, scan or index authoritative project sources.
    Source {
        #[command(subcommand)]
        command: source::SourceCommand,
    },
    /// Refresh source projections and summarize current project work.
    Status,
    /// List dependency-ready work with explicit reasons for excluded work.
    Ready {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Read one work item without expanding the full ledger or event history.
    Work {
        #[command(subcommand)]
        command: query::WorkCommand,
    },
    /// Inspect an existing AWR database without creating or repairing it.
    Doctor {
        #[arg(long)]
        database: Option<PathBuf>,
    },
    #[command(external_subcommand)]
    Unsupported(Vec<String>),
}

fn run(cli: &Cli) -> Result<()> {
    match &cli.command {
        Some(Command::Init { manifest, accept }) => {
            source::initialize(&cli.project, manifest.as_deref(), *accept, cli.json)
        }
        Some(Command::Source { command }) => source::run(&cli.project, command, cli.json),
        Some(Command::Status) => query::status(&cli.project, cli.json),
        Some(Command::Ready { limit }) => query::ready(&cli.project, *limit, cli.json),
        Some(Command::Work { command }) => query::work(&cli.project, command, cli.json),
        Some(Command::Doctor { database }) => {
            let path = database
                .clone()
                .unwrap_or_else(|| cli.project.join(".awr/state.db"));
            let report = awr_store::Store::inspect(&path)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "SQLite: {}\nSchema: {}\nJournal: {}\nIntegrity: {}\nForeign-key violations: {}",
                    report.sqlite_version,
                    report.schema_version,
                    report.journal_mode,
                    report.integrity.join(", "),
                    report.foreign_key_violations
                );
            }
            if !report.ok {
                return Err(Error::Storage(
                    "doctor reported integrity or schema problems".into(),
                ));
            }
            Ok(())
        }
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
        Some(Command::Unsupported(args)) => Err(Error::Unsupported(args.join(" "))),
    }
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            if cli.json {
                eprintln!(
                    "{}",
                    serde_json::to_string(&error.report()).expect("error report serializes")
                );
            } else {
                eprintln!("{}: {}", error.code(), error);
            }
            std::process::ExitCode::from(1)
        }
    }
}
