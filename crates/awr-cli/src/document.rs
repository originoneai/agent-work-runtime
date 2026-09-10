use awr_core::*;
use awr_runtime::{DocumentReport, DocumentRequest};
use awr_store::Store;
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub struct ChangeArgs {
    /// Versioned document edit or new draft, including a stable request key.
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    accept: bool,
    #[arg(long)]
    expected_preview: Option<String>,
    #[arg(long)]
    expected_revision: Option<Revision>,
}
#[derive(Debug, Subcommand)]
pub enum DocumentCommand {
    /// Preview a source-bound change; accept exactly the returned preview to write it.
    Change(ChangeArgs),
    /// Read a retained request outcome without indexing or writing.
    Status {
        #[arg(long)]
        key: String,
    },
    /// Explicitly recover one interrupted document change at the current revision.
    Recover {
        #[arg(long)]
        key: String,
        #[arg(long)]
        expected_revision: Revision,
    },
}
fn output(report: DocumentReport, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report.value)?)
    } else {
        println!("Document change: {}", report.value["status"])
    }
    match report.failure {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
pub fn run(root: &Path, command: &DocumentCommand, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let db = crate::source::runtime_dir(&root, false)?.join("state.db");
    match command {
        DocumentCommand::Status { key } => output(
            awr_runtime::document_status(&Store::open_readonly(&db)?, &root, key)?,
            json_output,
        ),
        DocumentCommand::Recover {
            key,
            expected_revision,
        } => output(
            awr_runtime::recover_document(
                &mut Store::open_existing(&db)?,
                &root,
                key,
                *expected_revision,
            )?,
            json_output,
        ),
        DocumentCommand::Change(args) => {
            let path = if args.input.is_absolute() {
                args.input.clone()
            } else {
                root.join(&args.input)
            };
            let input: DocumentRequest = serde_json::from_slice(&awr_source::read_capped(
                &path,
                awr_source::MARKDOWN_READ_CAP * 3,
            )?)
            .map_err(|_| {
                Error::InvalidInput(
                    "document JSON requires version, request_key and a supported change".into(),
                )
            })?;
            output(
                awr_runtime::change_document(
                    &mut Store::open_existing(&db)?,
                    &root,
                    input,
                    args.accept,
                    args.expected_preview.as_deref(),
                    args.expected_revision,
                )?,
                json_output,
            )
        }
    }
}
