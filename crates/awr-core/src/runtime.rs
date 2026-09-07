use crate::{Checkpoint, Claim, Id, Session};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDraft {
    pub context_hash: String,
    pub digest: String,
    pub next_action: String,
    pub open_loops: Vec<String>,
    pub changed_entities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDraft {
    pub artifact_type: String,
    pub locator: String,
    pub sha256: String,
    pub size: u64,
    pub mime: String,
    pub source_event_id: Id,
}

pub fn is_sha256_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDraft {
    pub work_item_key: Option<String>,
    pub agent_id: String,
    pub provider: String,
    pub model: String,
    pub branch_id: Option<Id>,
    pub claim: bool,
    pub claim_ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStarted {
    pub session: Session,
    pub claim: Option<Claim>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub from_session: Session,
    pub to_session: Option<Session>,
    pub checkpoint: Checkpoint,
    pub closed_claim_ids: Vec<Id>,
    pub transferred_claim: Option<Claim>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionOutcome {
    Ended,
    Interrupted,
    Incomplete,
}
impl SessionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::Interrupted => "interrupted",
            Self::Incomplete => "incomplete",
        }
    }
}
impl Claim {
    pub fn active_at(&self, at: i64) -> bool {
        self.status == "active"
            && self.released_at.is_none()
            && self.expires_at.is_none_or(|end| end > at)
    }
}
