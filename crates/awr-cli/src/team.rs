use awr_core::{Error, Result};
use awr_team::{AuthContext, authorize, execute, parse_envelope};
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::Path;

use crate::remote::{load_profile, team_err};

#[derive(Debug, Subcommand)]
pub enum TeamCommand {
    /// Show Team protocol capabilities. Does not open the local SQLite store.
    Capabilities {
        #[arg(long)]
        remote: Option<String>,
    },
    /// Check a Team envelope, then return Unsupported: remote submission is not implemented.
    Command {
        #[arg(long)]
        remote: Option<String>,
        #[arg(long)]
        body: String,
        #[arg(long)]
        offline: bool,
    },
}

pub fn run(project: &Path, command: &TeamCommand, json: bool) -> Result<()> {
    match command {
        TeamCommand::Capabilities { remote } => {
            let profile = remote
                .as_deref()
                .map(|name| load_profile(project, name))
                .transpose()?;
            let auth = auth_from_profile(profile.as_ref())?;
            let value = execute(
                "cli",
                parse_envelope(&json!({
                    "protocol_version": 1,
                    "request_id": "capabilities",
                    "op": "capabilities"
                }))
                .map_err(team_err)?,
                &auth,
                profile.as_ref(),
                true,
            )
            .map_err(team_err)?;
            print_value(&value, json);
            Ok(())
        }
        TeamCommand::Command {
            remote,
            body,
            offline,
        } => {
            let parsed: Value =
                serde_json::from_str(body).map_err(|e| Error::InvalidInput(e.to_string()))?;
            let profile = remote
                .as_deref()
                .map(|name| load_profile(project, name))
                .transpose()?;
            let auth = auth_from_profile(profile.as_ref())?;
            if profile.is_some() {
                authorize(&auth, &parsed).map_err(team_err)?;
            }
            let envelope = parse_envelope(&parsed).map_err(team_err)?;
            let value =
                execute("cli", envelope, &auth, profile.as_ref(), !offline).map_err(team_err)?;
            print_value(&value, json);
            Ok(())
        }
    }
}

fn auth_from_profile(profile: Option<&awr_team::RemoteProfile>) -> Result<AuthContext> {
    Ok(AuthContext {
        tenant_id: "local".into(),
        project_id: profile
            .map(|p| p.project_key.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_default(),
        actor_id: "cli".into(),
        client_id: "awr-cli".into(),
    })
}

fn print_value(value: &Value, json: bool) {
    if json {
        println!("{value}");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        );
    }
}
