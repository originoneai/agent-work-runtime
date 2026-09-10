use awr_core::*;
use awr_store::Store;

/// A source assertion alone cannot manufacture the protected runtime confirmation receipt.
pub fn assess_ordinary_completion(
    store: &Store,
    project: Id,
    work: &Projected<WorkItem>,
) -> Result<()> {
    let receipt = work
        .item
        .ordinary_completion
        .as_ref()
        .ok_or_else(|| Error::EvidenceMissing("no ordinary completion receipt".into()))?;
    receipt.validate(&work.item.meta.external_key, &work.item.acceptance)?;
    if work.item.status != WorkStatus::Completed
        || work.source.freshness != Freshness::Fresh
        || OrdinaryWorkPolicy::from_config(&work.source.config)?.as_ref() != Some(&receipt.policy)
    {
        return Err(Error::SourceConflict(
            "ordinary confirmation no longer matches current work or policy".into(),
        ));
    }
    let proposal = store
        .host_proposal(project, &receipt.request_key)?
        .ok_or_else(|| {
            Error::EvidenceMissing("source assertion has no applied host confirmation".into())
        })?;
    let patch = proposal.bound_patch()?;
    if proposal.status != ProposalStatus::Applied
        || patch.target.meta.id != work.item.meta.id
        || patch.target.meta.source_ref.source_id != work.source.id
        || !patch
            .host_edit
            .as_ref()
            .is_some_and(|h| h.action == HostEditAction::ConfirmOrdinary)
        || patch.changes["ordinary_completion"] != serde_json::to_value(receipt)?
    {
        return Err(Error::EvidenceMissing(
            "source confirmation differs from its applied runtime receipt".into(),
        ));
    }
    store.check_completion_dependencies(project, &work.item.meta.external_key, None)?;
    for artifact in &receipt.artifacts {
        crate::read::read_registered_file(
            store,
            project,
            &artifact.locator,
            None,
            Some(&artifact.sha256),
            1024 * 1024,
        )?;
    }
    Ok(())
}
