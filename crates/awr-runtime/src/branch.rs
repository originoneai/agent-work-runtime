use awr_core::*;
use awr_store::Store;
use std::{path::Path, process::Command};

#[derive(Debug, Clone)]
pub struct CreateBranchRequest {
    pub name: String,
    /// Omitted selects the current work branch; "main" explicitly selects the baseline.
    pub parent: Option<String>,
    /// Omitted leaves Git unbound. Supplied refs must resolve to an existing local commit.
    pub git_ref: Option<String>,
    pub expected_revision: Revision,
    pub actor: String,
    pub reason: String,
}
pub(crate) fn git_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(args);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(key);
    }
    command
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = git_command(root, args).output()?;
    if !output.status.success() {
        return Err(Error::InvalidInput(format!(
            "cannot resolve local Git binding: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    if output.stdout.len() > 65536 {
        return Err(Error::InvalidInput(
            "Git reference output exceeds 64 KiB".into(),
        ));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.strip_suffix('\n').unwrap_or(&s).to_string())
        .map_err(|_| Error::InvalidInput("Git reference output is not UTF-8".into()))
}
/// No checkout, Git branch creation, fetch, source reindex or file write occurs here.
pub fn observe_git_ref(root: &Path, reference: &str) -> Result<GitRefBinding> {
    validate_git_ref(reference)?;
    let root = root.canonicalize()?;
    let repository_root =
        std::path::PathBuf::from(git(&root, &["rev-parse", "--show-toplevel"])?).canonicalize()?;
    if !root.starts_with(&repository_root) {
        return Err(Error::SourceConflict(
            "Git repository does not contain the selected project root".into(),
        ));
    }
    let expression = format!("{reference}^{{commit}}");
    let commit_sha = git(
        &root,
        &["rev-parse", "--verify", "--end-of-options", &expression],
    )?;
    let symbolic = git(
        &root,
        &[
            "rev-parse",
            "--symbolic-full-name",
            "--verify",
            "--end-of-options",
            reference,
        ],
    )?;
    let repeated = git(
        &root,
        &["rev-parse", "--verify", "--end-of-options", &expression],
    )?;
    if commit_sha != repeated {
        return Err(Error::SourceConflict(
            "Git ref moved while its binding was observed".into(),
        ));
    }
    let binding = GitRefBinding {
        requested_ref: reference.into(),
        resolved_ref: symbolic.starts_with("refs/").then_some(symbolic),
        commit_sha,
        repository_root,
        observed_at: now_millis()?,
    };
    binding.validate()?;
    Ok(binding)
}
pub fn create_branch(
    store: &mut Store,
    root: &Path,
    request: &CreateBranchRequest,
) -> Result<(Branch, Event)> {
    let root = root.canonicalize()?;
    let project = store.project_by_root(&root)?;
    if project.project_revision != request.expected_revision {
        return Err(Error::RevisionConflict {
            expected: request.expected_revision,
            actual: project.project_revision,
        });
    }
    validate_branch_name(&request.name)?;
    let parent = match &request.parent {
        Some(reference) => store.resolve_branch(project.id, reference)?,
        None => project.current_branch_id,
    };
    let binding = request
        .git_ref
        .as_deref()
        .map(|reference| observe_git_ref(&root, reference))
        .transpose()?;
    store.create_branch(
        project.id,
        request.expected_revision,
        BranchDraft {
            name: request.name.clone(),
            parent_branch_id: parent,
            git_binding: binding,
            actor: request.actor.clone(),
            reason: request.reason.clone(),
        },
    )
}
pub fn switch_branch(
    store: &mut Store,
    root: &Path,
    reference: &str,
    expected: Revision,
    actor: &str,
    reason: &str,
) -> Result<(BranchSwitched, Event)> {
    let project = store.project_by_root(&root.canonicalize()?)?;
    if project.project_revision != expected {
        return Err(Error::RevisionConflict {
            expected,
            actual: project.project_revision,
        });
    }
    let branch = store.resolve_branch(project.id, reference)?;
    store.switch_branch(project.id, expected, branch, actor, reason)
}
