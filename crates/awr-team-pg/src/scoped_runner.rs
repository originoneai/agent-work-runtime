//! Operator-delegated adapter for the existing bounded file-write runner.
//! It consumes one fresh admission. Journals and reports never authorize reruns.
use crate::{
    OutboxDelivery, PgError, PgResult, ReferenceRunner, RunnerOutcome, WorkstreamCommand,
    WorkstreamCommandStore,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_PLAN: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceWrite {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceWritePlan {
    pub protocol_version: u32,
    pub writes: Vec<ReferenceWrite>,
}
impl ReferenceWritePlan {
    pub fn digest(&self) -> PgResult<String> {
        let mut paths = std::collections::BTreeSet::new();
        if self.protocol_version != 1
            || self.writes.is_empty()
            || self.writes.len() > 128
            || self
                .writes
                .iter()
                .any(|w| !portable_path(&w.path) || !paths.insert(&w.path))
        {
            return Err(invalid());
        }
        let value = json!(self);
        if serde_json::to_vec(&value).map_err(|_| invalid())?.len() > MAX_PLAN {
            return Err(invalid());
        }
        awr_team::request_hash(&value).map_err(|_| invalid())
    }
}
fn portable_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 4096
        && !p.chars().any(char::is_control)
        && !p.contains(['\\', ':'])
        && p.split('/').all(|s| !matches!(s, "" | "." | ".."))
}
fn invalid() -> PgError {
    PgError::Protocol("invalid reference runner request or journal".into())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceRunRequest {
    pub tenant_id: String,
    pub project_id: String,
    pub command: WorkstreamCommand,
    pub plan: ReferenceWritePlan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceReportRequest {
    pub tenant_id: String,
    pub project_id: String,
    pub command: WorkstreamCommand,
}

#[derive(Serialize, Deserialize)]
struct Admission {
    tenant_id: String,
    project_id: String,
    command: WorkstreamCommand,
    receipt: Value,
}

pub struct ScopedReferenceRunner {
    store: WorkstreamCommandStore,
    root: PathBuf,
}
impl ScopedReferenceRunner {
    pub fn new(store: WorkstreamCommandStore, root: impl Into<PathBuf>) -> Self {
        Self {
            store,
            root: root.into(),
        }
    }
    /// The operator controls this directory. Separate projects cannot share
    /// runner files merely because their external work IDs happen to match.
    pub fn project_root(&self, tenant: &str, project: &str) -> PgResult<PathBuf> {
        if [tenant, project]
            .iter()
            .any(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        {
            return Err(invalid());
        }
        let key = awr_team::request_hash(&json!([tenant, project])).map_err(|_| invalid())?;
        Ok(self.root.join("scoped-reference-v1").join(key))
    }

    pub async fn run(&self, bearer: &str, request: ReferenceRunRequest) -> PgResult<Value> {
        let input = request.plan.digest()?;
        let command = &request.command;
        if command.op != "execution.start"
            || command.args["execution_mode"] != "reference_write_v1"
            || command.args["expected_input_digest"] != input
        {
            return Err(invalid());
        }
        let root = self.project_root(&request.tenant_id, &request.project_id)?;
        // Subtract the entire admission round trip from the server's remaining
        // lease. Clock skew cannot extend the local effect budget.
        let before = Instant::now();
        let admitted = self
            .store
            .execute(
                &request.tenant_id,
                &request.project_id,
                bearer,
                command.clone(),
            )
            .await?;
        let receipt = &admitted["receipt"];
        text(&receipt["data"], "execution_id")?
            .parse::<ulid::Ulid>()
            .map_err(|_| invalid())?;
        let mut result = json!({"admission_receipt":receipt,"replayed":admitted["replayed"],
            "execution_authorized":false,"effects_attempted":false,"report_required":true});
        if admitted["execution_authorized"] != true || admitted["replayed"] != false {
            // A historical start receipt cannot determine current report state.
            result["report_required"] = Value::Null;
            let saved = root
                .join("reports")
                .join(format!("{}.json", text(&receipt["data"], "execution_id")?));
            if saved.is_file() {
                result["report_request_file"] = json!(saved);
            }
            result["next_action"] = json!(
                "Inspect the original execution and saved journal; replay never runs effects or extends the lease."
            );
            return Ok(result);
        }
        // After admission, return the committed receipt even if local work fails.
        // Losing local persistence is an unknown result, never permission to retry.
        let data = &receipt["data"];
        let local = Admission {
            tenant_id: request.tenant_id.clone(),
            project_id: request.project_id.clone(),
            command: command.clone(),
            receipt: receipt.clone(),
        };
        let execution = text(data, "execution_id")?;
        let record = root.join("admissions").join(format!("{execution}.json"));
        if persist_new(&record, &json!(local)).is_err() {
            result["next_action"] = json!(
                "Admission committed but local journal could not be created; inspect and reconcile. Do not run again."
            );
            return Ok(result);
        }
        let remaining = text(data, "lease_remaining_ms")?
            .parse::<u64>()
            .map_err(|_| invalid())?;
        let deadline = before
            .checked_add(Duration::from_millis(remaining))
            .ok_or_else(invalid)?;
        let delivery = OutboxDelivery {
            outbox_id: String::new(),
            execution_id: execution.into(),
            effect_key: text(data, "effect_key")?.into(),
            coordinator_epoch: command.coordinator_epoch.clone(),
            tenant_id: request.tenant_id.clone(),
            project_id: request.project_id.clone(),
            scope_id: "main".into(),
            work_id: command.work_id.clone(),
            fence: text(data, "fence")?.parse().map_err(|_| invalid())?,
            fencing_class: "uncontrolled".into(),
            declared_scope: data["declared_scope"].clone(),
            payload: json!({"writes":request.plan.writes}),
            delivery_attempts: 0,
        };
        let runner = ReferenceRunner::new(&root);
        let outcome = runner.handle_scoped_delivery(&delivery, deadline);
        result["effects_attempted"] = json!(outcome.started);
        result["outcome"] = json!(outcome);
        if persist_new(
            &root.join("observations").join(format!("{execution}.json")),
            &json!(outcome),
        )
        .is_err()
        {
            result["next_action"] = json!(
                "Execution outcome could not be saved durably; inspect and reconcile. Never rerun effects."
            );
            return Ok(result);
        }
        let report = report_request(&local, &outcome)?;
        // Persist the exact command BEFORE sending it, so a lost DB response can
        // be retried under the same idempotency key without re-running the effect.
        let report_path = root.join("reports").join(format!("{execution}.json"));
        if persist_new(&report_path, &json!(report)).is_err() {
            result["next_action"] = json!(
                "Execution journal saved but report request could not be saved; inspect and reconcile without rerunning."
            );
            return Ok(result);
        }
        result["report_request_file"] = json!(report_path);
        match self
            .store
            .execute(
                &report.tenant_id,
                &report.project_id,
                bearer,
                report.command,
            )
            .await
        {
            Ok(value) => {
                result["report"] = value;
                result["report_required"] = json!(false);
                result["next_action"] = json!(if outcome.unknown
                    || !outcome.partial_paths.is_empty()
                    || outcome.scope_violation
                {
                    "Operator reconciliation required; work acceptance remains separate."
                } else {
                    "Execution result recorded; work acceptance remains separate."
                });
            }
            Err(_) => {
                result["next_action"] = json!(
                    "Report not confirmed. Inspect its request ID, then retry the saved report exactly. Refresh conflicting preconditions only after establishing that it did not commit. Never rerun effects."
                );
            }
        }
        Ok(result)
    }

    /// Retry a saved report, or a reviewed revision replacement. Facts are always
    /// reconstructed from the local runner journal; this never calls the runner.
    pub async fn report(&self, bearer: &str, request: ReferenceReportRequest) -> PgResult<Value> {
        let command = &request.command;
        if command.op != "execution.attest" {
            return Err(invalid());
        }
        let execution = text(&command.args, "execution_id")?;
        // IDs returned by admission are ULIDs, never paths supplied by an agent.
        execution.parse::<ulid::Ulid>().map_err(|_| invalid())?;
        let root = self.project_root(&request.tenant_id, &request.project_id)?;
        let local: Admission = serde_json::from_value(read_json(
            &root.join("admissions").join(format!("{execution}.json")),
        )?)
        .map_err(|_| invalid())?;
        if local.tenant_id != request.tenant_id
            || local.project_id != request.project_id
            || local.command.work_id != command.work_id
            || local.command.workstream_id != command.workstream_id
            || local.command.coordinator_epoch != command.coordinator_epoch
            || local.command.expected_ownership_version != command.expected_ownership_version
            || local.command.args["session_id"] != command.args["session_id"]
        {
            return Err(invalid());
        }
        let outcome: RunnerOutcome = serde_json::from_value(read_json(
            &root.join("observations").join(format!("{execution}.json")),
        )?)
        .map_err(|_| invalid())?;
        let expected = report_request(&local, &outcome)?;
        if command.args["facts"] != expected.command.args["facts"] {
            return Err(invalid());
        }
        self.store
            .execute(
                &request.tenant_id,
                &request.project_id,
                bearer,
                request.command,
            )
            .await
    }
}

fn report_request(local: &Admission, outcome: &RunnerOutcome) -> PgResult<ReferenceReportRequest> {
    if local.receipt["data"]["execution_id"] != outcome.execution_id
        || local.receipt["data"]["effect_key"] != outcome.effect_key
    {
        return Err(invalid());
    }
    let uncertain = outcome.unknown
        || !outcome.partial_paths.is_empty()
        || outcome.scope_violation
        || !matches!(outcome.state.as_str(), "succeeded" | "failed");
    let mut paths = outcome.observed_paths.clone();
    paths.extend(outcome.partial_paths.clone());
    paths.sort();
    paths.dedup();
    let mut command = local.command.clone();
    command.op = "execution.attest".into();
    command.request_id =
        awr_team::request_hash(&json!(["reference-report-v1", outcome.execution_id]))
            .map_err(|_| invalid())?;
    command.expected_project_revision = text(&local.receipt, "committed_project_revision")?.into();
    command.args = json!({"session_id":local.command.args["session_id"],
        "expected_session_version":local.command.args["expected_session_version"],
        "execution_id":outcome.execution_id,"expected_execution_version":local.receipt["data"]["execution_version"],
        "facts":{"outcome":if uncertain {"unknown"} else {&outcome.state},
            "input_digest":local.receipt["data"]["input_digest"],"output_digest":outcome.output_digest,
            "environment_digest":outcome.environment_digest,"observed_paths":paths,
            "note":if uncertain {"Reference runner recorded an uncertain or scope-violating effect; operator review required."}
                else {"Reference runner observed bounded file writes; not work acceptance."}}});
    Ok(ReferenceReportRequest {
        tenant_id: local.tenant_id.clone(),
        project_id: local.project_id.clone(),
        command,
    })
}
fn text<'a>(value: &'a Value, field: &str) -> PgResult<&'a str> {
    value[field].as_str().ok_or_else(invalid)
}
fn persist_new(path: &Path, value: &Value) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("journal parent"))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    #[cfg(unix)]
    std::fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn read_json(path: &Path) -> PgResult<Value> {
    let file = std::fs::File::open(path).map_err(|_| invalid())?;
    let mut bytes = vec![];
    file.take(262145)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > 262144 {
        return Err(invalid());
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_digest_binds_order_paths_and_content_and_rejects_ambiguous_plans() {
        let plan = ReferenceWritePlan {
            protocol_version: 1,
            writes: vec![ReferenceWrite {
                path: "src/result".into(),
                content: "one".into(),
            }],
        };
        let digest = plan.digest().unwrap();
        assert_eq!(
            digest,
            serde_json::from_value::<ReferenceWritePlan>(json!(plan))
                .unwrap()
                .digest()
                .unwrap()
        );
        let mut changed = plan.clone();
        changed.writes[0].content = "two".into();
        assert_ne!(digest, changed.digest().unwrap());
        for p in [
            "../src",
            "src//file",
            "src/./file",
            "/absolute",
            "C:drive",
            "a\\b",
        ] {
            changed.writes[0].path = p.into();
            assert!(changed.digest().is_err());
        }
        changed = plan.clone();
        changed.writes.push(changed.writes[0].clone());
        assert!(changed.digest().is_err());
        changed = plan.clone();
        changed.writes[0].content = "x".repeat(MAX_PLAN);
        assert!(changed.digest().is_err());
        changed.writes.clear();
        assert!(changed.digest().is_err());
        let mut extra = json!(plan);
        extra["shell"] = json!("echo surprise");
        assert!(serde_json::from_value::<ReferenceWritePlan>(extra).is_err());
    }

    #[test]
    fn elapsed_admission_deadline_never_writes_and_retains_unknown_outcome() {
        let root = std::env::temp_dir().join(format!("awr-scoped-deadline-{}", ulid::Ulid::new()));
        let runner = ReferenceRunner::new(&root);
        let d = OutboxDelivery {
            outbox_id: String::new(),
            execution_id: ulid::Ulid::new().to_string(),
            effect_key: "effect".into(),
            tenant_id: "tenant".into(),
            project_id: "project".into(),
            scope_id: "main".into(),
            work_id: "work".into(),
            coordinator_epoch: "epoch".into(),
            fence: 1,
            fencing_class: "uncontrolled".into(),
            declared_scope: json!(["src"]),
            payload: json!({"writes":[{"path":"src/result","content":"never"}]}),
            delivery_attempts: 0,
        };
        let r = runner.handle_scoped_delivery(&d, Instant::now());
        assert!(r.unknown);
        assert!(!r.started);
        assert!(!root.join("worktree/src/result").exists());
        let duplicate = runner.handle_scoped_delivery(&d, Instant::now() + Duration::from_secs(60));
        assert!(duplicate.unknown);
        assert!(!root.join("worktree/src/result").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
