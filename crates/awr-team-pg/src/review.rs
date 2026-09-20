use crate::error::{PgError, PgResult};
use crate::tx::{bind_scope, new_id};
use awr_team::{CompletionView, EvidenceBundle, EvidenceGrade, ReviewPolicy, current_completion};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize)]
pub struct EvidenceRecord {
    pub id: String,
    pub work_id: String,
    pub trust_basis: String,
    pub digest: String,
    pub contract_hash: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReviewRound {
    pub id: String,
    pub work_id: String,
    pub round_index: i32,
    pub bundle_hash: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompletionReceipt {
    pub id: String,
    pub work_id: String,
    pub contract_hash: String,
    pub policy: String,
}

pub struct ReviewStore {
    pool: crate::PgPool,
}

impl ReviewStore {
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

    pub async fn record_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        work_id: &str,
        contract_hash: &str,
        claimed_trust: Option<&str>,
        payload: &Value,
        artifact_bytes: Option<&[u8]>,
        input_digest: Option<&str>,
        dirty_tree: bool,
        execution_id: Option<&str>,
    ) -> PgResult<EvidenceRecord> {
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
        let trust_basis = assigned_trust(&actor_kind, claimed_trust);
        if dirty_tree && input_digest.is_none() && artifact_bytes.is_none() {
            return Err(PgError::EvidenceInvalid);
        }
        let payload = payload.clone();
        if payload.get("passed").and_then(Value::as_bool) == Some(true)
            && artifact_bytes.is_none()
            && payload.get("output_digest").is_none()
        {
            return Err(PgError::EvidenceInvalid);
        }
        // Two DIFFERENT digest contracts live on one evidence row
        // (CR #59 r3 P2-2):
        // - `output_digest` (artifact digest): sha256 of the submitted
        //   artifact bytes, verified against the persisted artifact.
        // - `execution_result_digest`: the executor-reported result digest
        //   the evidence declares to bind (payload "output_digest"),
        //   verified against the executions row at completion. The runner's
        //   result digest hashes path+content pairs, so it is NEVER equal
        //   to a single artifact's digest and must not be overwritten by it
        //   (CR #59 r3 P2-1).
        let output_digest = artifact_bytes.map(|bytes| sha256_hex(bytes));
        let execution_result_digest = payload
            .get("output_digest")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        // The evidence digest binds the FULL context — work, contract,
        // input, artifact identity and the declared execution result — so
        // an approval for one contract can never be reused by re-recording
        // the same payload under another contract (CR #42 P2-3).
        let digest = evidence_digest(
            work_id,
            contract_hash,
            input_digest,
            output_digest.as_deref(),
            execution_result_digest.as_deref(),
            &payload,
        )?;
        let mut artifact_id: Option<String> = None;
        if let Some(bytes) = artifact_bytes {
            let id = new_id();
            // Persist the actual bytes WITH the metadata: 'finalized' means
            // the content is durably readable, not merely described
            // (CR #42 P2-2).
            tx.execute(
                "INSERT INTO awr_team.artifacts(
                    tenant_id, project_id, id, object_key, sha256, byte_length,
                    media_type, state, created_by, content)
                 VALUES ($1,$2,$3,$4,$5,$6,'application/octet-stream','finalized',$7,$8)",
                &[
                    &tenant_id,
                    &project_id,
                    &id,
                    &format!("evidence/{id}"),
                    &sha256_hex(bytes),
                    &(bytes.len() as i64),
                    &actor_id,
                    &bytes,
                ],
            )
            .await?;
            artifact_id = Some(id);
        }
        let id = new_id();
        tx.execute(
            "INSERT INTO awr_team.evidence(
                tenant_id, project_id, id, work_id, execution_id, artifact_id,
                contract_hash, input_digest, output_digest, execution_result_digest,
                evidence_kind, trust_basis, digest, payload_json, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'report',$11,$12,$13,$14)",
            &[
                &tenant_id,
                &project_id,
                &id,
                &work_id,
                &execution_id.map(ToOwned::to_owned),
                &artifact_id,
                &contract_hash,
                &input_digest.map(ToOwned::to_owned),
                &output_digest,
                &execution_result_digest,
                &trust_basis,
                &digest,
                &payload,
                &actor_id,
            ],
        )
        .await?;
        crate::tx::emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            work_id,
            "evidence.recorded",
            json!({"evidence_id": id, "trust_basis": trust_basis, "digest": digest}),
        )
        .await?;
        tx.commit().await?;
        Ok(EvidenceRecord {
            id,
            work_id: work_id.into(),
            trust_basis,
            digest,
            contract_hash: contract_hash.into(),
        })
    }

    pub async fn open_review(
        &self,
        tenant_id: &str,
        project_id: &str,
        author_actor_id: &str,
        work_id: &str,
        evidence_id: &str,
    ) -> PgResult<ReviewRound> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let evidence = load_evidence(&tx, tenant_id, project_id, evidence_id).await?;
        if evidence.work_id != work_id {
            return Err(PgError::EvidenceInvalid);
        }
        // The author must be a real account in this tenant; accepting an
        // arbitrary string proves nothing about author/reviewer separation
        // (CR #42 P2-5).
        let author_exists: bool = tx
            .query_opt(
                "SELECT 1 FROM awr_team.actors WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &author_actor_id],
            )
            .await?
            .is_some();
        if !author_exists {
            return Err(PgError::Forbidden);
        }
        invalidate_open_rounds(&tx, tenant_id, project_id, work_id, &evidence.digest).await?;
        let round_index: i32 = tx
            .query_one(
                "SELECT COALESCE(max(round_index),0)+1 FROM awr_team.review_rounds
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?
            .get(0);
        let id = new_id();
        tx.execute(
            "INSERT INTO awr_team.review_rounds(
                tenant_id, project_id, id, work_id, round_index, bundle_hash,
                contract_hash, author_actor_id, state)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'open')",
            &[
                &tenant_id,
                &project_id,
                &id,
                &work_id,
                &round_index,
                &evidence.digest,
                &evidence.contract_hash,
                &author_actor_id,
            ],
        )
        .await?;
        crate::tx::emit_event(
            &tx,
            tenant_id,
            project_id,
            author_actor_id,
            work_id,
            "review.opened",
            json!({"round_id": id, "round_index": round_index, "bundle_hash": evidence.digest}),
        )
        .await?;
        tx.commit().await?;
        Ok(ReviewRound {
            id,
            work_id: work_id.into(),
            round_index,
            bundle_hash: evidence.digest,
            state: "open".into(),
        })
    }

    pub async fn decide_review(
        &self,
        tenant_id: &str,
        project_id: &str,
        reviewer_actor_id: &str,
        round_id: &str,
        decision: &str,
        reason: &str,
    ) -> PgResult<ReviewRound> {
        if !matches!(decision, "approve" | "reject") {
            return Err(PgError::Protocol("invalid review decision".into()));
        }
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT work_id, author_actor_id, bundle_hash, state, round_index, contract_hash
                 FROM awr_team.review_rounds
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3
                 FOR UPDATE",
                &[&tenant_id, &project_id, &round_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("review round not found".into()))?;
        let work_id: String = row.get(0);
        let author: String = row.get(1);
        let bundle_hash: String = row.get(2);
        let state: String = row.get(3);
        let round_index: i32 = row.get(4);
        if state != "open" {
            return Err(PgError::ReviewRequired);
        }
        if reviewer_actor_id == author {
            return Err(PgError::AuthorCannotReview);
        }
        // Reviewer: real, active, approval-capable member (status +
        // membership role), and not an agent (CR #42 P2-5).
        let reviewer_kind: String = tx
            .query_opt(
                "SELECT kind FROM awr_team.actors WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &reviewer_actor_id],
            )
            .await?
            .map(|r| r.get(0))
            .ok_or(PgError::Forbidden)?;
        if reviewer_kind == "agent" {
            return Err(PgError::AuthorCannotReview);
        }
        crate::tx::validate_reviewer(&tx, tenant_id, project_id, reviewer_actor_id).await?;
        let next = if decision == "approve" {
            "approved"
        } else {
            "rejected"
        };
        tx.execute(
            "INSERT INTO awr_team.review_decisions(
                tenant_id, project_id, id, review_round_id, work_id, bundle_hash,
                reviewer_actor_id, decision, reason)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            &[
                &tenant_id,
                &project_id,
                &new_id(),
                &round_id,
                &work_id,
                &bundle_hash,
                &reviewer_actor_id,
                &decision,
                &reason,
            ],
        )
        .await?;
        tx.execute(
            "UPDATE awr_team.review_rounds SET state=$4
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &round_id, &next],
        )
        .await?;
        crate::tx::emit_event(
            &tx,
            tenant_id,
            project_id,
            reviewer_actor_id,
            &work_id,
            "review.decided",
            json!({"round_id": round_id, "decision": decision}),
        )
        .await?;
        tx.commit().await?;
        Ok(ReviewRound {
            id: round_id.into(),
            work_id,
            round_index,
            bundle_hash,
            state: next.into(),
        })
    }

    /// Complete a work. Idempotent on (client_id, request_id): the same
    /// request returns the original receipt, changed parameters conflict
    /// (CR #42 P2-9). All gates, the receipt, the dependency mapping and the
    /// runtime update commit in ONE transaction (CR #42 P2-10).
    pub async fn complete(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        client_id: &str,
        request_id: &str,
        work_id: &str,
        scope_id: &str,
        evidence_id: &str,
        requested_policy: Option<&str>,
        context_complete: bool,
    ) -> PgResult<CompletionReceipt> {
        let op_args = json!({
            "work_id": work_id,
            "scope_id": scope_id,
            "evidence_id": evidence_id,
            "requested_policy": requested_policy,
            "context_complete": context_complete,
        });
        let request_hash = awr_team::request_hash(&json!({
            "op": "work.complete",
            "request_id": request_id,
            "args": op_args,
        }))
        .map_err(|e| PgError::Protocol(e.to_string()))?;
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        if let Some((stored_hash, result)) =
            load_operation(&tx, tenant_id, project_id, actor_id, client_id, request_id).await?
        {
            if stored_hash != request_hash {
                return Err(PgError::IdempotencyConflict);
            }
            let receipt_id = result
                .get("receipt_id")
                .and_then(Value::as_str)
                .ok_or_else(|| PgError::Protocol("stored completion receipt missing id".into()))?;
            return Ok(CompletionReceipt {
                id: receipt_id.into(),
                work_id: work_id.into(),
                contract_hash: result
                    .get("contract_hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                policy: result
                    .get("policy")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            });
        }
        if !context_complete {
            return Err(PgError::ContextIncomplete);
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
        // An explicit recovery block is a deliberate hold; completion must
        // not skip it nor silently clear it (CR #42 P2-8).
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
        let contract = current_contract(&tx, tenant_id, project_id, scope_id, work_id).await?;
        let policy = contract
            .get("completion_policy")
            .and_then(Value::as_str)
            .unwrap_or("trusted_execution_and_review");
        if let Some(requested) = requested_policy {
            if requested != policy {
                return Err(PgError::PolicyDowngrade);
            }
        }
        let evidence = load_evidence(&tx, tenant_id, project_id, evidence_id).await?;
        if evidence.work_id != work_id || evidence.contract_hash != current_contract_hash(&contract)
        {
            return Err(PgError::EvidenceInvalid);
        }
        if evidence_bytes_changed(&evidence)? {
            return Err(PgError::EvidenceInvalid);
        }
        if evidence.artifact_id.is_some() {
            let row = tx
                .query_opt(
                    "SELECT sha256, state, content FROM awr_team.artifacts
                     WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                    &[&tenant_id, &project_id, &evidence.artifact_id],
                )
                .await?
                .ok_or(PgError::EvidenceInvalid)?;
            let sha: String = row.get(0);
            let state: String = row.get(1);
            // The artifact must be ACTUALLY readable: re-read the persisted
            // bytes and verify them against both the artifact digest and the
            // evidence output digest (CR #42 P2-2). Metadata-only legacy
            // records cannot satisfy completion.
            let content: Option<Vec<u8>> = row.get(2);
            let content = content.ok_or(PgError::EvidenceInvalid)?;
            if state != "finalized"
                || sha256_hex(&content) != sha
                || evidence.output_digest.as_deref() != Some(sha.as_str())
            {
                return Err(PgError::EvidenceInvalid);
            }
        }
        let grade = match evidence.trust_basis.as_str() {
            "trusted_executor" => EvidenceGrade::TrustedExecutionReceipt,
            "human_review" => EvidenceGrade::AuthorizedReview,
            _ => EvidenceGrade::AgentSelfReport,
        };
        if policy == "ordinary_confirm" {
            let kind: String = tx
                .query_one(
                    "SELECT kind FROM awr_team.actors WHERE tenant_id=$1 AND id=$2",
                    &[&tenant_id, &actor_id],
                )
                .await?
                .get(0);
            if kind != "human" {
                return Err(PgError::Forbidden);
            }
        } else if grade != EvidenceGrade::TrustedExecutionReceipt {
            // trusted_execution_and_review requires a bound trusted
            // receipt — an AuthorizedReview grade (human material) must NOT
            // satisfy it, and AgentSelfReport is never trusted (CR #59 P2-1).
            return Err(PgError::EvidenceInvalid);
        } else {
            // A trusted-execution grade must bind to a SUCCEEDED execution
            // of the same contract and input by the delegated executor —
            // actor kind alone proves nothing (CR #42 P2-1). A report that
            // admits failure (passed=false) can never complete.
            if evidence.payload.get("passed").and_then(Value::as_bool) == Some(false) {
                return Err(PgError::EvidenceInvalid);
            }
            let execution_id = evidence
                .execution_id
                .as_deref()
                .ok_or(PgError::EvidenceInvalid)?;
            let exec = tx
                .query_opt(
                    "SELECT state, contract_hash, input_digest, executor_actor_id,
                            scope_id, result_digest
                     FROM awr_team.executions
                     WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                    &[&tenant_id, &project_id, &execution_id],
                )
                .await?
                .ok_or(PgError::EvidenceInvalid)?;
            let exec_state: String = exec.get(0);
            let exec_contract: String = exec.get(1);
            let exec_input: Option<String> = exec.get(2);
            let exec_executor: String = exec.get(3);
            let exec_scope: String = exec.get(4);
            let exec_result: Option<String> = exec.get(5);
            if exec_state != "succeeded" || exec_contract != evidence.contract_hash {
                return Err(PgError::EvidenceInvalid);
            }
            // The binding is complete: the evidence's submitter IS the
            // delegated executor, the scope matches the completion scope,
            // input digests are EQUAL on both sides (NULL does not skip),
            // and the declared execution result digest is present and equal
            // on both sides (CR #59 P2-1, CR #59 r3 P2-1).
            if exec_executor != evidence.created_by || exec_scope != scope_id {
                return Err(PgError::EvidenceInvalid);
            }
            if exec_input != evidence.input_digest {
                return Err(PgError::EvidenceInvalid);
            }
            // Output binding under the strict policy: the execution must
            // carry a recorded result digest AND the evidence must declare
            // the SAME digest. A missing value on either side proves
            // nothing and fails closed — None == None is not evidence of
            // output identity (CR #59 r3 P2-1). This binds the EXECUTION
            // RESULT digest; the artifact digest was verified against the
            // persisted artifact bytes above and is a separate contract
            // (CR #59 r3 P2-2).
            let declared_result = evidence
                .execution_result_digest
                .as_deref()
                .ok_or(PgError::EvidenceInvalid)?;
            let recorded_result = exec_result.as_deref().ok_or(PgError::EvidenceInvalid)?;
            if declared_result != recorded_result {
                return Err(PgError::EvidenceInvalid);
            }
        }
        let (binding_valid, dependency_links) =
            required_dependencies_covered(&tx, tenant_id, project_id, work_id, scope_id, &contract)
                .await?;
        let pinned_round: Option<String> = tx
            .query_opt(
                "SELECT id FROM awr_team.review_rounds
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND bundle_hash=$4
                 ORDER BY round_index DESC LIMIT 1",
                &[&tenant_id, &project_id, &work_id, &evidence.digest],
            )
            .await?
            .map(|row| row.get(0));
        let mut review =
            current_review(&tx, tenant_id, project_id, work_id, &evidence.digest).await?;
        if policy == "ordinary_confirm" {
            review.required = false;
        }
        // The consumed review must be for THIS contract's bundle — a new
        // contract never reuses an old approval (CR #42 P2-3). Only applies
        // where a review is required at all (ordinary_confirm has none).
        if policy != "ordinary_confirm" {
            let round_contract: Option<String> = tx
                .query_opt(
                    "SELECT contract_hash FROM awr_team.review_rounds
                     WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND bundle_hash=$4
                     ORDER BY round_index DESC LIMIT 1",
                    &[&tenant_id, &project_id, &work_id, &evidence.digest],
                )
                .await?
                .map(|row| row.get(0));
            if round_contract.as_deref() != Some(evidence.contract_hash.as_str()) {
                return Err(PgError::ReviewRequired);
            }
        }
        let bundle = EvidenceBundle {
            grade,
            contract_hash: evidence.contract_hash.clone(),
            artifact_digest: evidence.output_digest.clone(),
            accessible: true,
        };
        let view = current_completion(
            false,
            true,
            Some(&evidence.contract_hash),
            &evidence.contract_hash,
            binding_valid,
            Some(&bundle),
            &review,
        );
        if view != CompletionView::CurrentlyVerified {
            return Err(PgError::CompletionRejected);
        }
        // The receipt records the ACTUAL approver (from the pinned round's
        // decision) separately from the submitter (CR #42 audit note).
        // The approver comes from the SAME pinned round the gate consumed;
        // never re-pick from other rounds at receipt time (CR #59 P2-3).
        let approver: Option<String> = match &pinned_round {
            Some(round) => tx
                .query_opt(
                    "SELECT reviewer_actor_id FROM awr_team.review_decisions
                     WHERE tenant_id=$1 AND project_id=$2 AND review_round_id=$3
                       AND decision='approve'
                     ORDER BY created_at DESC LIMIT 1",
                    &[&tenant_id, &project_id, round],
                )
                .await?
                .map(|row| row.get(0)),
            None => None,
        };
        let approved_by = json!({"approved_by": approver, "submitted_by": actor_id});
        let dependency_binding_hash = sha256_hex(json!(dependency_links).to_string().as_bytes());
        let receipt_id = new_id();
        tx.execute(
            "INSERT INTO awr_team.completion_receipts(
                tenant_id, project_id, id, work_id, scope_id, contract_hash,
                result_digest, dependency_binding_hash, evidence_bundle_hash,
                policy, approved_by_json)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            &[
                &tenant_id,
                &project_id,
                &receipt_id,
                &work_id,
                &scope_id,
                &evidence.contract_hash,
                &evidence.digest,
                &dependency_binding_hash,
                &evidence.digest,
                &policy,
                &approved_by,
            ],
        )
        .await?;
        // Real mappings for audit: which evidence and which predecessor
        // receipts this completion consumed (CR #42 audit note).
        tx.execute(
            "INSERT INTO awr_team.completion_evidence(
                tenant_id, project_id, completion_id, evidence_id, criterion_id)
             VALUES ($1,$2,$3,$4,'contract')",
            &[&tenant_id, &project_id, &receipt_id, &evidence_id],
        )
        .await?;
        for (upstream_work, upstream_receipt) in &dependency_links {
            tx.execute(
                "INSERT INTO awr_team.completion_dependencies(
                    tenant_id, project_id, completion_id, predecessor_work_id,
                    predecessor_completion_id)
                 VALUES ($1,$2,$3,$4,$5)",
                &[
                    &tenant_id,
                    &project_id,
                    &receipt_id,
                    &upstream_work,
                    &upstream_receipt,
                ],
            )
            .await?;
        }
        tx.execute(
            "INSERT INTO awr_team.work_runtime(
                tenant_id, project_id, scope_id, work_id, state, work_version, last_fence,
                selected_completion_id)
             VALUES ($1,$2,$3,$4,'completed',1,0,$5)
             ON CONFLICT (tenant_id, project_id, scope_id, work_id)
             DO UPDATE SET state='completed', selected_completion_id=$5,
                 work_version = awr_team.work_runtime.work_version + 1",
            &[&tenant_id, &project_id, &scope_id, &work_id, &receipt_id],
        )
        .await?;
        let committed_revision = crate::tx::emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            work_id,
            "work.completed",
            json!({"receipt_id": receipt_id, "evidence_id": evidence_id, "policy": policy}),
        )
        .await?;
        let result = json!({
            "receipt_id": receipt_id,
            "contract_hash": evidence.contract_hash,
            "policy": policy,
            "committed_project_revision": committed_revision.to_string(),
        });
        store_operation(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            client_id,
            request_id,
            "work.complete",
            &request_hash,
            committed_revision,
            &result,
        )
        .await?;
        tx.commit().await?;
        Ok(CompletionReceipt {
            id: receipt_id,
            work_id: work_id.into(),
            contract_hash: evidence.contract_hash,
            policy: policy.into(),
        })
    }

    pub async fn source_declared_is_not_complete(
        &self,
        tenant_id: &str,
        project_id: &str,
        work_id: &str,
        scope_id: &str,
    ) -> PgResult<bool> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let completed: i64 = tx
            .query_one(
                "SELECT count(*) FROM awr_team.completion_receipts
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?
            .get(0);
        let runtime: Option<String> = tx
            .query_opt(
                "SELECT state FROM awr_team.work_runtime
                 WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4",
                &[&tenant_id, &project_id, &scope_id, &work_id],
            )
            .await?
            .map(|row| row.get(0));
        tx.commit().await?;
        Ok(completed == 0 && runtime.as_deref() != Some("completed"))
    }
}

