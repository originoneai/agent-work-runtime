use crate::error::{PgError, PgResult};
use crate::graph::path_within_scope;
use crate::tx::{bind_scope, de_i64_flex, emit_event, new_id, ser_i64_string};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub id: String,
    pub work_id: String,
    pub session_id: String,
    pub claim_id: String,
    #[serde(serialize_with = "ser_i64_string", deserialize_with = "de_i64_flex")]
    pub fence: i64,
    pub contract_hash: String,
    pub effect_key: String,
    pub state: String,
    pub cancel_requested: bool,
    pub fencing_class: String,
    pub replayed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutboxDelivery {
    /// Issuer generation; legacy deliveries without one fail closed at the resource.
    #[serde(default)]
    pub coordinator_epoch: String,
    pub outbox_id: String,
    pub execution_id: String,
    pub effect_key: String,
    #[serde(serialize_with = "ser_i64_string", deserialize_with = "de_i64_flex")]
    pub fence: i64,
    /// Full identity of the token issuer: fences increment per
    /// (tenant, project, scope, work) and must never be compared across
    /// them (CR #58 r3/r4).
    pub tenant_id: String,
    pub project_id: String,
    pub work_id: String,
    pub scope_id: String,
    pub fencing_class: String,
    pub declared_scope: Value,
    pub payload: Value,
    pub delivery_attempts: i32,
}

pub struct ExecutionStore {
    pool: crate::PgPool,
}

