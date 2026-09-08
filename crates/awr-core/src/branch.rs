use crate::{Branch, Error, Id, Result, Revision, is_source_sha};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// An observation of an existing local Git ref, not a checkout or a Source snapshot.
/// Kept in the immutable branch.created receipt alongside the Branch row's git_ref.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GitRefBinding {
    pub requested_ref: String,
    pub resolved_ref: Option<String>,
    pub commit_sha: String,
    pub repository_root: PathBuf,
    pub observed_at: i64,
}
impl GitRefBinding {
    pub fn validate(&self) -> Result<()> {
        validate_git_ref(&self.requested_ref)?;
        if !is_source_sha(&self.commit_sha)
            || !self.repository_root.is_absolute()
            || self.observed_at < 0
            || self
                .resolved_ref
                .as_ref()
                .is_some_and(|r| !r.starts_with("refs/") || validate_git_ref(r).is_err())
        {
            return Err(Error::InvalidInput("Git binding requires a full commit SHA, absolute repository root, verification time and valid ref".into()));
        }
        Ok(())
    }
}
pub fn validate_git_ref(reference: &str) -> Result<()> {
    if reference.trim() != reference
        || reference.is_empty()
        || reference.len() > 4096
        || reference.starts_with('-')
        || reference.chars().any(char::is_control)
    {
        return Err(Error::InvalidInput("Git ref must contain 1..4096 bytes without outer whitespace, control characters or a leading dash".into()));
    }
    Ok(())
}
pub fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.trim() != name
        || name.len() > 128
        || name.chars().any(char::is_control)
        || name == "main"
    {
        return Err(Error::InvalidInput("work branch name must contain 1..128 bytes without outer whitespace or control characters; main is the existing baseline".into()));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct BranchDraft {
    pub name: String,
    /// None explicitly selects the existing main baseline. The runtime resolves defaults.
    pub parent_branch_id: Option<Id>,
    pub git_binding: Option<GitRefBinding>,
    pub actor: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRecord {
    pub branch: Branch,
    /// Missing for legacy rows; a stored ref without this receipt is not verified Git data.
    pub git_binding: Option<GitRefBinding>,
    pub creation_event_id: Option<Id>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BranchPage {
    pub project_id: Id,
    pub project_revision: Revision,
    pub current_branch_id: Option<Id>,
    pub branches: Vec<Branch>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct BranchSwitched {
    pub previous_branch_id: Option<Id>,
    pub current_branch_id: Option<Id>,
    /// Existing runtime identities stay on their original branch, including main/None.
    pub retained_active_sessions: usize,
    pub retained_active_claims: usize,
}
