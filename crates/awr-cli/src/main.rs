use awr_core::{Error, Result};
use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;
mod branch;
mod capabilities;
mod catalog;
mod client;
mod context;
mod doctor;
mod drill;
mod event_append;
mod execution;
mod intake_plan;
mod mutation;
mod onboarding;
mod query;
mod records;
mod recovery;
mod resume;
mod search;
mod session;
mod source;
mod source_changes;
mod work_action;
mod work_create;

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
    /// Negotiate host capabilities without opening a project or its database.
    Capabilities(capabilities::CapabilitiesArgs),
    /// Preview source authority mapping; --accept initializes using the reviewed mapping.
    Init(onboarding::InitArgs),
    /// Diagnose project organization and recheck readiness after source edits.
    Intake {
        #[command(subcommand)]
        command: onboarding::IntakeCommand,
    },
    /// Bind client conversations and persist lifecycle checkpoints.
    Client {
        #[command(subcommand)]
        command: client::ClientCommand,
    },
    /// Register, run and inspect executions that can outlive the calling session.
    Execution {
        #[command(subcommand)]
        command: execution::ExecutionCommand,
    },
    /// Inspect saved work and verify execution evidence before resuming.
    Recovery {
        #[command(subcommand)]
        command: recovery::RecoveryCommand,
    },
    /// List, scan or index authoritative project sources.
    Source {
        #[command(subcommand)]
        command: source::SourceCommand,
    },
    /// Refresh source projections and summarize current project work.
    Status {
        #[arg(long)]
        branch: Option<String>,
        /// Verify completion reports against this explicit full source SHA.
        #[arg(long)]
        source_sha: Option<String>,
    },
    /// List dependency-ready work with explicit reasons for excluded work.
    Ready {
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        branch: Option<String>,
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
    /// Append validated events or inspect immutable events and bounded history.
    Event {
        #[command(subcommand)]
        command: drill::EventCommand,
    },
    /// Search bounded summaries, optionally filtering by entity type, status or work item.
    Search(search::SearchArgs),
    /// Create, inspect and review source-bound mutation proposals.
    Proposal {
        #[command(subcommand)]
        command: mutation::ProposalCommand,
    },
    /// Create, inspect or select an Agent Work Branch without changing Git checkout.
    Branch {
        #[command(subcommand)]
        command: branch::BranchCommand,
    },
    /// Diagnose project/database state; apply only explicitly selected runtime repairs.
    Doctor(doctor::DoctorArgs),
    #[command(external_subcommand)]
    Unsupported(Vec<String>),
}

fn run(cli: &Cli) -> Result<()> {
    match &cli.command {
        Some(Command::Capabilities(args)) => capabilities::run(args, cli.json),
        Some(Command::Init(args)) => onboarding::run(&cli.project, args, cli.json),
        Some(Command::Intake { command }) => onboarding::inspect(&cli.project, command, cli.json),
        Some(Command::Client { command }) => client::run(&cli.project, command, cli.json),
        Some(Command::Execution { command }) => execution::run(&cli.project, command, cli.json),
        Some(Command::Recovery { command }) => recovery::run(&cli.project, command, cli.json),
        Some(Command::Source { command }) => source::run(&cli.project, command, cli.json),
        Some(Command::Status { branch, source_sha }) => query::status(
            &cli.project,
            branch.as_deref(),
            source_sha.as_deref(),
            cli.json,
        ),
        Some(Command::Ready { limit, branch }) => {
            query::ready(&cli.project, *limit, branch.as_deref(), cli.json)
        }
        Some(Command::Work { command }) => query::work(&cli.project, command, cli.json),
        Some(Command::Session { command }) => session::run(&cli.project, command, cli.json),
        Some(Command::Evidence { command }) => records::evidence(&cli.project, command, cli.json),
        Some(Command::Decision { command }) => records::decision(&cli.project, command, cli.json),
        Some(Command::Artifact { command }) => records::artifact(&cli.project, command, cli.json),
        Some(Command::Context { command }) => context::run(&cli.project, command, cli.json),
        Some(Command::Object { command }) => drill::object(&cli.project, command, cli.json),
        Some(Command::Event { command }) => drill::event(&cli.project, command, cli.json),
        Some(Command::Search(args)) => search::run(&cli.project, args, cli.json),
        Some(Command::Proposal { command }) => mutation::run(&cli.project, command, cli.json),
        Some(Command::Branch { command }) => branch::run(&cli.project, command, cli.json),
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
    let args = std::env::args_os().collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| awr_core::contains_sensitive_text(&arg.to_string_lossy()))
    {
        let report =
            Error::RuleViolation("sensitive command arguments are not accepted".into()).report();
        eprintln!(
            "{}",
            serde_json::to_string(&report).expect("error report serializes")
        );
        return std::process::ExitCode::from(1);
    }
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => {
            // A literal after `--` is data, and `--field=--json` is not the JSON flag.
            let json = args
                .iter()
                .skip(1)
                .take_while(|arg| *arg != "--")
                .any(|arg| arg == "--json");
            if error.use_stderr() && json {
                let report = Error::InvalidInput(error.to_string().trim().into()).report();
                eprintln!(
                    "{}",
                    serde_json::to_string(&report).expect("error report serializes")
                );
            } else {
                let _ = error.print();
            }
            return std::process::ExitCode::from(error.exit_code() as u8);
        }
    };
    match run(&cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            if cli.json {
                eprintln!(
                    "{}",
                    serde_json::to_string(&error.report()).expect("error report serializes")
                );
            } else {
                eprintln!("{}: {}", error.code(), error.report().message);
            }
            std::process::ExitCode::from(1)
        }
    }
}
