use awr_core::{Error, Result};
use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;
mod context;
mod doctor;
mod drill;
mod query;
mod records;
mod resume;
mod search;
mod session;
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
    /// Start, inspect, checkpoint or end an explicitly identified agent session.
    Session {
        #[command(subcommand)]
        command: session::SessionCommand,
    },
    /// Add evidence records or inspect a specific record and its report reference.
    Evidence {
        #[command(subcommand)]
        command: records::EvidenceCommand,
    },
    /// Inspect a specific authoritative decision.
    Decision {
        #[command(subcommand)]
        command: records::DecisionCommand,
    },
    /// Import, inspect or explicitly read a registered artifact.
    Artifact {
        #[command(subcommand)]
        command: records::ArtifactCommand,
    },
    /// Compile revision-bound context after refreshing project sources.
    Context {
        #[command(subcommand)]
        command: context::ContextCommand,
    },
    /// Read one referenced goal, plan, rule, work item or checkpoint.
    Object {
        #[command(subcommand)]
        command: drill::ObjectCommand,
    },
    /// Inspect immutable events and bounded historical summaries.
    Event {
        #[command(subcommand)]
        command: drill::EventCommand,
    },
    /// Search bounded summaries, optionally filtering by entity type, status or work item.
    Search(search::SearchArgs),
    /// Diagnose project/database state; apply only explicitly selected runtime repairs.
    Doctor(doctor::DoctorArgs),
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
        Some(Command::Session { command }) => session::run(&cli.project, command, cli.json),
        Some(Command::Evidence { command }) => records::evidence(&cli.project, command, cli.json),
        Some(Command::Decision { command }) => records::decision(&cli.project, command, cli.json),
        Some(Command::Artifact { command }) => records::artifact(&cli.project, command, cli.json),
        Some(Command::Context { command }) => context::run(&cli.project, command, cli.json),
        Some(Command::Object { command }) => drill::object(&cli.project, command, cli.json),
        Some(Command::Event { command }) => drill::event(&cli.project, command, cli.json),
        Some(Command::Search(args)) => search::run(&cli.project, args, cli.json),
        Some(Command::Doctor(args)) => doctor::run(&cli.project, args, cli.json),
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