struct LoadedEvidence {
    work_id: String,
    contract_hash: String,
    digest: String,
    trust_basis: String,
    payload: Value,
    artifact_id: Option<String>,
    output_digest: Option<String>,
    execution_result_digest: Option<String>,
    input_digest: Option<String>,
    execution_id: Option<String>,
    created_by: String,
}

fn assigned_trust(actor_kind: &str, claimed: Option<&str>) -> String {
    match actor_kind {
        "system" => "trusted_executor".into(),
        "human" => "human_review".into(),
        _ => {
            let _ = claimed;
            "caller_asserted".into()
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Canonical evidence digest: bound to work/contract/input/artifact AND
/// computed over canonical JSON, so a jsonb round-trip cannot drift it
/// (CR #42 P2-3).
pub(crate) fn evidence_digest(
    work_id: &str,
    contract_hash: &str,
    input_digest: Option<&str>,
    output_digest: Option<&str>,
    execution_result_digest: Option<&str>,
    payload: &Value,
) -> PgResult<String> {
    let canonical = awr_team::canonical_json(&json!({
        "work_id": work_id,
        "contract_hash": contract_hash,
        "input_digest": input_digest,
        "output_digest": output_digest,
        "execution_result_digest": execution_result_digest,
        "payload": payload,
    }))
    .map_err(|e| PgError::Protocol(e.to_string()))?;
    Ok(sha256_hex(&canonical))
}

fn evidence_bytes_changed(evidence: &LoadedEvidence) -> PgResult<bool> {
    let expected = evidence_digest(
        &evidence.work_id,
        &evidence.contract_hash,
        evidence.input_digest.as_deref(),
        evidence.output_digest.as_deref(),
        evidence.execution_result_digest.as_deref(),
        &evidence.payload,
    )?;
    Ok(expected != evidence.digest)
}

fn current_contract_hash(contract: &Value) -> String {
    contract
        .get("contract_hash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

async fn lock_project(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
) -> PgResult<()> {
    crate::tx::lock_active_project(tx, tenant_id, project_id).await
}

async fn load_evidence(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    evidence_id: &str,
) -> PgResult<LoadedEvidence> {
    let row = tx
        .query_opt(
            "SELECT work_id, contract_hash, digest, trust_basis, payload_json, artifact_id, output_digest,
                    execution_result_digest, input_digest, execution_id, created_by
             FROM awr_team.evidence
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &evidence_id],
        )
        .await?
        .ok_or(PgError::EvidenceInvalid)?;
    Ok(LoadedEvidence {
        work_id: row.get(0),
        contract_hash: row.get(1),
        digest: row.get(2),
        trust_basis: row.get(3),
        payload: row.get(4),
        artifact_id: row.get(5),
        output_digest: row.get(6),
        execution_result_digest: row.get(7),
        input_digest: row.get(8),
        execution_id: row.get(9),
        created_by: row.get(10),
    })
}

async fn current_contract(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    scope_id: &str,
    work_id: &str,
) -> PgResult<Value> {
    let row = tx
        .query_opt(
            "SELECT c.contract_hash, c.contract_json
             FROM awr_team.projects p
             JOIN awr_team.work_contracts c
               ON c.tenant_id=p.tenant_id AND c.project_id=p.id
              AND c.snapshot_id=p.active_snapshot_id
             WHERE p.tenant_id=$1 AND p.id=$2 AND c.scope_id=$3 AND c.work_id=$4",
            &[&tenant_id, &project_id, &scope_id, &work_id],
        )
        .await?
        .ok_or(PgError::EvidenceInvalid)?;
    let hash: String = row.get(0);
    let mut json: Value = row.get(1);
    json.as_object_mut()
        .map(|map| map.insert("contract_hash".into(), Value::String(hash)));
    Ok(json)
}

/// Required-dependency coverage: every required predecessor of the current
/// contract must have a completion receipt for ITS current contract; the
/// actual (upstream_work, receipt) pairs are returned for the completion
/// mapping. An empty required set passes; "no invalid binding rows" is NOT
/// proof of coverage (CR #42 P2-6).
async fn required_dependencies_covered(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    work_id: &str,
    scope_id: &str,
    contract: &Value,
) -> PgResult<(bool, Vec<(String, String)>)> {
    let invalid: i64 = tx
        .query_one(
            "SELECT count(*) FROM awr_team.dependency_bindings
             WHERE tenant_id=$1 AND project_id=$2 AND downstream_work_id=$3 AND valid=FALSE",
            &[&tenant_id, &project_id, &work_id],
        )
        .await?
        .get(0);
    if invalid > 0 {
        return Ok((false, vec![]));
    }
    // current_contract() returns the contract fields at the TOP level.
    let required: Vec<String> = contract
        .get("required_dependencies")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut links = Vec::new();
    for upstream in &required {
        let receipt: Option<String> = tx
            .query_opt(
                "SELECT r.id FROM awr_team.completion_receipts r
                 JOIN awr_team.work_runtime w
                   ON w.tenant_id=r.tenant_id AND w.project_id=r.project_id
                  AND w.scope_id=r.scope_id AND w.work_id=r.work_id
                  AND w.selected_completion_id=r.id AND w.state='completed'
                 JOIN awr_team.work_contracts c
                   ON c.tenant_id=r.tenant_id AND c.project_id=r.project_id
                  AND c.work_id=r.work_id AND c.scope_id=r.scope_id
                  AND c.contract_hash=r.contract_hash
                 JOIN awr_team.projects p
                   ON p.tenant_id=c.tenant_id AND p.id=c.project_id
                  AND p.active_snapshot_id=c.snapshot_id
                 WHERE r.tenant_id=$1 AND r.project_id=$2 AND r.work_id=$3
                   AND r.scope_id=$4",
                &[&tenant_id, &project_id, upstream, &scope_id],
            )
            .await?
            .map(|row| row.get(0));
        match receipt {
            Some(id) => links.push((upstream.clone(), id)),
            None => return Ok((false, vec![])),
        }
    }
    Ok((true, links))
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

async fn current_review(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    work_id: &str,
    bundle_hash: &str,
) -> PgResult<ReviewPolicy> {
    let row = tx
        .query_opt(
            "SELECT id, author_actor_id, state FROM awr_team.review_rounds
             WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND bundle_hash=$4
             ORDER BY round_index DESC LIMIT 1",
            &[&tenant_id, &project_id, &work_id, &bundle_hash],
        )
        .await?;
    match row {
        None => Ok(ReviewPolicy {
            required: true,
            author_may_self_approve: false,
            approved: false,
            reviewer_is_author: false,
        }),
        Some(row) => {
            let round_id: String = row.get(0);
            let author: String = row.get(1);
            let state: String = row.get(2);
            // Pin the round first, then read ITS decisions only: the latest
            // round and the latest decision must not be picked from
            // different rounds (CR #42 P2-4).
            let reviewer: Option<String> = tx
                .query_opt(
                    "SELECT reviewer_actor_id FROM awr_team.review_decisions d
                     WHERE d.tenant_id=$1 AND d.project_id=$2 AND d.review_round_id=$3
                     ORDER BY d.created_at DESC LIMIT 1",
                    &[&tenant_id, &project_id, &round_id],
                )
                .await?
                .map(|row| row.get(0));
            Ok(ReviewPolicy {
                required: true,
                author_may_self_approve: false,
                approved: state == "approved",
                reviewer_is_author: reviewer.as_deref() == Some(author.as_str()),
            })
        }
    }
}

async fn invalidate_open_rounds(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    work_id: &str,
    new_bundle: &str,
) -> PgResult<()> {
    tx.execute(
        "UPDATE awr_team.review_rounds SET state='invalidated'
         WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3
           AND state IN ('open','approved') AND bundle_hash <> $4",
        &[&tenant_id, &project_id, &work_id, &new_bundle],
    )
    .await?;
    Ok(())
}
