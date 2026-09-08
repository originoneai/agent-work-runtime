use awr_core::{Error, GitRefBinding, Id, Project, Result, Revision};
use awr_store::Store;
use serde::{Deserialize, Serialize};

/// A read binding, never a checkout or a mutation of the project's branch defaults.
/// Source versions are the current shared projections, not a snapshot at fork or at git_ref.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchContextBinding {
    pub branch_id: Option<Id>,
    pub name: String,
    pub parent_branch_id: Option<Id>,
    pub branch_revision: Option<Revision>,
    pub fork_project_revision: Revision,
    pub creation_event_id: Option<Id>,
    pub git_ref: Option<String>,
    pub git_binding: Option<GitRefBinding>,
    pub source_basis: String,
    pub runtime_scope: String,
}

pub(crate) fn branch_binding(
    store: &Store,
    project: &Project,
    branch: Option<Id>,
) -> Result<BranchContextBinding> {
    let record = branch.map(|id| store.branch(project.id, id)).transpose()?;
    if let Some(record) = &record {
        let branch = &record.branch;
        if branch.status != "active" {
            return Err(Error::InvalidTransition(format!(
                "branch {} is {}; execution context requires an active branch",
                branch.id, branch.status
            )));
        }
        if branch.fork_project_revision > project.project_revision
            || branch.parent_branch_id == Some(branch.id)
        {
            return Err(Error::InvalidInput(
                "branch has an invalid fork revision or parent".into(),
            ));
        }
    }
    Ok(BranchContextBinding {
        branch_id: branch,
        name: record.as_ref().map(|r| r.branch.name.clone()).unwrap_or_else(|| "main".into()),
        parent_branch_id: record.as_ref().and_then(|r| r.branch.parent_branch_id),
        branch_revision: record.as_ref().map(|r| r.branch.revision),
        fork_project_revision: record.as_ref().map(|r| r.branch.fork_project_revision).unwrap_or(0),
        creation_event_id: record.as_ref().and_then(|r| r.creation_event_id),
        git_ref: record.as_ref().and_then(|r| r.branch.git_ref.clone()),
        git_binding: record.and_then(|r| r.git_binding),
        source_basis: "shared_current_sources; git_ref is a creation observation, not a frozen source snapshot".into(),
        runtime_scope: "exact_branch; no parent or sibling runtime inheritance; delta never precedes fork".into(),
    })
}

pub(crate) fn require_fork_request(baseline: &crate::DeltaBaseline) -> Result<()> {
    if !matches!(
        baseline,
        crate::DeltaBaseline::Auto | crate::DeltaBaseline::BranchFork
    ) {
        return Err(Error::InvalidInput(
            "named branch context/delta uses the fork baseline; use context compile/delta for a checkpoint or revision baseline".into(),
        ));
    }
    Ok(())
}
