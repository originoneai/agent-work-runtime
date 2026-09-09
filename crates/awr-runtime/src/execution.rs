//! Read-only recovery of an execution's evidence. A numeric PID never grants liveness or control.
use awr_core::*;
use awr_store::Store;
use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};

fn unknown(e: &Execution, basis: &str) -> Result<ExecutionObservation> {
    Ok(ExecutionObservation{execution_id:e.id,operation_key:e.intent.operation_key.clone(),purpose:e.intent.purpose.clone(),origin_session_id:e.session_id,branch_id:e.branch_id,recorded_revision:e.revision,state:ObservedExecutionState::Unknown,recorded_state:e.state,verified:false,observed_at:now_millis()?,evidence_at:None,basis:basis.into(),exit_code:None,signal:None,error:None,stdout:e.stdout.clone(),stderr:e.stderr.clone(),receipt:e.receipt.clone(),next_action:"Verify the original executor and any side effects before choosing a new operation. No automatic retry or process termination.".into()})
}
fn outcome(
    mut o: ExecutionObservation,
    result: ExecutionResult,
    basis: &str,
) -> ExecutionObservation {
    o.state = if result.success {
        ObservedExecutionState::Succeeded
    } else {
        ObservedExecutionState::Failed
    };
    o.verified = true;
    o.evidence_at = Some(result.finished_at);
    o.basis = basis.into();
    o.exit_code = result.exit_code;
    o.signal = result.signal;
    o.error = result.error;
    o.next_action=if result.success{"Read the recorded result and logs, then continue the dependent work. Do not repeat the operation."}else{"Inspect the failure and its side effects before planning a new operation with a new key."}.into();
    o
}
fn receipt(root: &Path, e: &Execution) -> Result<Option<ExecutionResult>> {
    let path = root.join(format!(".awr/executions/{}/result.json", e.id));
    match fs::symlink_metadata(&path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
        Ok(metadata) if !metadata.is_file() => {
            return Err(Error::InvalidInput("receipt is not a regular file".into()));
        }
        _ => {}
    }
    let bytes = awr_source::read_capped(&path, 64 * 1024)?;
    let r: ExecutionResult = serde_json::from_slice(&bytes)
        .map_err(|_| Error::InvalidInput("invalid execution receipt".into()))?;
    ensure_public_data(&r)?;
    if r.execution_id != e.id
        || e.worker.as_ref().is_none_or(|w| w.nonce != r.nonce)
        || r.finished_at < e.started_at.unwrap_or(e.registered_at)
        || (r.success && (r.exit_code != Some(0) || r.signal.is_some() || r.error.is_some()))
        || r.error.as_ref().is_some_and(|s| s.len() > 8192)
    {
        return Err(Error::InvalidInput(
            "execution receipt identity or outcome mismatch".into(),
        ));
    }
    Ok(Some(r))
}
fn probe(e: &Execution) -> Option<ExecutionProbeReply> {
    let worker = e.worker.as_ref()?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, worker.port));
    let mut socket = TcpStream::connect_timeout(&address, Duration::from_millis(200)).ok()?;
    socket
        .set_read_timeout(Some(Duration::from_millis(250)))
        .ok()?;
    socket
        .set_write_timeout(Some(Duration::from_millis(150)))
        .ok()?;
    socket
        .write_all(
            &serde_json::to_vec(&ExecutionProbe {
                execution_id: e.id,
                nonce: worker.nonce,
            })
            .ok()?,
        )
        .ok()?;
    socket.shutdown(Shutdown::Write).ok()?;
    let mut bytes = Vec::new();
    socket.take(4096).read_to_end(&mut bytes).ok()?;
    let response: ExecutionProbeReply = serde_json::from_slice(&bytes).ok()?;
    if response.execution_id != e.id
        || response.nonce != worker.nonce
        || response.worker_pid != worker.pid
        || worker.child_pid.is_some_and(|id| id != response.child_pid)
        || response.child_pid == 0
        || response.observed_at < e.started_at?
    {
        return None;
    }
    Some(response)
}
pub fn inspect_execution(root: &Path, e: &Execution) -> Result<ExecutionObservation> {
    let root = root.canonicalize()?;
    let mut o = unknown(e, "supervisor_unreachable_without_completion_receipt")?;
    if Path::new(&e.intent.cwd) != root {
        o.basis = "project_root_mismatch".into();
        return Ok(o);
    }
    if e.intent.executor == ExecutorKind::External {
        o.basis = "external_reference_requires_executor_adapter".into();
        return Ok(o);
    }
    if e.state.terminal() {
        return Ok(outcome(
            o,
            ExecutionResult {
                execution_id: e.id,
                nonce: e
                    .worker
                    .as_ref()
                    .ok_or_else(|| {
                        Error::Storage(
                            "managed terminal execution lacks supervisor identity".into(),
                        )
                    })?
                    .nonce,
                finished_at: e
                    .finished_at
                    .ok_or_else(|| Error::Storage("terminal execution lacks finish time".into()))?,
                success: e.state == ExecutionState::Succeeded,
                exit_code: e.exit_code,
                signal: e.signal,
                error: e.error.clone(),
            },
            "immutable_supervisor_completion_event",
        ));
    }
    if e.worker.is_none() {
        o.basis = "registered_without_supervisor_identity".into();
        return Ok(o);
    }
    match receipt(&root, e) {
        Ok(Some(r)) => return Ok(outcome(o, r, "supervisor_result_receipt")),
        Err(_) => {
            o.basis = "invalid_or_unreadable_completion_receipt".into();
            return Ok(o);
        }
        Ok(None) => {}
    }
    if let Some(reply) = probe(e) {
        o.state = ObservedExecutionState::Running;
        o.verified = true;
        o.evidence_at = Some(reply.observed_at);
        o.basis = "owned_supervisor_identity_and_live_child_probe".into();
        o.observed_at = now_millis()?;
        o.next_action="The managed command is still running at the observation time. Wait for its result; do not launch it again.".into();
        return Ok(o);
    }
    // Child completion can race a failed probe. Re-read the atomic receipt before saying unknown.
    match receipt(&root, e) {
        Ok(Some(r)) => Ok(outcome(o, r, "supervisor_result_receipt_after_probe")),
        Err(_) => {
            o.basis = "invalid_or_unreadable_completion_receipt".into();
            Ok(o)
        }
        Ok(None) => Ok(o),
    }
}
pub fn inspect_work_executions(
    store: &Store,
    root: &Path,
    project: Id,
    work: Id,
    branch: Option<Id>,
) -> Result<Vec<ExecutionObservation>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    store
        .executions(project, Some(work))?
        .iter()
        .filter(|e| e.branch_id == branch)
        .map(|e| {
            if !e.state.terminal() && Instant::now() > deadline {
                unknown(
                    e,
                    "inspection_time_budget_exhausted_requires_explicit_inspect",
                )
            } else {
                inspect_execution(root, e)
            }
        })
        .collect()
}
pub fn render_execution_observations(observations: &[ExecutionObservation]) -> Result<String> {
    if observations.is_empty() {
        return Ok(String::new());
    }
    let mut text =
        "Execution recovery observations (separate from the source/context hash):\n".to_owned();
    for o in observations {
        text.push_str(&format!("{} {}: {} | verified={} | observed_at={} | evidence_at={:?} | basis={} | exit={:?} | signal={:?}\nPurpose: {}\nNext: {}\n",o.execution_id,serde_json::to_string(&o.operation_key)?,serde_json::to_string(&o.state)?,o.verified,o.observed_at,o.evidence_at,o.basis,o.exit_code,o.signal,serde_json::to_string(&o.purpose)?,o.next_action));
    }
    Ok(text)
}
