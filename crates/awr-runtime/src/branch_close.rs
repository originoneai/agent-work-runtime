use awr_core::*;
use awr_source::{Manifest, index_project};
use awr_store::Store;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
};

#[derive(Debug, Clone)]
pub struct CloseBranchRequest {
    pub reference: String,
    /// Work-runtime destination, independent of physical Git refs. Defaults to main in CLI.
    pub into: String,
    pub expected_revision: Revision,
    pub actor: String,
    pub reason: String,
    pub input: CloseBranchInput,
}

fn source_merge_report(
    root: &Path,
    locator: &str,
    expected_hash: &str,
) -> Result<BranchMergeObservation> {
    let path = Path::new(locator);
    if path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(Error::InvalidInput(
            "source merge report must be a relative project file".into(),
        ));
    }
    let path = root.join(path).canonicalize()?;
    if !path.starts_with(root) {
        return Err(Error::InvalidInput(
            "source merge report escapes project root".into(),
        ));
    }
    let file = File::open(&path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > 1048576 {
        return Err(Error::InvalidInput(
            "source merge report must be a nonempty file of at most 1 MiB".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(1048577).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > 1048576 {
        return Err(Error::InvalidInput(
            "source merge report changed size or exceeds 1 MiB".into(),
        ));
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if !hash.eq_ignore_ascii_case(expected_hash) {
        return Err(Error::SourceConflict(
            "source merge report SHA256 differs from the request".into(),
        ));
    }
    Ok(BranchMergeObservation::Source {
        locator: locator.into(),
        sha256: hash,
        size: bytes.len() as u64,
        observed_at: now_millis()?,
    })
}

fn observe_merge(root: &Path, input: &BranchMergeInput) -> Result<BranchMergeObservation> {
    match input {
        BranchMergeInput::Source { locator, sha256 } => source_merge_report(root, locator, sha256),
        BranchMergeInput::Git {
            source_ref,
            target_ref,
        } => {
            let source = crate::observe_git_ref(root, source_ref)?;
            let target = crate::observe_git_ref(root, target_ref)?;
            let head = crate::observe_git_ref(root, "HEAD")?;
            if source.repository_root != target.repository_root
                || head.commit_sha != target.commit_sha
            {
                return Err(Error::SourceConflict(
                    "Git merge target must be the current project checkout HEAD".into(),
                ));
            }
            let ancestry = crate::branch::git_command(
                root,
                &[
                    "merge-base",
                    "--is-ancestor",
                    &source.commit_sha,
                    &target.commit_sha,
                ],
            )
            .output()?;
            if !ancestry.status.success() {
                return Err(Error::SourceConflict("Git source tip is not an ancestor of the checked-out target, or ancestry could not be verified".into()));
            }
            let clean =
                crate::branch::git_command(root, &["diff", "--quiet", "HEAD", "--"]).output()?;
            if !clean.status.success() {
                return Err(Error::SourceConflict(
                    "commit or resolve tracked Git changes before recording a Git merge closure"
                        .into(),
                ));
            }
            Ok(BranchMergeObservation::Git {
                source,
                target,
                head_sha: head.commit_sha,
            })
        }
    }
}
fn same_observation(
    a: &Option<BranchMergeObservation>,
    b: &Option<BranchMergeObservation>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (
            Some(BranchMergeObservation::Git {
                source: a,
                target: b,
                head_sha: c,
            }),
            Some(BranchMergeObservation::Git {
                source: x,
                target: y,
                head_sha: z,
            }),
        ) => {
            a.commit_sha == x.commit_sha
                && a.resolved_ref == x.resolved_ref
                && a.repository_root == x.repository_root
                && b.commit_sha == y.commit_sha
                && b.resolved_ref == y.resolved_ref
                && b.repository_root == y.repository_root
                && c == z
        }
        (
            Some(BranchMergeObservation::Source {
                locator: a,
                sha256: b,
                size: c,
                ..
            }),
            Some(BranchMergeObservation::Source {
                locator: x,
                sha256: y,
                size: z,
                ..
            }),
        ) => a == x && b == y && c == z,
        _ => false,
    }
}

/// Verify an external merge, reindex Source, then atomically record summary and close.
/// If refresh advances revision, keep the refreshed projections and require a reviewed retry;
/// no branch summary, status change, session cleanup or Git/Source write has occurred.
pub fn close_branch(
    store: &mut Store,
    root: &Path,
    request: &CloseBranchRequest,
) -> Result<(BranchClosed, Event)> {
    request.input.validate()?;
    if request.actor.trim().is_empty()
        || request.actor.len() > 256
        || request.reason.trim().is_empty()
        || request.reason.len() > 4096
    {
        return Err(Error::InvalidInput(
            "branch closure needs actor (1..256 bytes) and reason (1..4096 bytes)".into(),
        ));
    }
    let root = root.canonicalize()?;
    let project = store.project_by_root(&root)?;
    if project.project_revision != request.expected_revision {
        return Err(Error::RevisionConflict {
            expected: request.expected_revision,
            actual: project.project_revision,
        });
    }
    let branch = store
        .resolve_branch(project.id, &request.reference)?
        .ok_or_else(|| {
            Error::InvalidInput("main is the persistent baseline and cannot close".into())
        })?;
    let target = store.resolve_branch(project.id, &request.into)?;
    if target == Some(branch) {
        return Err(Error::InvalidInput(
            "branch cannot close into itself".into(),
        ));
    }
    let plan = store.branch_close_plan(project.id, branch)?;
    if !plan.blockers.is_empty() {
        return Err(Error::InvalidTransition(plan.blockers.join("; ")));
    }
    let observed = request
        .input
        .merge
        .as_ref()
        .map(|m| observe_merge(&root, m))
        .transpose()?;
    let manifest = Manifest::load(&root)?;
    let refresh = index_project(store, &root, &manifest, false)?;
    if !refresh.ok || refresh.pending > 0 {
        return Err(Error::SourceStale(format!(
            "branch closure source refresh failed: {}",
            serde_json::to_string(&refresh.issues)?
        )));
    }
    if refresh.project_revision != request.expected_revision {
        return Err(Error::RevisionConflict {
            expected: request.expected_revision,
            actual: refresh.project_revision,
        });
    }
    let versions = store
        .sources(project.id)?
        .into_iter()
        .map(BranchSourceVersion::from)
        .collect();
    // Recheck files/manifest and Git or the source report after indexing, before final DB CAS.
    let recheck = index_project(store, &root, &Manifest::load(&root)?, false)?;
    if !recheck.ok || recheck.pending > 0 {
        return Err(Error::SourceStale(
            "sources became unavailable during branch closure".into(),
        ));
    }
    if recheck.project_revision != request.expected_revision {
        return Err(Error::RevisionConflict {
            expected: request.expected_revision,
            actual: recheck.project_revision,
        });
    }
    let repeated = request
        .input
        .merge
        .as_ref()
        .map(|m| observe_merge(&root, m))
        .transpose()?;
    if !same_observation(&observed, &repeated) {
        return Err(Error::SourceConflict(
            "external merge references moved during source refresh".into(),
        ));
    }
    store.close_branch(
        project.id,
        branch,
        request.expected_revision,
        CloseBranchDraft {
            input: request.input.clone(),
            target_branch_id: target,
            actor: request.actor.clone(),
            reason: request.reason.clone(),
            source_versions: versions,
            merge: repeated,
        },
    )
}
