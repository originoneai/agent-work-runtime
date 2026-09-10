use awr_core::*;
use awr_runtime::{CreateWorkInput, CreationReport};
use awr_store::Store;
use clap::Args;
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Protected JSON with version, request_key, title and optional source_id.
    #[arg(long,conflicts_with_all=["title","request_key","source"])]
    input: Option<PathBuf>,
    #[arg(long, required_unless_present = "input")]
    title: Option<String>,
    /// Stable host request ID, reused only with identical creation content.
    #[arg(long, required_unless_present = "input")]
    request_key: Option<String>,
    #[arg(long)]
    source: Option<Id>,
    #[arg(long)]
    accept: bool,
    #[arg(long)]
    expected_preview: Option<String>,
    #[arg(long)]
    expected_revision: Option<Revision>,
}
#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long)]
    key: String,
}
#[derive(Debug, Args)]
pub struct RecoverArgs {
    #[arg(long)]
    key: String,
    #[arg(long)]
    expected_revision: Revision,
}
fn output(report: CreationReport, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report.value)?);
    } else {
        println!(
            "Work creation: {}\nWork key: {}\nReceipt: {}",
            report.value["status"],
            report.value["external_key"],
            report.value["recovery_directory"]
        );
    }
    match report.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
pub fn create(root: &Path, args: &CreateArgs, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let input = if let Some(path) = &args.input {
        let path = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };
        serde_json::from_slice(&awr_source::read_capped(&path, 64 * 1024)?).map_err(|_| {
            Error::InvalidInput(
                "creation JSON requires version, request_key, title and optional source_id".into(),
            )
        })?
    } else {
        CreateWorkInput {
            version: 1,
            request_key: args.request_key.clone().unwrap(),
            title: args.title.clone().unwrap(),
            source_id: args.source,
        }
    };
    let mut store =
        Store::open_existing(&crate::source::runtime_dir(&root, false)?.join("state.db"))?;
    output(
        awr_runtime::create_work(
            &mut store,
            &root,
            input,
            args.accept,
            args.expected_preview.as_deref(),
            args.expected_revision,
        )?,
        json_output,
    )
}
pub fn status(root: &Path, args: &StatusArgs, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let store = Store::open_readonly(&crate::source::runtime_dir(&root, false)?.join("state.db"))?;
    output(
        awr_runtime::creation_status(&store, &root, &args.key)?,
        json_output,
    )
}
pub fn recover(root: &Path, args: &RecoverArgs, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let mut store =
        Store::open_existing(&crate::source::runtime_dir(&root, false)?.join("state.db"))?;
    output(
        awr_runtime::recover_creation(&mut store, &root, &args.key, args.expected_revision)?,
        json_output,
    )
}
