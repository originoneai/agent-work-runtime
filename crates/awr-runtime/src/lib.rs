//! Agent runtime operations over an explicitly selected project.
//! Callers refresh source projections before acquisition. Cleanup remains possible with stale sources.
use awr_core::{Claim, Event, Id, Revision, Session, SessionDraft, SessionOutcome, SessionStarted};
pub use awr_core::{Error, Result};
mod artifact;
mod read;
pub use artifact::ArtifactFile;
use awr_store::Store;
pub use awr_store::{BranchFilter, EventCursor, EventPage, EventQuery};

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