impl ExecutionStore {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            pool: crate::PgPool::new(url),
        }
    }

    /// Build from a validated `tokio_postgres::Config` (see PgPool::from_config).
    pub fn from_config(config: tokio_postgres::Config) -> Self {
        Self {
            pool: crate::PgPool::from_config(config),
        }
    }

    async fn connect(&self) -> PgResult<crate::PgClient> {
        self.pool.get().await
    }

    pub async fn prepare(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        client_id: &str,
        request_id: &str,
        claim_id: &str,
        executor_actor_id: &str,
        contract_hash: &str,
        input_digest: &str,
        fencing_class: &str,
        declared_scope: &[String],
        writes: &Value,
    ) -> PgResult<ExecutionRecord> {
        validate_fencing_class(fencing_class)?;
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        // The idempotency identity must cover everything that changes the
        // execution plan; omitting input_digest/declared_scope/writes let a
        // rewritten payload replay the old receipt (CR #41 P2-5). Receipts
        // written with the pre-fix incomplete hash never match and fail
        // closed as IdempotencyConflict.
        let request_hash = awr_team::request_hash(&json!({
            "op": "execution.prepare",
            "request_id": request_id,
            "args": {
                "claim_id": claim_id,
                "executor_actor_id": executor_actor_id,
                "contract_hash": contract_hash,
                "fencing_class": fencing_class,
                "input_digest": input_digest,
                "declared_scope": declared_scope,
                "writes": writes,
            },
        }))
        .map_err(|e| PgError::Protocol(e.to_string()))?;
        if let Some((existing_hash, result)) =
            load_operation(&tx, tenant_id, project_id, actor_id, client_id, request_id).await?
        {
            if existing_hash != request_hash {
                return Err(PgError::IdempotencyConflict);
            }
            return replay_execution(&result);
        }
        let claim = tx
            .query_opt(
                "SELECT session_id, work_id, scope_id, actor_id, fence, state
                 FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3
                   AND state='active' AND expires_at > clock_timestamp()
                 FOR UPDATE",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?
            .ok_or(PgError::LeaseExpired)?;
        let session_id: String = claim.get(0);
        let work_id: String = claim.get(1);
        let scope_id: String = claim.get(2);
        let holder: String = claim.get(3);
        let fence: i64 = claim.get(4);
        if holder != actor_id {
            return Err(PgError::Forbidden);
        }
        let blocked: bool = tx
            .query_opt(
                "SELECT recovery_blocked FROM awr_team.work_runtime
                 WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4",
                &[&tenant_id, &project_id, &scope_id, &work_id],
            )
            .await?
            .map(|row| row.get(0))
            .unwrap_or(false);
        if blocked {
            return Err(PgError::RecoveryBlocked);
        }
        let unknown: i64 = tx
            .query_one(
                "SELECT count(*) FROM awr_team.executions
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='unknown'",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?
            .get(0);
        if unknown > 0 {
            return Err(PgError::RecoveryBlocked);
        }
        let execution_id = new_id();
        let effect_key = execution_id.clone();
        let scope_json = json!(declared_scope);
        tx.execute(
            "INSERT INTO awr_team.executions(
                tenant_id, project_id, id, work_id, session_id, claim_id, fence,
                contract_hash, input_digest, executor_actor_id, state, effect_key,
                fencing_class, declared_scope_json, scope_id, coordinator_epoch)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'prepared',$11,$12,$13,$14,(SELECT coordinator_epoch FROM awr_team.projects WHERE tenant_id=$1 AND id=$2))",
            &[
                &tenant_id,
                &project_id,
                &execution_id,
                &work_id,
                &session_id,
                &claim_id,
                &fence,
                &contract_hash,
                &input_digest,
                &executor_actor_id,
                &effect_key,
                &fencing_class,
                &scope_json,
                &scope_id,
            ],
        )
        .await?;
        let payload = json!({
            "execution_id": execution_id,
            "effect_key": effect_key,
            "tenant_id": tenant_id,
            "project_id": project_id,
            "work_id": work_id,
            "scope_id": scope_id,
            "session_id": session_id,
            "claim_id": claim_id,
            "fence": fence.to_string(),
            "contract_hash": contract_hash,
            "input_digest": input_digest,
            "executor_actor_id": executor_actor_id,
            "fencing_class": fencing_class,
            "declared_scope": declared_scope,
            "writes": writes,
        });
        let outbox_id = new_id();
        tx.execute(
            "INSERT INTO awr_team.outbox(
                tenant_id, project_id, id, state, payload_json, action_kind, aggregate_id)
             VALUES ($1,$2,$3,'pending',$4,'execution.dispatch',$5)",
            &[&tenant_id, &project_id, &outbox_id, &payload, &execution_id],
        )
        .await?;
        let result = json!({
            "id": execution_id,
            "work_id": work_id,
            "session_id": session_id,
            "claim_id": claim_id,
            "fence": fence.to_string(),
            "contract_hash": contract_hash,
            "effect_key": effect_key,
            "state": "prepared",
            "cancel_requested": false,
            "fencing_class": fencing_class,
        });
        let committed_revision = emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            &work_id,
            "execution.prepared",
            json!({"execution_id": execution_id, "fence": fence.to_string()}),
        )
        .await?;
        store_operation(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            client_id,
            request_id,
            "execution.prepare",
            &request_hash,
            committed_revision,
            &result,
        )
        .await?;
        tx.commit().await?;
        replay_execution(&result).map(|mut record| {
            record.replayed = false;
            record
        })
    }

    pub async fn claim_dispatch(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> PgResult<Option<OutboxDelivery>> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "UPDATE awr_team.outbox SET
                    state='sending',
                    delivery_attempts = delivery_attempts + 1,
                    delivery_token = $3
                 WHERE id = (
                    SELECT o.id FROM awr_team.outbox o
                    JOIN awr_team.executions e
                      ON e.tenant_id=o.tenant_id AND e.project_id=o.project_id
                     AND e.id=o.aggregate_id
                    WHERE o.tenant_id=$1 AND o.project_id=$2
                      AND o.action_kind='execution.dispatch'
                      AND o.state IN ('pending','sending')
                      AND o.available_at <= clock_timestamp()
                      AND e.state IN ('prepared','queued','accepted','running')
                    ORDER BY o.available_at, o.id
                    FOR UPDATE OF o SKIP LOCKED
                    LIMIT 1
                 )
                 RETURNING id, aggregate_id, payload_json, delivery_attempts, tenant_id, project_id",
                &[&tenant_id, &project_id, &new_id()],
            )
            .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let outbox_id: String = row.get(0);
        let execution_id: String = row.get(1);
        let payload: Value = row.get(2);
        let attempts: i32 = row.get(3);
        // Identity comes from the outbox ROW plus the linked EXECUTION row,
        // never from the payload: legacy messages lack these fields, and
        // missing scope must NOT default to main (CR #58 r6 P2-1).
        let tenant: String = row.get(4);
        let project: String = row.get(5);
        let identity = tx
            .query_opt(
                "SELECT scope_id, work_id, coordinator_epoch FROM awr_team.executions
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant, &project, &execution_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("dispatch without an execution row".into()))?;
        let scope_id: String = identity.get(0);
        let work_id: String = identity.get(1);
        let coordinator_epoch: String = identity
            .get::<_, Option<String>>(2)
            .filter(|e| !e.is_empty())
            .ok_or(PgError::EpochChanged)?;
        tx.execute(
            "UPDATE awr_team.executions SET state='queued'
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3 AND state='prepared'",
            &[&tenant_id, &project_id, &execution_id],
        )
        .await?;
        tx.commit().await?;
        Ok(Some(OutboxDelivery {
            coordinator_epoch,
            outbox_id,
            execution_id: execution_id.clone(),
            effect_key: payload
                .get("effect_key")
                .and_then(Value::as_str)
                .unwrap_or(&execution_id)
                .to_owned(),
            fence: match payload.get("fence") {
                // Decimal-string (current) and legacy numeric forms; a
                // missing or invalid fence never silently becomes 0
                // (CR #58 P2-8).
                Some(Value::String(s)) => s
                    .parse()
                    .map_err(|_| PgError::Protocol("outbox payload has invalid fence".into()))?,
                Some(Value::Number(n)) => n
                    .as_i64()
                    .ok_or_else(|| PgError::Protocol("outbox payload has invalid fence".into()))?,
                _ => return Err(PgError::Protocol("outbox payload missing fence".into())),
            },
            tenant_id: tenant,
            project_id: project,
            work_id,
            scope_id,
            fencing_class: payload
                .get("fencing_class")
                .and_then(Value::as_str)
                .unwrap_or("uncontrolled")
                .to_owned(),
            declared_scope: payload
                .get("declared_scope")
                .cloned()
                .unwrap_or_else(|| json!([])),
            payload,
            delivery_attempts: attempts,
        }))
    }

    pub async fn ack_dispatch(
        &self,
        tenant_id: &str,
        project_id: &str,
        outbox_id: &str,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        tx.execute(
            "UPDATE awr_team.outbox SET state='delivered'
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3 AND state='sending'",
            &[&tenant_id, &project_id, &outbox_id],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn accept(
        &self,
        tenant_id: &str,
        project_id: &str,
        execution_id: &str,
        fence: i64,
    ) -> PgResult<ExecutionRecord> {
        self.transition(
            tenant_id,
            project_id,
            execution_id,
            fence,
            &["prepared", "queued", "accepted"],
            "accepted",
        )
        .await
    }

    pub async fn start(
        &self,
        tenant_id: &str,
        project_id: &str,
        execution_id: &str,
        fence: i64,
    ) -> PgResult<ExecutionRecord> {
        self.transition(
            tenant_id,
            project_id,
            execution_id,
            fence,
            &["accepted", "running"],
            "running",
        )
        .await
    }

    pub async fn cancel(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        client_id: &str,
        request_id: &str,
        execution_id: &str,
    ) -> PgResult<ExecutionRecord> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let request_hash = format!("execution.cancel:{execution_id}");
        if let Some((existing_hash, result)) =
            load_operation(&tx, tenant_id, project_id, actor_id, client_id, request_id).await?
        {
            if existing_hash != request_hash {
                return Err(PgError::IdempotencyConflict);
            }
            return replay_execution(&result);
        }
        let row = tx
            .query_opt(
                "SELECT state, fence, work_id, session_id, claim_id, contract_hash,
                        effect_key, fencing_class, cancel_requested
                 FROM awr_team.executions
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3
                 FOR UPDATE",
                &[&tenant_id, &project_id, &execution_id],
            )
            .await?
            .ok_or(PgError::ExecutionNotFound)?;
        let state: String = row.get(0);
        let fence: i64 = row.get(1);
        let mut cancel_requested: bool = row.get(8);
        let next_state = if matches!(state.as_str(), "prepared" | "queued") {
            "cancelled"
        } else {
            cancel_requested = true;
            state.as_str()
        };
        tx.execute(
            "UPDATE awr_team.executions
             SET cancel_requested=$4, state=$5
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[
                &tenant_id,
                &project_id,
                &execution_id,
                &cancel_requested,
                &next_state,
            ],
        )
        .await?;
        // A cancelled execution must not be dispatched again (CR #41 P2-6).
        // The outbox state enum has no 'cancelled'; 'failed' is its terminal
        // non-redeliverable state.
        tx.execute(
            "UPDATE awr_team.outbox SET state='failed'
             WHERE tenant_id=$1 AND project_id=$2 AND aggregate_id=$3
               AND state IN ('pending','sending')",
            &[&tenant_id, &project_id, &execution_id],
        )
        .await?;
        let result = json!({
            "id": execution_id,
            "work_id": row.get::<_, String>(2),
            "session_id": row.get::<_, Option<String>>(3).unwrap_or_default(),
            "claim_id": row.get::<_, Option<String>>(4).unwrap_or_default(),
            "fence": fence.to_string(),
            "contract_hash": row.get::<_, String>(5),
            "effect_key": row.get::<_, Option<String>>(6).unwrap_or_default(),
            "state": next_state,
            "cancel_requested": cancel_requested,
            "fencing_class": row.get::<_, String>(7),
        });
        let cancel_event = if next_state == "cancelled" {
            "execution.cancelled"
        } else {
            "execution.cancel_requested"
        };
        let committed_revision = emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            &row.get::<_, String>(2),
            cancel_event,
            json!({"execution_id": execution_id}),
        )
        .await?;
        store_operation(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            client_id,
            request_id,
            "execution.cancel",
            &request_hash,
            committed_revision,
            &result,
        )
        .await?;
        tx.commit().await?;
        replay_execution(&result).map(|mut record| {
            record.replayed = false;
            record
        })
    }

    pub async fn report(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        receipt_kind: &str,
        execution_id: &str,
        outcome: &str,
        payload: Value,
        observed_paths: &[String],
    ) -> PgResult<ExecutionRecord> {
        if !matches!(
            receipt_kind,
            "caller_asserted" | "trusted_executor" | "reconcile"
        ) {
            return Err(PgError::Protocol("invalid receipt kind".into()));
        }
        if !matches!(outcome, "succeeded" | "failed" | "unknown" | "cancelled") {
            return Err(PgError::Protocol("invalid outcome".into()));
        }
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let actor_kind: String = tx
            .query_opt(
                "SELECT kind FROM awr_team.actors WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &actor_id],
            )
            .await?
            .map(|row| row.get(0))
            .ok_or(PgError::Forbidden)?;
        let row = tx
            .query_opt(
                "SELECT state, fence, work_id, session_id, claim_id, contract_hash,
                        effect_key, fencing_class, cancel_requested, declared_scope_json,
                        scope_id, executor_actor_id
                 FROM awr_team.executions
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3
                 FOR UPDATE",
                &[&tenant_id, &project_id, &execution_id],
            )
            .await?
            .ok_or(PgError::ExecutionNotFound)?;
        // Receipt authority is uniform and bound to the DELEGATED executor:
        // trusted_executor requires the system actor actually assigned to
        // this execution; reconcile receipts require a system coordinator —
        // the kind string in the request grants nothing by itself (CR #41
        // P2-7).
        let delegated_executor: String = row.get(11);
        if receipt_kind == "trusted_executor"
            && (actor_kind != "system" || actor_id != delegated_executor)
        {
            return Err(PgError::Forbidden);
        }
        if receipt_kind == "reconcile" && actor_kind != "system" {
            return Err(PgError::Forbidden);
        }
        let current: String = row.get(0);
        let _fence: i64 = row.get(1);
        let work_id: String = row.get(2);
        let scope_id: String = row.get(10);
        let declared: Value = row.get(9);
        let cancel_requested: bool = row.get(8);
        let fencing_class: String = row.get(7);
        let _exactly_once = exactly_once_supported(&fencing_class);
        if matches!(current.as_str(), "succeeded" | "failed" | "cancelled")
            && outcome == "cancelled"
        {
            insert_receipt(
                &tx,
                tenant_id,
                project_id,
                execution_id,
                actor_id,
                receipt_kind,
                &payload,
            )
            .await?;
            if !cancel_requested {
                tx.execute(
                    "UPDATE awr_team.executions SET cancel_requested=TRUE
                     WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                    &[&tenant_id, &project_id, &execution_id],
                )
                .await?;
                // The flag change is a state change: it must be observable
                // (CR #58 P2-7).
                emit_event(
                    &tx,
                    tenant_id,
                    project_id,
                    actor_id,
                    &work_id,
                    "execution.cancel_requested",
                    json!({"execution_id": execution_id}),
                )
                .await?;
            }
            tx.commit().await?;
            return self.get(tenant_id, project_id, execution_id).await;
        }
        // Explicit state machine for ordinary reports (CR #41 P2-8):
        // - terminal + same outcome        → idempotent audit receipt
        // - terminal + different outcome   → audit receipt kept, facts stand
        // - running + cancelled            → a real stop confirmation lands
        // - cancel requested then success  → success still stands (unchanged)
        let terminal = matches!(current.as_str(), "succeeded" | "failed" | "cancelled");
        if terminal && outcome == current {
            // Same outcome is NOT automatically the same result: replay only
            // when the key facts match; otherwise keep an audit receipt and
            // leave the terminal state untouched (CR #58 P2-5). Authorized
            // corrections go through reconcile().
            let stored_digest: Option<String> = tx
                .query_one(
                    "SELECT result_digest FROM awr_team.executions WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                    &[&tenant_id, &project_id, &execution_id],
                )
                .await?
                .get(0);
            // observed_paths_json is NULLABLE (e.g. a terminal state set via
            // reconcile never wrote it); read it safely instead of panicking
            // (CR #58 r3 P2-5).
            let stored_paths: Option<Value> = tx
                .query_one(
                    "SELECT observed_paths_json FROM awr_team.executions WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                    &[&tenant_id, &project_id, &execution_id],
                )
                .await?
                .get(0);
            let incoming_digest = payload.get("output_digest").and_then(Value::as_str);
            // A SQL NULL is "unknown facts", NOT "confirmed no effects";
            // unknown facts can never prove the replay identical.
            let facts_match = match &stored_paths {
                Some(stored) => {
                    stored_digest.as_deref() == incoming_digest && *stored == json!(observed_paths)
                }
                None => false,
            };
            if facts_match {
                tx.commit().await?;
                return self.get(tenant_id, project_id, execution_id).await;
            }
            insert_receipt(
                &tx,
                tenant_id,
                project_id,
                execution_id,
                actor_id,
                receipt_kind,
                &json!({"late_diverging_report": outcome, "payload": payload}),
            )
            .await?;
            tx.commit().await?;
            return Err(PgError::Protocol(format!(
                "diverging late report for terminal execution in state {current}"
            )));
        }
        if terminal && outcome != current {
            insert_receipt(
                &tx,
                tenant_id,
                project_id,
                execution_id,
                actor_id,
                receipt_kind,
                &json!({"late_conflicting_report": outcome, "payload": payload}),
            )
            .await?;
            tx.commit().await?;
            return Err(PgError::Protocol(format!(
                "conflicting late report {outcome} for terminal execution in state {current}"
            )));
        }
        let mut next = outcome.to_owned();
        if outcome == "cancelled" && !matches!(current.as_str(), "prepared" | "queued") {
            next = current.clone();
        }
        if outcome == "cancelled" && current == "running" {
            next = "cancelled".into();
        }
        if outcome == "succeeded" && scope_exceeded(&declared, observed_paths) {
            insert_receipt(
                &tx,
                tenant_id,
                project_id,
                execution_id,
                actor_id,
                receipt_kind,
                &json!({"scope_violation": true, "observed_paths": observed_paths, "payload": payload}),
            )
            .await?;
            tx.execute(
                "UPDATE awr_team.executions
                 SET state='failed', observed_paths_json=$4
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[
                    &tenant_id,
                    &project_id,
                    &execution_id,
                    &json!(observed_paths),
                ],
            )
            .await?;
            // Early exits that still change state must emit the lifecycle
            // event in the same transaction (CR #58 P2-7).
            emit_event(
                &tx,
                tenant_id,
                project_id,
                actor_id,
                &work_id,
                "execution.reported",
                json!({"execution_id": execution_id, "outcome": "failed", "receipt_kind": receipt_kind, "scope_violation": true}),
            )
            .await?;
            tx.commit().await?;
            return Err(PgError::ScopeExceeded);
        }
        insert_receipt(
            &tx,
            tenant_id,
            project_id,
            execution_id,
            actor_id,
            receipt_kind,
            &payload,
        )
        .await?;
        let unknown_reason = payload
            .get("unknown_reason")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let environment_digest = payload
            .get("environment_digest")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let result_digest = payload
            .get("output_digest")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let cancel_flag = cancel_requested || outcome == "cancelled";
        tx.execute(
            "UPDATE awr_team.executions
             SET state=$4,
                 cancel_requested=$5,
                 observed_paths_json=$6,
                 environment_digest=$7,
                 unknown_reason=$8,
                 result_digest=$9
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[
                &tenant_id,
                &project_id,
                &execution_id,
                &next,
                &cancel_flag,
                &json!(observed_paths),
                &environment_digest,
                &unknown_reason,
                &result_digest,
            ],
        )
        .await?;
        if next == "unknown" {
            tx.execute(
                "UPDATE awr_team.work_runtime SET recovery_blocked=TRUE
                 WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4",
                &[&tenant_id, &project_id, &scope_id, &work_id],
            )
            .await?;
            tx.execute(
                "UPDATE awr_team.resource_reservations SET state='unknown'
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='reserved'",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?;
        }
        emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            &work_id,
            "execution.reported",
            json!({"execution_id": execution_id, "outcome": next, "receipt_kind": receipt_kind}),
        )
        .await?;
        tx.commit().await?;
        let _ = _exactly_once;
        self.get(tenant_id, project_id, execution_id).await
    }

    pub async fn reconcile(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        execution_id: &str,
        terminal_state: &str,
        payload: Value,
        clear_block: bool,
    ) -> PgResult<ExecutionRecord> {
        if !matches!(
            terminal_state,
            "succeeded" | "failed" | "cancelled" | "unknown"
        ) {
            return Err(PgError::Protocol("invalid reconcile state".into()));
        }
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let actor_kind: String = tx
            .query_opt(
                "SELECT kind FROM awr_team.actors WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &actor_id],
            )
            .await?
            .map(|row| row.get(0))
            .ok_or(PgError::Forbidden)?;
        if actor_kind != "system" {
            return Err(PgError::Forbidden);
        }
        let row = tx
            .query_opt(
                "SELECT work_id, scope_id, state FROM awr_team.executions
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3
                 FOR UPDATE",
                &[&tenant_id, &project_id, &execution_id],
            )
            .await?
            .ok_or(PgError::ExecutionNotFound)?;
        let work_id: String = row.get(0);
        let scope_id: String = row.get(1);
        insert_receipt(
            &tx,
            tenant_id,
            project_id,
            execution_id,
            actor_id,
            "reconcile",
            &payload,
        )
        .await?;
        tx.execute(
            "UPDATE awr_team.executions SET state=$4, unknown_reason=NULL
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &execution_id, &terminal_state],
        )
        .await?;
        if clear_block && terminal_state != "unknown" {
            // A single reconcile must not lift the protection of OTHER
            // unresolved executions on the same work (CR #41 P2-9). Stay
            // conservatively blocked until every unknown execution settles.
            let remaining_unknown: i64 = tx
                .query_one(
                    "SELECT count(*) FROM awr_team.executions
                     WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3
                       AND state='unknown' AND id<>$4",
                    &[&tenant_id, &project_id, &work_id, &execution_id],
                )
                .await?
                .get(0);
            if remaining_unknown > 0 {
                return Err(PgError::Protocol(format!(
                    "{remaining_unknown} unknown execution(s) remain on this work; recovery block kept"
                )));
            }
            tx.execute(
                "UPDATE awr_team.work_runtime SET recovery_blocked=FALSE
                 WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4",
                &[&tenant_id, &project_id, &scope_id, &work_id],
            )
            .await?;
            tx.execute(
                "UPDATE awr_team.resource_reservations SET state='released'
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='unknown'",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?;
        }
        emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            &work_id,
            "execution.reconciled",
            json!({"execution_id": execution_id, "terminal_state": terminal_state, "clear_block": clear_block}),
        )
        .await?;
        tx.commit().await?;
        self.get(tenant_id, project_id, execution_id).await
    }

    pub async fn get(
        &self,
        tenant_id: &str,
        project_id: &str,
        execution_id: &str,
    ) -> PgResult<ExecutionRecord> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT id, work_id, session_id, claim_id, fence, contract_hash, effect_key,
                        state, cancel_requested, fencing_class
                 FROM awr_team.executions
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &execution_id],
            )
            .await?
            .ok_or(PgError::ExecutionNotFound)?;
        tx.commit().await?;
        Ok(ExecutionRecord {
            id: row.get(0),
            work_id: row.get(1),
            session_id: row.get::<_, Option<String>>(2).unwrap_or_default(),
            claim_id: row.get::<_, Option<String>>(3).unwrap_or_default(),
            fence: row.get(4),
            contract_hash: row.get(5),
            effect_key: row.get::<_, Option<String>>(6).unwrap_or_default(),
            state: row.get(7),
            cancel_requested: row.get(8),
            fencing_class: row.get(9),
            replayed: false,
        })
    }

    async fn transition(
        &self,
        tenant_id: &str,
        project_id: &str,
        execution_id: &str,
        fence: i64,
        allowed: &[&str],
        next: &str,
    ) -> PgResult<ExecutionRecord> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT e.state, e.fence, e.claim_id, e.scope_id, e.work_id,
                        w.last_fence,
                        c.state AS claim_state, c.expires_at, w.recovery_blocked, e.coordinator_epoch,
                        (SELECT coordinator_epoch FROM awr_team.projects WHERE tenant_id=e.tenant_id AND id=e.project_id)
                 FROM awr_team.executions e
                 JOIN awr_team.work_runtime w
                   ON w.tenant_id=e.tenant_id AND w.project_id=e.project_id
                  AND w.scope_id=e.scope_id AND w.work_id=e.work_id
                 LEFT JOIN awr_team.claims c
                   ON c.tenant_id=e.tenant_id AND c.project_id=e.project_id
                  AND c.id=e.claim_id
                 WHERE e.tenant_id=$1 AND e.project_id=$2 AND e.id=$3
                 FOR UPDATE OF e",
                &[&tenant_id, &project_id, &execution_id],
            )
            .await?
            .ok_or(PgError::ExecutionNotFound)?;
        if row.get::<_, bool>(8) {
            return Err(PgError::RecoveryBlocked);
        }
        if row.get::<_, Option<String>>(9).as_deref() != Some(row.get::<_, String>(10).as_str()) {
            return Err(PgError::EpochChanged);
        }
        let state: String = row.get(0);
        let current_fence: i64 = row.get(1);
        // Admission must bind the CURRENT fence and a still-valid claim:
        // an execution prepared before a handoff keeps its old fence value
        // and must not start with it (CR #41 P1-2).
        let live_fence: i64 = row.get(5);
        let claim_state: Option<String> = row.get(6);
        if current_fence != fence || live_fence != fence {
            return Err(PgError::StaleFence);
        }
        let claim_valid: bool = tx
            .query_one(
                "SELECT count(*) FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3
                   AND state='active' AND expires_at > clock_timestamp()",
                &[&tenant_id, &project_id, &row.get::<_, Option<String>>(2)],
            )
            .await?
            .get::<_, i64>(0)
            > 0;
        let _ = claim_state;
        if !claim_valid {
            return Err(PgError::StaleFence);
        }
        // Idempotent same-state calls return BEFORE any update or event;
        // 'accepted'/'running' are in their own allowed sets, so this check
        // must come first (CR #58 P2-7).
        if state == next {
            tx.commit().await?;
            return self.get(tenant_id, project_id, execution_id).await;
        }
        if !allowed.contains(&state.as_str()) {
            return Err(PgError::Protocol(format!(
                "cannot move execution from {state} to {next}"
            )));
        }
        tx.execute(
            "UPDATE awr_team.executions SET state=$4
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &execution_id, &next],
        )
        .await?;
        // Execution lifecycle transitions are observable in the same
        // transaction (CR #41 P2-11).
        emit_event(
            &tx,
            tenant_id,
            project_id,
            "system",
            &row.get::<_, String>(4),
            &format!("execution.{next}"),
            json!({"execution_id": execution_id, "from": state, "to": next}),
        )
        .await?;
        tx.commit().await?;
        self.get(tenant_id, project_id, execution_id).await
    }
}

