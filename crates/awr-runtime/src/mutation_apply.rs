use crate::mutation::{ProposalReport, ReviewProposalRequest, report};
use awr_core::*;
use awr_source::{
    Locator, fingerprint, inspect_mutation_source, parse_mutation_projection,
    prepare_yaml_mutation, read_capped, verify_mutation_source,
};
use awr_store::Store;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

fn directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(Error::RuleViolation(
            "mutation recovery directory must be a real directory".into(),
        ));
    }
    Ok(())
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    Ok(())
}
fn recovery_root(root: &Path) -> Result<PathBuf> {
    let runtime = root.join(".awr");
    let meta = fs::symlink_metadata(&runtime)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(Error::RuleViolation(
            "runtime directory must not be a symlink".into(),
        ));
    }
    let path = runtime.join("mutations");
    directory(&path)?;
    Ok(path)
}
fn source_lock(root: &Path, source: Id) -> Result<File> {
    let path = recovery_root(root)?.join(format!("{source}.lock"));
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink() || !m.is_file()) {
        return Err(Error::RuleViolation(
            "source lock must be a regular file".into(),
        ));
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock().map_err(|e| {
        Error::MutationConflict(format!("another writer holds the source lock: {e}"))
    })?;
    Ok(file)
}
fn create_file(path: &Path, bytes: &[u8], permissions: Option<fs::Permissions>) -> Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    file.sync_all()?;
    Ok(())
}
fn stage(root: &Path, plan: &MutationWritePlan, before: &[u8], after: &[u8]) -> Result<()> {
    let path = recovery_root(root)?.join(plan.id.to_string());
    fs::create_dir(&path)?;
    create_file(&path.join("before.yaml"), before, None)?;
    create_file(&path.join("after.yaml"), after, None)?;
    create_file(
        &path.join("plan.json"),
        &serde_json::to_vec_pretty(plan)?,
        None,
    )?;
    sync_directory(&path)?;
    sync_directory(path.parent().unwrap())
}
fn stored_after(root: &Path, plan: &MutationWritePlan) -> Result<Vec<u8>> {
    let path = root.join(plan.recovery_directory());
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(Error::RuleViolation(
            "recovery directory was replaced".into(),
        ));
    }
    let mut after = None;
    for (name, size, fp) in [
        ("before.yaml", plan.before_size, &plan.before_fingerprint),
        ("after.yaml", plan.after_size, &plan.after_fingerprint),
    ] {
        let file = path.join(name);
        let meta = fs::symlink_metadata(&file)?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err(Error::RuleViolation(
                "recovery snapshot must be a regular file".into(),
            ));
        }
        let bytes = read_capped(&file, 16 * 1024 * 1024)?;
        if bytes.len() as u64 != size || fingerprint(&bytes) != *fp {
            return Err(Error::SourceConflict(
                "recovery snapshot differs from its immutable write plan".into(),
            ));
        }
        if name == "after.yaml" {
            after = Some(bytes);
        }
    }
    Ok(after.unwrap())
}
fn decorate(
    mut result: ProposalReport,
    store: &Store,
    project: Id,
    attempt: &MutationApplyAttempt,
    wrote: Option<bool>,
    outcome: &'static str,
) -> Result<ProposalReport> {
    result.project_revision = store.project(project)?.project_revision;
    result.source_write_performed = wrote;
    result.write_outcome = outcome;
    result.recovery_directory = Some(attempt.plan.recovery_directory());
    result.apply_attempt = store.proposal_apply_attempt(project, attempt.proposal_id)?;
    Ok(result)
}
fn pending(
    store: &Store,
    project: Id,
    attempt: &MutationApplyAttempt,
    wrote: Option<bool>,
    reason: String,
) -> Result<ProposalReport> {
    let failure = Error::MutationIncomplete {
        proposal_id: attempt.proposal_id,
        attempt_event_id: attempt.event_id,
        reason,
    };
    decorate(
        report(
            store.proposal(project, attempt.proposal_id)?,
            store.event(project, attempt.event_id)?,
            false,
            None,
            Some(failure),
        ),
        store,
        project,
        attempt,
        wrote,
        "pending_recovery",
    )
}
fn preparation_failure(
    store: &mut Store,
    project: Id,
    request: &ReviewProposalRequest,
    error: Error,
) -> Result<ProposalReport> {
    let (action, error) = match error {
        Error::MutationUnsupported(reason) => (
            ProposalAction::RequireManualApply,
            Error::ProposalRequired {
                proposal_id: request.proposal_id,
                reason,
            },
        ),
        e @ (Error::SourceConflict(_) | Error::SourceStale(_)) => (ProposalAction::Conflict, e),
        e => (ProposalAction::Fail, e),
    };
    let (proposal, event) = store.review_proposal(
        project,
        request.expected_revision,
        request.proposal_id,
        action,
        &request.actor,
        &format!("{}; {error}", request.reason),
    )?;
    Ok(report(proposal, event, false, None, Some(error)))
}
fn stopped(
    store: &mut Store,
    project: Id,
    request: &ReviewProposalRequest,
    attempt: &MutationApplyAttempt,
    wrote: Option<bool>,
    error: Error,
) -> Result<ProposalReport> {
    // A storage/finalization failure must retain the open attempt for exact recovery, even
    // when the source already has the planned bytes. Never conceal that fact with a bare error.
    if matches!(error, Error::Storage(_) | Error::RevisionConflict { .. }) {
        return pending(store, project, attempt, wrote, error.to_string());
    }
    let conflict = matches!(error, Error::SourceConflict(_) | Error::SourceStale(_));
    if wrote != Some(false) {
        let source = store.source(project, attempt.source_id)?;
        if let Err(mark) = store.mark_source_freshness(&source, Freshness::Stale) {
            return pending(
                store,
                project,
                attempt,
                wrote,
                format!("{error}; cache freshness could not be updated: {mark}"),
            );
        }
    }
    let expected = store.project(project)?.project_revision;
    match store.fail_proposal_apply(
        project,
        expected,
        request.proposal_id,
        attempt.event_id,
        conflict,
        &request.actor,
        &format!("{}; {error}", request.reason),
    ) {
        Ok((proposal, event)) => decorate(
            report(proposal, event, false, None, Some(error)),
            store,
            project,
            attempt,
            wrote,
            "stopped",
        ),
        Err(failure) => pending(
            store,
            project,
            attempt,
            wrote,
            format!("{error}; could not record stop: {failure}"),
        ),
    }
}

