//! Agent runtime operations over an explicitly selected project.
//! Callers refresh source projections before acquisition. Cleanup remains possible with stale sources.
use awr_core::{Claim, Event, Id, Revision, Session, SessionDraft, SessionOutcome, SessionStarted};
pub use awr_core::{Error, Result};
mod artifact;
mod branch;
mod branch_close;
mod completion;
mod doctor;
mod execution;
mod fs_sync;
mod mutation;
mod mutation_apply;
mod organization;
mod read;
mod resume;
mod work_action;
pub use artifact::ArtifactFile;
use awr_store::Store;
pub use awr_store::{BranchFilter, EventCursor, EventPage, EventQuery};
pub use branch::{CreateBranchRequest, create_branch, observe_git_ref, switch_branch};
pub use branch_close::{CloseBranchRequest, close_branch};
pub use completion::{CompleteWorkRequest, complete_work};
pub use doctor::{ProjectDoctorReport, diagnose_project};
pub use execution::{inspect_execution, inspect_work_executions, render_execution_observations};
pub use mutation::{
    CreateProposalRequest, ProposalReport, ReviewProposalAction, ReviewProposalRequest,
    create_proposal, review_proposal,
};
pub use organization::{OrganizationReport, OrganizationState, inspect_organization};
pub use resume::{ResumeReport, ResumeRequest, resume_session};
pub use work_action::{WorkActionRequest, perform_work_action};

/// Hard ceilings; a caller may select a smaller budget, never raise these limits.
pub const ARTIFACT_IMPORT_CAP: u64 = 64 * 1024 * 1024;
pub const REGISTERED_CONTENT_READ_CAP: u64 = 16 * 1024 * 1024;

pub struct Runtime<'a> {
    store: &'a mut Store,
    project: Id,
}
impl<'a> Runtime<'a> {
    pub fn record_evidence(
        &mut self,
        expected: Revision,
        draft: awr_core::EvidenceDraft,
    ) -> Result<(awr_core::Evidence, Event)> {
        self.store.record_evidence(self.project, expected, draft)
    }
    pub fn handoff(
        &mut self,
        expected: Revision,
        from: Id,
        to: Option<Id>,
        ttl_ms: Option<u64>,
    ) -> Result<(awr_core::Handoff, Event)> {
        self.store.handoff(self.project, expected, from, to, ttl_ms)
    }
    pub fn checkpoint(
        &mut self,
        expected: Revision,
        session: Id,
        draft: awr_core::CheckpointDraft,
    ) -> Result<(awr_core::Checkpoint, Event)> {
        let started = self
            .store
            .begin_checkpoint_save(self.project, expected, session, draft)?;
        self.store
            .finish_checkpoint_save(self.project, started.project_revision, started.id)
            .map_err(|error| Error::CheckpointIncomplete {
                attempt_id: started.id,
                reason: error.to_string(),
            })
    }
    pub fn append_event(
        &mut self,
        expected: Revision,
        draft: awr_core::EventDraft,
    ) -> Result<Event> {
        self.store.append_event(self.project, expected, draft)
    }
    /// Preserve an explicit event branch selection, including main.
    pub fn append_event_in_branch(
        &mut self,
        expected: Revision,
        draft: awr_core::EventDraft,
    ) -> Result<Event> {
        self.store
            .append_event_in_branch(self.project, expected, draft)
    }
    pub fn events(&self, query: &EventQuery) -> Result<EventPage> {
        self.store.query_events(self.project, query)
    }
    pub fn attach(store: &'a mut Store, project: Id) -> Result<Self> {
        store.project(project)?;
        Ok(Self { store, project })
    }
    pub fn start_session(
        &mut self,
        expected: Revision,
        draft: SessionDraft,
    ) -> Result<(SessionStarted, Event)> {
        self.store.start_session(self.project, expected, draft)
    }
    pub fn acquire_claim(
        &mut self,
        expected: Revision,
        session: Id,
        ttl_ms: Option<u64>,
    ) -> Result<(Claim, Event)> {
        self.store
            .acquire_claim(self.project, expected, session, ttl_ms)
    }
    pub fn release_claim(
        &mut self,
        expected: Revision,
        session: Id,
        claim: Id,
    ) -> Result<(Claim, Event)> {
        self.store
            .release_claim(self.project, expected, session, claim)
    }
    pub fn end_session(
        &mut self,
        expected: Revision,
        session: Id,
        outcome: SessionOutcome,
    ) -> Result<(Session, Event)> {
        self.store
            .end_session(self.project, expected, session, outcome)
    }
}
