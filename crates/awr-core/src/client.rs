use crate::{Id, Revision};
use serde::{Deserialize, Serialize};

/// Work continuity attached to a client conversation, not a copy of that conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientBinding {
    pub client: String,
    pub external_session: String,
    pub session_id: Id,
    pub revision: Revision,
    pub digest: String,
    pub next_action: String,
    pub open_loops: Vec<String>,
    pub context_hash: Option<String>,
    pub context_revision: Option<Revision>,
    pub progress_revision: Revision,
    pub last_delivery_key: Option<String>,
    pub last_hook_event: Option<String>,
    pub checkpoint_id: Option<Id>,
    pub observed_at: i64,
}