pub(crate) fn apply(
    store: &mut Store,
    root: &Path,
    request: &ReviewProposalRequest,
    recover: bool,
) -> Result<ProposalReport> {
    if request.actor.trim().is_empty()
        || request.actor.len() > 256
        || request.reason.trim().is_empty()
        || request.reason.len() > 4096
    {
        return Err(Error::InvalidInput(
            "application requires an actor of 1..256 bytes and reason of 1..4096 bytes".into(),
        ));
    }
    let project = store.project_by_root(root)?.id;
    let proposal = store.proposal(project, request.proposal_id)?;
    ProposalAction::RequireManualApply.next_status(proposal.status)?;
    let _lock = source_lock(root, proposal.source_id)?;
    let existing = store
        .proposal_apply_attempt(project, proposal.id)?
        .filter(|attempt| attempt.resolved_event_id.is_none());
    if !recover {
        if let Some(attempt) = existing {
            return pending(
                store,
                project,
                &attempt,
                None,
                "use proposal recover to inspect and resume the recorded attempt".into(),
            );
        }
    }
    let patch = proposal.bound_patch()?;
    let source = store.source(project, proposal.source_id)?;
    let attempt = if recover {
        existing.ok_or_else(|| {
            Error::InvalidTransition("proposal has no unfinished application to recover".into())
        })?
    } else {
        let prepared = match (|| {
            verify_mutation_source(root, &source, &patch)?;
            crate::work_action::verify_work_dependencies(store, root, project, &patch)?;
            prepare_yaml_mutation(root, &source, &proposal, store.projection_ids(&source)?)
        })() {
            Ok(prepared) => prepared,
            Err(error) => return preparation_failure(store, project, request, error),
        };
        if fs::metadata(&prepared.path)?.permissions().readonly() {
            return preparation_failure(
                store,
                project,
                request,
                Error::SourceUnavailable("source file is read-only".into()),
            );
        }
        stage(
            root,
            &prepared.plan,
            &prepared.before.bytes,
            &prepared.after.bytes,
        )?;
        store
            .begin_proposal_apply(
                project,
                request.expected_revision,
                proposal.id,
                prepared.plan,
                &request.actor,
                &request.reason,
            )?
            .0
    };
    let mut wrote = Some(false);
    let mut refreshed = false;
    let result = (|| {
        let after = stored_after(root, &attempt.plan)?;
        let source = store.source(project, proposal.source_id)?;
        let (locator, _, snapshot) = inspect_mutation_source(root, &source, &patch)?;
        let Locator::File(path) = locator else {
            return Err(Error::MutationUnsupported(
                "Git sources remain read-only".into(),
            ));
        };
        if path.starts_with(root.join(".awr")) {
            return Err(Error::RuleViolation(
                "runtime-owned files cannot be rewritten".into(),
            ));
        }
        if snapshot.fingerprint == attempt.plan.before_fingerprint {
            verify_mutation_source(root, &source, &patch)?;
            let permissions = fs::metadata(&path)?.permissions();
            if permissions.readonly() {
                return Err(Error::SourceUnavailable("source file is read-only".into()));
            }
            let temp = path
                .parent()
                .unwrap()
                .join(format!(".awr-write-{}.tmp", Id::new()));
            create_file(&temp, &after, Some(permissions))?;
            let before_replace = (|| {
                if patch.work_action.is_some() {
                    store.check_work_proposal(project, &proposal)?;
                    crate::work_action::verify_work_dependencies(store, root, project, &patch)?;
                }
                let (current, _, observed) = inspect_mutation_source(root, &source, &patch)?;
                if !matches!(current,Locator::File(ref current) if current==&path)
                    || observed.fingerprint != attempt.plan.before_fingerprint
                {
                    return Err(Error::SourceConflict(
                        "source changed immediately before replacement".into(),
                    ));
                }
                // The current source must still refer to the original bytes at this last check.
                let actual = store.project(project)?.project_revision;
                let expected = if recover {
                    request.expected_revision
                } else {
                    attempt.project_revision
                };
                if actual != expected {
                    return Err(Error::RevisionConflict { expected, actual });
                }
                wrote = None;
                fs::rename(&temp, &path)?;
                wrote = Some(true);
                sync_directory(path.parent().unwrap())
            })();
            if temp.is_file() {
                let _ = fs::remove_file(&temp);
            }
            before_replace?;
        } else if snapshot.fingerprint != attempt.plan.after_fingerprint {
            return Err(Error::SourceConflict("current source matches neither recorded snapshot; retained both snapshots without overwriting it".into()));
        }
        let source = store.source(project, proposal.source_id)?;
        let (_, spec, observed) = inspect_mutation_source(root, &source, &patch)?;
        if observed.fingerprint != attempt.plan.after_fingerprint {
            return Err(Error::SourceConflict(
                "source differs from the intended result after replacement".into(),
            ));
        }
        let (batch, hash) = parse_mutation_projection(
            &source,
            &spec,
            &observed,
            store.projection_ids(&source)?,
            &patch.target,
        )?;
        if hash != attempt.plan.target_after_hash {
            return Err(Error::SourceConflict(
                "reparsed target differs from the intended facts".into(),
            ));
        }
        let projected = store.commit_source_projection(&source, &observed.fingerprint, batch)?;
        refreshed = projected.revision != source.revision;
        let source = store.source(project, proposal.source_id)?;
        let (_, _, latest) = inspect_mutation_source(root, &source, &patch)?;
        if latest.fingerprint != attempt.plan.after_fingerprint {
            return Err(Error::SourceConflict(
                "source changed while its projection was rebuilt".into(),
            ));
        }
        let revision = store.project(project)?.project_revision;
        store.finish_proposal_apply(
            project,
            revision,
            proposal.id,
            attempt.event_id,
            &request.actor,
            &request.reason,
        )
    })();
    let mut result = match result {
        Ok((proposal, event)) => decorate(
            report(proposal, event, false, None, None),
            store,
            project,
            &attempt,
            wrote,
            if recover { "recovered" } else { "applied" },
        ),
        Err(error) => stopped(store, project, request, &attempt, wrote, error),
    }?;
    result.source_refresh_performed = refreshed;
    Ok(result)
}
