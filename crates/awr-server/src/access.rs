use awr_team_pg::{AccessPlan, OperatorAccess, PgError};
use clap::Subcommand;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AccessCommand {
    /// Generate a bearer into a new local file; print only registration metadata.
    Token {
        #[arg(long)]
        credential_id: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Inspect one actor/client's project access through the schema-owner connection.
    Inspect {
        #[arg(long)]
        tenant_id: String,
        #[arg(long)]
        project_id: String,
        #[arg(long)]
        actor_id: String,
        #[arg(long)]
        client_id: String,
    },
    /// Preview an access plan without changing policy.
    Preview {
        #[arg(long)]
        input: PathBuf,
    },
    /// Apply the exact reviewed plan; a stale state digest is rejected.
    Apply {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        request_id: String,
        #[arg(long)]
        expected_state: String,
    },
    /// Inspect an original request after a timeout before retrying it exactly.
    Outcome {
        #[arg(long)]
        tenant_id: String,
        #[arg(long)]
        project_id: String,
        #[arg(long)]
        request_id: String,
    },
}

pub type Error = (&'static str, &'static str);
fn pg_error(e: PgError) -> Error {
    match e {
        PgError::Forbidden => ("Forbidden", "schema-owner access or requested scope denied"),
        PgError::Protocol(_) => ("InvalidInput", "invalid access plan, identity or bounds"),
        PgError::PreconditionsChanged => (
            "PreconditionsChanged",
            "access changed or plan conflicts with current identity; preview again",
        ),
        PgError::IdempotencyConflict => (
            "IdempotencyConflict",
            "request ID is bound to a different intent",
        ),
        PgError::Unsupported(_) => (
            "Unsupported",
            "operation requires an enabled workstream project",
        ),
        _ => (
            "Unavailable",
            "operator operation did not return a confirmed result; inspect its original request before retrying",
        ),
    }
}
fn plan(path: &PathBuf) -> Result<AccessPlan, Error> {
    let file =
        std::fs::File::open(path).map_err(|_| ("InvalidInput", "cannot open access plan"))?;
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| ("InvalidInput", "cannot read access plan"))?;
    if bytes.len() > 65536 {
        return Err(("InvalidInput", "access plan exceeds 64 KiB"));
    }
    serde_json::from_slice(&bytes).map_err(|_| ("InvalidInput", "invalid access plan JSON"))
}

fn token(id: &str, output: &PathBuf) -> Result<Value, Error> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(("InvalidInput", "invalid credential ID"));
    }
    let mut random = [0u8; 32];
    getrandom::fill(&mut random)
        .map_err(|_| ("Unavailable", "operating-system randomness unavailable"))?;
    let secret = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let bearer = format!("awr1.{id}.{secret}");
    let hash = awr_team_pg::workstream_credential_hash(&bearer).map_err(pg_error)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(output).map_err(|_| {
        (
            "InvalidInput",
            "cannot create credential file; existing paths are never overwritten",
        )
    })?;
    file.write_all(format!("{bearer}\n").as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| {
            (
                "Unavailable",
                "credential file write failed; nothing was registered",
            )
        })?;
    Ok(json!({"credential_id":id,"secret_hash":hash,"credential_file":output,"registered":false}))
}

pub async fn run(command: AccessCommand) -> Result<Value, Error> {
    if let AccessCommand::Token {
        credential_id,
        output,
    } = &command
    {
        return token(credential_id, output);
    }
    let url = std::env::var("AWR_TEAM_DATABASE_URL").map_err(|_| {
        (
            "InvalidInput",
            "AWR_TEAM_DATABASE_URL is required for operator access",
        )
    })?;
    let mut client = awr_team_pg::connect(&url).await.map_err(pg_error)?;
    match command {
        AccessCommand::Token { .. } => unreachable!(),
        AccessCommand::Inspect {
            tenant_id,
            project_id,
            actor_id,
            client_id,
        } => {
            OperatorAccess::inspect(&mut client, &tenant_id, &project_id, &actor_id, &client_id)
                .await
        }
        AccessCommand::Preview { input } => {
            OperatorAccess::preview(&mut client, &plan(&input)?).await
        }
        AccessCommand::Apply {
            input,
            request_id,
            expected_state,
        } => OperatorAccess::apply(&mut client, &plan(&input)?, &request_id, &expected_state).await,
        AccessCommand::Outcome {
            tenant_id,
            project_id,
            request_id,
        } => OperatorAccess::outcome(&mut client, &tenant_id, &project_id, &request_id).await,
    }
    .map_err(pg_error)
}
