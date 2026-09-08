use awr_core::*;
use awr_store::Store;
use clap::Subcommand;
use serde_json::json;
use std::path::Path;

#[derive(Debug, Subcommand)]
pub enum BranchCommand {
    /// Create runtime work identity at the current revision; never creates a Git branch.
    Create {
        name: String,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        git_ref: Option<String>,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        expected_revision: Revision,
    },
    /// List retained work branches; main is the existing baseline, represented by a null ID.
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Read a branch and its creation/Git receipt; omit the selector for the current branch.
    Show { reference: Option<String> },
    /// Select defaults for later operations. Existing sessions and claims keep their branch.
    Switch {
        reference: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        expected_revision: Revision,
    },
}
pub fn run(root: &Path, command: &BranchCommand, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let database = crate::source::runtime_dir(&root, false)?.join("state.db");
    let readonly = matches!(
        command,
        BranchCommand::List { .. } | BranchCommand::Show { .. }
    );
    let mut store = if readonly {
        Store::open_readonly(&database)?
    } else {
        Store::open_existing(&database)?
    };
    let project = store.project_by_root(&root)?;
    match command {
        BranchCommand::Create {
            name,
            parent,
            git_ref,
            actor,
            reason,
            expected_revision,
        } => {
            let (branch, event) = awr_runtime::create_branch(
                &mut store,
                &root,
                &awr_runtime::CreateBranchRequest {
                    name: name.clone(),
                    parent: parent.clone(),
                    git_ref: git_ref.clone(),
                    expected_revision: *expected_revision,
                    actor: actor.clone(),
                    reason: reason.clone(),
                },
            )?;
            let record = store.branch(project.id, branch.id)?;
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({"project_id":project.id,"project_revision":event.project_revision,"current_branch_id":project.current_branch_id,"branch":branch,"git_binding":record.git_binding,"event":event,"git_write_performed":false,"source_write_performed":false})
                    )?
                );
            } else {
                println!(
                    "Created work branch: {} ({})\nParent: {}\nFork revision: {}\nGit ref: {}\nRevision: {}\nCreation leaves the current work branch selected.",
                    branch.name,
                    branch.id,
                    branch
                        .parent_branch_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "main".into()),
                    branch.fork_project_revision,
                    branch.git_ref.as_deref().unwrap_or("unbound"),
                    event.project_revision
                );
            }
        }
        BranchCommand::List {
            status,
            offset,
            limit,
        } => {
            let page = store.branches(project.id, status.as_deref(), *offset, *limit)?;
            if json_output {
                let mut value = serde_json::to_value(&page)?;
                value["main"] = json!({"name":"main","branch_id":null,"selected":page.current_branch_id.is_none()});
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!(
                    "{} main (baseline)\nRevision: {}",
                    if page.current_branch_id.is_none() {
                        "*"
                    } else {
                        " "
                    },
                    page.project_revision
                );
                for branch in &page.branches {
                    println!(
                        "{} {} ({}) [{}] fork={} git={}",
                        if page.current_branch_id == Some(branch.id) {
                            "*"
                        } else {
                            " "
                        },
                        branch.name,
                        branch.id,
                        branch.status,
                        branch.fork_project_revision,
                        branch.git_ref.as_deref().unwrap_or("unbound")
                    );
                }
                println!(
                    "Work branches: {} of {} (offset {}, more: {})",
                    page.branches.len(),
                    page.total,
                    page.offset,
                    page.has_more
                );
            }
        }
        BranchCommand::Show { reference } => {
            let id = match reference {
                Some(reference) => store.resolve_branch(project.id, reference)?,
                None => project.current_branch_id,
            };
            let record = id.map(|id| store.branch(project.id, id)).transpose()?;
            let actual = store.project(project.id)?.project_revision;
            if actual != project.project_revision {
                return Err(Error::RevisionConflict {
                    expected: project.project_revision,
                    actual,
                });
            }
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({"project_id":project.id,"project_revision":project.project_revision,"current_branch_id":project.current_branch_id,"selected":id==project.current_branch_id,"name":record.as_ref().map(|r|r.branch.name.as_str()).unwrap_or("main"),"branch_id":id,"record":record})
                    )?
                );
            } else if let Some(record) = record {
                println!(
                    "Work branch: {} ({})\nStatus: {}\nParent: {}\nFork revision: {}\nGit ref: {}\nRecorded Git commit: {}\nCreation receipt: {}",
                    record.branch.name,
                    record.branch.id,
                    record.branch.status,
                    record
                        .branch
                        .parent_branch_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "main".into()),
                    record.branch.fork_project_revision,
                    record.branch.git_ref.as_deref().unwrap_or("unbound"),
                    record
                        .git_binding
                        .as_ref()
                        .map(|b| b.commit_sha.as_str())
                        .unwrap_or("not verified"),
                    record
                        .creation_event_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "legacy row without receipt".into())
                );
            } else {
                println!(
                    "Work branch: main (existing baseline, null branch ID)\nHistorical sessions, events, claims and evidence remain unchanged.\nRevision: {}",
                    project.project_revision
                );
            }
        }
        BranchCommand::Switch {
            reference,
            actor,
            reason,
            expected_revision,
        } => {
            let (selection, event) = awr_runtime::switch_branch(
                &mut store,
                &root,
                reference,
                *expected_revision,
                actor,
                reason,
            )?;
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({"project_id":project.id,"project_revision":event.project_revision,"selection":selection,"event":event,"git_write_performed":false,"source_write_performed":false})
                    )?
                );
            } else {
                println!(
                    "Selected work branch: {reference}\nRevision: {}\nRetained on previous branch: {} active sessions, {} live claims.\nExisting records retain their branch; Git checkout and sources are unchanged.",
                    event.project_revision,
                    selection.retained_active_sessions,
                    selection.retained_active_claims
                );
            }
        }
    }
    Ok(())
}
