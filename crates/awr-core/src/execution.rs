use crate::{Id, Revision};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorKind {
    ManagedLocal,
    External,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Registered,
    Starting,
    Running,
    Succeeded,
    Failed,
}
impl ExecutionState {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

/// Immutable intent. An operation key is project-wide, including across session handoffs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionIntent {
    pub operation_key: String,
    pub purpose: String,
    pub executor: ExecutorKind,
    pub command: Vec<String>,
    pub cwd: String,
    pub external_reference: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkerIdentity {
    pub nonce: Id,
    pub pid: u32,
    pub port: u16,
    pub child_pid: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub id: Id,
    pub project_id: Id,
    pub work_item_id: Id,
    pub session_id: Id,
    pub branch_id: Option<Id>,
    pub revision: Revision,
    pub intent: ExecutionIntent,
    pub state: ExecutionState,
    pub worker: Option<WorkerIdentity>,
    pub registered_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub error: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub receipt: Option<String>,
}

/// Written by the owned supervisor after wait/try_wait observes its direct child exit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionResult {
    pub execution_id: Id,
    pub nonce: Id,
    pub finished_at: i64,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionProbe {
    pub execution_id: Id,
    pub nonce: Id,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionProbeReply {
    pub execution_id: Id,
    pub nonce: Id,
    pub worker_pid: u32,
    pub child_pid: u32,
    pub observed_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservedExecutionState {
    Running,
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionObservation {
    pub execution_id: Id,
    pub operation_key: String,
    pub purpose: String,
    pub origin_session_id: Id,
    pub branch_id: Option<Id>,
    pub recorded_revision: Revision,
    pub recorded_state: ExecutionState,
    pub state: ObservedExecutionState,
    pub verified: bool,
    pub observed_at: i64,
    pub evidence_at: Option<i64>,
    pub basis: String,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub error: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub receipt: Option<String>,
    pub next_action: String,
}
impl Execution {
    /// Deterministic journal facts for hashed L0/L1 context; live probes are separate observations.
    pub fn continuity_text(&self) -> crate::Result<String> {
        Ok(format!(
            "Execution: {} | operation: {} | purpose: {}\nOrigin session: {} | branch: {} | revision: {}\nRecorded state: {} | started: {:?} | finished: {:?} | exit: {:?} | signal: {:?}\nLogs: {}; {} | receipt: {}\n{}",
            self.id,
            serde_json::to_string(&self.intent.operation_key)?,
            serde_json::to_string(&self.intent.purpose)?,
            self.session_id,
            self.branch_id
                .map(|v| v.to_string())
                .unwrap_or_else(|| "main".into()),
            self.revision,
            serde_json::to_string(&self.state)?,
            self.started_at,
            self.finished_at,
            self.exit_code,
            self.signal,
            self.stdout.as_deref().unwrap_or("none"),
            self.stderr.as_deref().unwrap_or("none"),
            self.receipt.as_deref().unwrap_or("none"),
            if self.state.terminal() {
                "A supervisor completion is recorded; read the result and logs before advancing work."
            } else {
                "Current outcome is unverified by context compilation. Use awr execution inspect before acting; do not automatically retry or kill this operation."
            }
        ))
    }
}