pub fn exactly_once_supported(fencing_class: &str) -> bool {
    matches!(fencing_class, "hard_fence" | "queryable_idempotent")
}

fn validate_fencing_class(class: &str) -> PgResult<()> {
    match class {
        "hard_fence" | "queryable_idempotent" | "uncontrolled" => Ok(()),
        _ => Err(PgError::Protocol("invalid fencing class".into())),
    }
}

fn scope_exceeded(declared: &Value, observed: &[String]) -> bool {
    let declared: Vec<String> = declared
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect();
    if declared.is_empty() {
        return !observed.is_empty();
    }
    // Directional containment, not symmetric overlap: an ancestor of the
    // declared scope is NOT inside it (CR #41 P2-10).
    observed
        .iter()
        .any(|path| !declared.iter().any(|item| path_within_scope(item, path)))
}

async fn insert_receipt(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    execution_id: &str,
    actor_id: &str,
    kind: &str,
    payload: &Value,
) -> PgResult<()> {
    let digest = format!("{:x}", Sha256::digest(payload.to_string().as_bytes()));
    tx.execute(
        "INSERT INTO awr_team.execution_receipts(
            tenant_id, project_id, id, execution_id, reporter_actor_id, receipt_kind,
            digest, payload_json)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        &[
            &tenant_id,
            &project_id,
            &new_id(),
            &execution_id,
            &actor_id,
            &kind,
            &digest,
            payload,
        ],
    )
    .await?;
    Ok(())
}

