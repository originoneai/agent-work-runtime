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
