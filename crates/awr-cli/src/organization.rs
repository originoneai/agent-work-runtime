use awr_core::*;
use awr_runtime::OrganizationChange;
use awr_store::Store;
use clap::Subcommand;
use std::path::{Path, PathBuf};
#[derive(Debug, Subcommand)]
pub enum OrganizationCommand {
    /// Read source annotations through an explicit JSON mapping file.
    Show {
        #[arg(long)]
        source: Id,
        #[arg(long)]
        mapping: PathBuf,
    },
    /// Preview exact changes to explicitly mapped project metadata.
    Preview {
        #[arg(long)]
        input: PathBuf,
    },
    /// Apply the reviewed metadata edit without changing entity lifecycle or contracts.
    Change {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        expected_preview: String,
        #[arg(long)]
        expected_revision: Revision,
    },
    /// Read a retained request's outcome without applying it again.
    Status {
        #[arg(long)]
        key: String,
    },
    /// Recover an interrupted request only from its matching before/after bytes.
    Recover {
        #[arg(long)]
        key: String,
        #[arg(long)]
        expected_revision: Revision,
    },
}
fn input(root: &Path, path: &Path) -> Result<OrganizationChange> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    };
    serde_json::from_slice(&awr_source::read_capped(&path,awr_source::YAML_READ_CAP)?).map_err(|_|Error::InvalidInput("organization JSON requires version, request_key, actor, reason, source_id, source_fingerprint, mapping and values".into()))
}
pub fn run(root: &Path, command: &OrganizationCommand, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let database = crate::source::runtime_dir(&root, false)?.join("state.db");
    let value = match command {
        OrganizationCommand::Show { source, mapping } => {
            let path = if mapping.is_absolute() {
                mapping.to_owned()
            } else {
                root.join(mapping)
            };
            let mapping = serde_json::from_slice(&awr_source::read_capped(&path, 65536)?)?;
            awr_runtime::read_organization(
                &Store::read_snapshot(&database, 256 * 1024 * 1024)?,
                &root,
                *source,
                &mapping,
            )?
        }
        OrganizationCommand::Preview { input: path } => awr_runtime::organization_preview(
            &Store::read_snapshot(&database, 256 * 1024 * 1024)?,
            &root,
            input(&root, path)?,
        )?,
        OrganizationCommand::Status { key } => awr_runtime::organization_status(
            &Store::read_snapshot(&database, 256 * 1024 * 1024)?,
            &root,
            key,
        )?,
        OrganizationCommand::Change {
            input: path,
            expected_preview,
            expected_revision,
        } => awr_runtime::change_organization(
            &mut Store::open_existing(&database)?,
            &root,
            input(&root, path)?,
            *expected_revision,
            expected_preview,
        )?,
        OrganizationCommand::Recover {
            key,
            expected_revision,
        } => awr_runtime::recover_organization(
            &mut Store::open_existing(&database)?,
            &root,
            key,
            *expected_revision,
        )?,
    };
    if json_output {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "Organization: {}",
            value["phase"].as_str().unwrap_or("preview")
        );
    }
    Ok(())
}