async fn lock_project(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
) -> PgResult<()> {
    crate::tx::lock_active_project(tx, tenant_id, project_id).await
}

async fn load_operation(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    actor_id: &str,
    client_id: &str,
    request_id: &str,
) -> PgResult<Option<(String, Value)>> {
    let row = tx
        .query_opt(
            "SELECT request_hash, result_json FROM awr_team.operations
             WHERE tenant_id=$1 AND project_id=$2 AND actor_id=$3 AND client_id=$4 AND request_id=$5",
            &[&tenant_id, &project_id, &actor_id, &client_id, &request_id],
        )
        .await?;
    Ok(row.map(|row| (row.get(0), row.get(1))))
}

async fn store_operation(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    actor_id: &str,
    client_id: &str,
    request_id: &str,
    op: &str,
    request_hash: &str,
    committed_revision: i64,
    result: &Value,
) -> PgResult<()> {
    // Receipts always carry the committed project revision (CR #41 P2-11).
    tx.execute(
        "INSERT INTO awr_team.operations(
            tenant_id, project_id, id, actor_id, client_id, request_id, op,
            request_hash, state, committed_project_revision, result_json)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'committed',$9,$10)",
        &[
            &tenant_id,
            &project_id,
            &new_id(),
            &actor_id,
            &client_id,
            &request_id,
            &op,
            &request_hash,
            &committed_revision,
            result,
        ],
    )
    .await?;
    Ok(())
}

fn replay_execution(result: &Value) -> PgResult<ExecutionRecord> {
    let required = |key: &str| -> PgResult<String> {
        result
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| PgError::Protocol(format!("stored receipt missing {key}")))
    };
    let fence = match result.get("fence") {
        // Decimal-string form (current) and legacy numeric form are both
        // accepted; a missing or invalid fence never defaults to 0 (CR #41
        // P2-12).
        Some(Value::String(s)) => s
            .parse()
            .map_err(|_| PgError::Protocol("stored receipt has invalid fence".into())),
        Some(Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| PgError::Protocol("stored receipt has invalid fence".into())),
        _ => Err(PgError::Protocol("stored receipt missing fence".into())),
    }?;
    Ok(ExecutionRecord {
        id: required("id")?,
        work_id: required("work_id")?,
        session_id: required("session_id")?,
        claim_id: required("claim_id")?,
        fence,
        contract_hash: required("contract_hash")?,
        effect_key: required("effect_key")?,
        state: required("state")?,
        cancel_requested: result["cancel_requested"].as_bool().unwrap_or(false),
        fencing_class: required("fencing_class")?,
        replayed: true,
    })
}
