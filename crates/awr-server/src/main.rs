mod access;
mod runner;
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "awr-server",
    version,
    about = "AWR Team coordinator and scoped HTTP service"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run bounded file plans under an explicitly delegated scoped identity.
    Runner {
        #[command(subcommand)]
        command: runner::RunnerCommand,
    },
    /// Provision scoped clients using an explicit schema-owner connection.
    Access {
        #[command(subcommand)]
        command: access::AccessCommand,
    },
    /// Run authenticated multi-project reads and durable session journaling.
    Serve {
        #[arg(long)]
        config: std::path::PathBuf,
    },
    /// Apply owner migrations, then refuse to start if schema is incompatible.
    Migrate {
        /// Re-apply the application role grants after migrating (idempotent).
        /// Required when upgrading a database bootstrapped by an older
        /// version whose grants predate the current bootstrap (CR #52 P2-1).
        /// Runs as the owner connection; never required from app credentials.
        #[arg(long)]
        app_role: Option<String>,
    },
    /// Check schema version without running migrations.
    Check,
    /// Experimental Team v1 query entry. Unknown ops return Unsupported.
    Query {
        /// capabilities | work.prepare | work.graph | session.inspect | events.list
        #[arg(long)]
        op: String,
        /// JSON object with tenant_id, project_id and query fields.
        #[arg(long)]
        body: Option<String>,
    },
    /// Validate envelope syntax, then return Unsupported; no authenticated command transport exists.
    Command {
        #[arg(long)]
        op: String,
        #[arg(long)]
        body: Option<String>,
    },
    /// Local test/validation only. Context is supplied independently, NOT authenticated.
    ValidateCommand {
        #[arg(long)]
        op: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        tenant_id: String,
        #[arg(long)]
        project_id: String,
        #[arg(long)]
        actor_id: String,
        #[arg(long)]
        client_id: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    match args.command {
        Command::Runner { command } => match runner::run(command).await {
            Ok(value) => {
                println!("{value}");
                ExitCode::SUCCESS
            }
            Err((code, message)) => fail(code, message),
        },
        Command::Access { command } => match access::run(command).await {
            Ok(value) => {
                println!("{value}");
                ExitCode::SUCCESS
            }
            Err((code, message)) => fail(code, message),
        },
        Command::Serve { config } => match awr_server::service::serve(&config).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail("ServiceFailed", error),
        },
        Command::Query { op, body } => run_query(&op, body.as_deref()).await,
        Command::Command { op, body } => run_command(&op, body.as_deref(), None).await,
        Command::ValidateCommand {
            op,
            body,
            tenant_id,
            project_id,
            actor_id,
            client_id,
        } => {
            let context = awr_team::AuthContext {
                tenant_id,
                project_id,
                actor_id,
                client_id,
            };
            run_command(&op, Some(&body), Some(&context)).await
        }
        Command::Migrate { app_role } => schema_command(true, app_role).await,
        Command::Check => schema_command(false, None).await,
    }
}

fn fail(code: &str, message: impl ToString) -> ExitCode {
    eprintln!("{}", json!({"code": code, "message": message.to_string()}));
    ExitCode::FAILURE
}

async fn schema_command(migrate: bool, app_role: Option<String>) -> ExitCode {
    let url = match std::env::var("AWR_TEAM_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => return fail("SchemaIncompatible", "AWR_TEAM_DATABASE_URL is required"),
    };
    let client = match awr_team_pg::connect(&url).await {
        Ok(client) => client,
        Err(error) => return fail("SchemaIncompatible", error.to_string()),
    };
    let result = if migrate {
        match awr_team_pg::migrate(&client).await {
            Ok(()) => match &app_role {
                Some(role) => awr_team_pg::Bootstrap::grant_app(&client, role).await,
                None => Ok(()),
            },
            Err(error) => Err(error),
        }
    } else {
        awr_team_pg::check_schema(&client).await
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail("SchemaIncompatible", error.to_string()),
    }
}

fn required<'a>(body: &'a Value, key: &str) -> Result<&'a str, ExitCode> {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| fail("InvalidInput", format!("{key} required")))
}

async fn run_query(op: &str, body: Option<&str>) -> ExitCode {
    if op == "capabilities" {
        println!("{}", awr_team_pg::capabilities());
        return ExitCode::SUCCESS;
    }
    if let Err(awr_team_pg::PgError::Unsupported(name)) = awr_team_pg::dispatch_query(op) {
        return fail("Unsupported", format!("unsupported query {name}"));
    }
    let url = match std::env::var("AWR_TEAM_DATABASE_URL") {
        Ok(url) => url,
        Err(_) => return fail("SchemaIncompatible", "AWR_TEAM_DATABASE_URL is required"),
    };
    if let Err(error) = awr_team_pg::check_schema(&match awr_team_pg::connect(&url).await {
        Ok(client) => client,
        Err(error) => return fail("SchemaIncompatible", error.to_string()),
    })
    .await
    {
        return fail("SchemaIncompatible", error.to_string());
    }
    let parsed: Value = match body {
        None => return fail("InvalidInput", "query body is required"),
        Some(raw) => match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(error) => return fail("InvalidInput", error.to_string()),
        },
    };
    let tenant = match required(&parsed, "tenant_id") {
        Ok(value) => value,
        Err(code) => return code,
    };
    let project = match required(&parsed, "project_id") {
        Ok(value) => value,
        Err(code) => return code,
    };
    let store = awr_team_pg::ReadStore::new(url);
    let result = match op {
        "work.prepare" => {
            let work_id = match required(&parsed, "work_id") {
                Ok(value) => value,
                Err(code) => return code,
            };
            let max_bytes = parsed.get("max_context_bytes").and_then(Value::as_u64);
            store
                .prepare(tenant, project, work_id, max_bytes.map(|n| n as usize))
                .await
                .map(|value| serde_json::to_value(value).expect("prepare json"))
        }
        "work.graph" => store
            .graph(tenant, project)
            .await
            .map(|value| serde_json::to_value(value).expect("graph json")),
        "session.inspect" => {
            let session_id = match required(&parsed, "session_id") {
                Ok(value) => value,
                Err(code) => return code,
            };
            store.inspect_session(tenant, project, session_id).await
        }
        "events.list" => {
            let after = parsed.get("after").and_then(Value::as_str);
            let limit = parsed.get("limit").and_then(Value::as_i64).unwrap_or(50);
            store
                .list_events(tenant, project, after, limit)
                .await
                .map(|value| serde_json::to_value(value).expect("events json"))
        }
        other => return fail("Unsupported", format!("unsupported query {other}")),
    };
    match result {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(awr_team_pg::PgError::Unsupported(name)) => fail("Unsupported", name),
        Err(awr_team_pg::PgError::SessionNotFound) => fail("SessionNotFound", "session not found"),
        Err(awr_team_pg::PgError::EpochChanged) => {
            fail("EPOCH_CHANGED", "coordinator epoch changed")
        }
        Err(awr_team_pg::PgError::CursorExpired) => fail("CURSOR_EXPIRED", "event cursor expired"),
        Err(error) => fail("QueryFailed", error.to_string()),
    }
}

async fn run_command(
    op: &str,
    body: Option<&str>,
    context: Option<&awr_team::AuthContext>,
) -> ExitCode {
    let parsed: Value = match body {
        None => return fail("InvalidInput", "command body is required"),
        Some(raw) => match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(_) => return fail("InvalidInput", "invalid command JSON"),
        },
    };
    // Strict receiver: no inferred protocol, request id or operation, and no mutation.
    let envelope = match awr_team::parse_envelope(&parsed) {
        Ok(value) => value,
        Err(error) => return fail(error.code(), error.to_string()),
    };
    if envelope.operation() != op {
        return fail("InvalidInput", "--op does not match envelope op");
    }
    let Some(context) = context else {
        return fail(
            "Unsupported",
            "authenticated Team command transport/dispatch is not implemented; nothing was submitted",
        );
    };
    match awr_team::validate_only("cli", &envelope, context) {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(error) => fail(error.code(), error.to_string()),
    }
}
