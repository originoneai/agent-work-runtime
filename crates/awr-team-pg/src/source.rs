use crate::error::{PgError, PgResult};
use crate::path::validate_package;
use crate::tx::{bind_scope, bind_workstream_scope, new_id};
use awr_team::{SourceActivationPlan, WorkContract, WorkId};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[path = "source_workstreams.rs"]
mod workstreams;
use workstreams::SourceProjection;

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct IngestRequest {
    pub tenant_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub parser_version: String,
    pub files: Vec<SourceFile>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CandidateRecord {
    pub snapshot_id: String,
    pub proposal_id: String,
    pub manifest_digest: String,
    pub parser_version: String,
    pub preview_hash: String,
    pub base_epoch: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CurrentSource {
    pub snapshot_id: String,
    pub manifest_digest: String,
    pub parser_version: String,
    pub authority_epoch: String,
    pub contract_hash: String,
}

/// The aggregate hash identifies the complete source projection. Individual
/// contract hashes retain the V1 codec and are never replaced by this hash.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CurrentWorkstreamSource {
    pub snapshot_id: String,
    pub manifest_digest: String,
    pub parser_version: String,
    pub authority_epoch: String,
    pub projection_hash: String,
    pub contract_hashes: BTreeMap<String, String>,
}

/// Trusted source coordinator API. This is not an authenticated transport;
/// callers must authorize source administration before invoking these methods.
pub struct SourceStore {
    pool: crate::PgPool,
}

impl SourceStore {
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

    pub async fn ingest(&self, request: IngestRequest) -> PgResult<CandidateRecord> {
        if request.parser_version.trim().is_empty() {
            return Err(PgError::Protocol("parser_version required".into()));
        }
        let files: Vec<(String, Vec<u8>)> = request
            .files
            .iter()
            .map(|f| (f.path.clone(), f.bytes.clone()))
            .collect();
        validate_package(&files)?;
        let projection = SourceProjection::parse(&files, &request.project_id)?;
        let preview_hash = projection.hash.clone();
        let manifest = build_manifest(&request.parser_version, &files)?;
        let manifest_digest = sha256_hex(
            &serde_json::to_vec(&manifest).map_err(|e| PgError::Protocol(e.to_string()))?,
        );

        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_workstream_scope(&tx, &request.tenant_id, &request.project_id).await?;
        crate::tx::lock_active_project(&tx, &request.tenant_id, &request.project_id).await?;
        let project = tx
            .query_opt(
                "SELECT authority_epoch FROM awr_team.projects
                 WHERE tenant_id=$1 AND id=$2 FOR SHARE",
                &[&request.tenant_id, &request.project_id],
            )
            .await?;
        let Some(project) = project else {
            return Err(PgError::ProjectNotAvailable);
        };
        let base_epoch: i64 = project.get(0);
        let snapshot_id = new_id();
        let proposal_id = new_id();
        let artifact_id = new_id();
        let source_ref = json!({
            "manifest": manifest,
            "files": files
                .iter()
                .map(|(path, bytes)| {
                    json!({
                        "path": path,
                        "sha256": sha256_hex(bytes),
                        "bytes": bytes.len() as u64,
                        "text": String::from_utf8_lossy(bytes),
                    })
                })
                .collect::<Vec<_>>(),
        });
        tx.execute(
            "INSERT INTO awr_team.artifacts(
                tenant_id, project_id, id, object_key, sha256, byte_length,
                media_type, state, created_by, content)
             VALUES ($1,$2,$3,$4,$5,$6,'application/json','finalized',$7,$8)",
            &[
                &request.tenant_id,
                &request.project_id,
                &artifact_id,
                &format!("snapshots/{snapshot_id}"),
                &manifest_digest,
                &(manifest.to_string().len() as i64),
                &request.actor_id,
                &manifest.to_string().into_bytes(),
            ],
        )
        .await?;
        tx.execute(
            "INSERT INTO awr_team.source_snapshots(
                tenant_id, project_id, id, manifest_digest, source_ref_json,
                artifact_id, parser_version, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            &[
                &request.tenant_id,
                &request.project_id,
                &snapshot_id,
                &manifest_digest,
                &source_ref,
                &artifact_id,
                &request.parser_version,
                &request.actor_id,
            ],
        )
        .await?;
        tx.execute(
            "INSERT INTO awr_team.source_proposals(
                tenant_id, project_id, id, base_epoch, candidate_snapshot_id,
                preview_hash, state, reason, author_actor_id)
             VALUES ($1,$2,$3,$4,$5,$6,'pending','ingest',$7)",
            &[
                &request.tenant_id,
                &request.project_id,
                &proposal_id,
                &base_epoch,
                &snapshot_id,
                &preview_hash,
                &request.actor_id,
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(CandidateRecord {
            snapshot_id,
            proposal_id,
            manifest_digest,
            parser_version: request.parser_version,
            preview_hash,
            base_epoch: base_epoch.to_string(),
        })
    }

    pub async fn approve(
        &self,
        tenant_id: &str,
        project_id: &str,
        proposal_id: &str,
        reviewer_actor_id: &str,
        candidate_digest: &str,
    ) -> PgResult<String> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_workstream_scope(&tx, tenant_id, project_id).await?;
        crate::tx::lock_active_project(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT p.author_actor_id, s.manifest_digest, p.state
                 FROM awr_team.source_proposals p
                 JOIN awr_team.source_snapshots s
                   ON s.tenant_id=p.tenant_id AND s.project_id=p.project_id
                  AND s.id=p.candidate_snapshot_id
                 WHERE p.tenant_id=$1 AND p.project_id=$2 AND p.id=$3
                 FOR UPDATE OF p",
                &[&tenant_id, &project_id, &proposal_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("proposal not found".into()))?;
        let author: String = row.get(0);
        let digest: String = row.get(1);
        let state: String = row.get(2);
        if author == reviewer_actor_id {
            return Err(PgError::AuthorCannotApprove);
        }
        validate_reviewer(&tx, tenant_id, project_id, reviewer_actor_id).await?;
        if digest != candidate_digest {
            return Err(PgError::StaleApproval);
        }
        if state != "pending" && state != "approved" {
            return Err(PgError::CandidateNotApproved);
        }
        let approval_id = new_id();
        tx.execute(
            "INSERT INTO awr_team.source_approvals(
                tenant_id, project_id, id, proposal_id, candidate_digest,
                reviewer_actor_id, decision)
             VALUES ($1,$2,$3,$4,$5,$6,'approve')",
            &[
                &tenant_id,
                &project_id,
                &approval_id,
                &proposal_id,
                &candidate_digest,
                &reviewer_actor_id,
            ],
        )
        .await?;
        tx.execute(
            "UPDATE awr_team.source_proposals SET state='approved'
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &proposal_id],
        )
        .await?;
        tx.commit().await?;
        Ok(approval_id)
    }

    pub async fn activate(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        proposal_id: &str,
        plan: &SourceActivationPlan,
    ) -> PgResult<CurrentSource> {
        let result = self
            .activate_inner(
                tenant_id,
                project_id,
                actor_id,
                proposal_id,
                plan,
                false,
                false,
            )
            .await?;
        Ok(CurrentSource {
            snapshot_id: result.snapshot_id,
            manifest_digest: result.manifest_digest,
            parser_version: result.parser_version,
            authority_epoch: result.authority_epoch,
            contract_hash: result.projection_hash,
        })
    }

    /// Explicit opt-in; never reinterpret `activate`'s singular contract hash.
    pub async fn activate_workstreams(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        proposal_id: &str,
        plan: &SourceActivationPlan,
    ) -> PgResult<CurrentWorkstreamSource> {
        self.activate_inner(
            tenant_id,
            project_id,
            actor_id,
            proposal_id,
            plan,
            true,
            false,
        )
        .await
    }

    #[cfg(feature = "pg-tests")]
    #[doc(hidden)]
    pub async fn abort_workstreams_after_installing_projection(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        proposal_id: &str,
        plan: &SourceActivationPlan,
    ) -> PgResult<()> {
        match self
            .activate_inner(
                tenant_id,
                project_id,
                actor_id,
                proposal_id,
                plan,
                true,
                true,
            )
            .await
        {
            Err(PgError::Protocol(message)) if message == "injected activate abort" => Ok(()),
            other => other.map(|_| ()),
        }
    }

    pub async fn abort_after_installing_projection(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        proposal_id: &str,
        plan: &SourceActivationPlan,
    ) -> PgResult<()> {
        match self
            .activate_inner(
                tenant_id,
                project_id,
                actor_id,
                proposal_id,
                plan,
                false,
                true,
            )
            .await
        {
            Err(PgError::Protocol(message)) if message == "injected activate abort" => Ok(()),
            other => other.map(|_| ()),
        }
    }

    async fn activate_inner(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        proposal_id: &str,
        plan: &SourceActivationPlan,
        scoped: bool,
        abort: bool,
    ) -> PgResult<CurrentWorkstreamSource> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        if scoped {
            bind_workstream_scope(&tx, tenant_id, project_id).await?;
            // Serialize enablement with every legacy entrypoint before taking
            // the project lock. Ordinary legacy updates share the mode row.
            tx.query_opt(
                "SELECT enabled FROM awr_team.workstream_modes
                WHERE tenant_id=$1 AND project_id=$2 FOR UPDATE",
                &[&tenant_id, &project_id],
            )
            .await?
            .ok_or(PgError::ProjectNotAvailable)?;
        } else {
            bind_scope(&tx, tenant_id, project_id).await?;
        }
        crate::tx::lock_active_project(&tx, tenant_id, project_id).await?;
        let locked = tx
            .query_opt(
                "SELECT authority_epoch, active_snapshot_id, project_revision
                 FROM awr_team.projects
                 WHERE tenant_id=$1 AND id=$2 FOR UPDATE",
                &[&tenant_id, &project_id],
            )
            .await?
            .ok_or(PgError::ProjectNotAvailable)?;
        let epoch: i64 = locked.get(0);
        let previous_snapshot: Option<String> = locked.get(1);
        let revision: i64 = locked.get(2);
        let expected: i64 = plan
            .expected_authority_epoch
            .parse()
            .map_err(|_| PgError::EpochMismatch)?;
        if expected != epoch {
            return Err(PgError::EpochMismatch);
        }

        let row = tx
            .query_opt(
                "SELECT p.state, p.base_epoch, s.id, s.manifest_digest, s.parser_version,
                        s.source_ref_json
                 FROM awr_team.source_proposals p
                 JOIN awr_team.source_snapshots s
                   ON s.tenant_id=p.tenant_id AND s.project_id=p.project_id
                  AND s.id=p.candidate_snapshot_id
                 WHERE p.tenant_id=$1 AND p.project_id=$2 AND p.id=$3",
                &[&tenant_id, &project_id, &proposal_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("proposal not found".into()))?;
        let state: String = row.get(0);
        let base_epoch: i64 = row.get(1);
        let snapshot_id: String = row.get(2);
        let digest: String = row.get(3);
        let parser_version: String = row.get(4);
        let source_ref: Value = row.get(5);
        // The candidate must have been generated from the CURRENT baseline.
        // A caller refreshing expected_authority_epoch after another
        // activation must not push a stale-base candidate over it (CR #37
        // P2-2). Deliberate rollback needs its own explicit operation.
        if base_epoch != epoch {
            return Err(PgError::EpochMismatch);
        }
        if state != "approved" {
            return Err(PgError::CandidateNotApproved);
        }
        if digest != plan.candidate_digest || digest != plan.approved_candidate_digest {
            return Err(PgError::StaleApproval);
        }
        if parser_version != plan.parser_version {
            return Err(PgError::ParserMismatch);
        }
        let approval = tx
            .query_opt(
                "SELECT candidate_digest, reviewer_actor_id FROM awr_team.source_approvals
                 WHERE tenant_id=$1 AND project_id=$2 AND proposal_id=$3
                   AND decision='approve'
                 ORDER BY decided_at DESC LIMIT 1",
                &[&tenant_id, &project_id, &proposal_id],
            )
            .await?
            .ok_or(PgError::CandidateNotApproved)?;
        let approved_digest: String = approval.get(0);
        let approval_reviewer: String = approval.get(1);
        if approved_digest != digest {
            return Err(PgError::StaleApproval);
        }
        // Approvals written before reviewer validation existed (or by any
        // legacy path) must not activate: the consumed approval is checked
        // with the SAME rules as a fresh one (CR #54 P2).
        validate_reviewer(&tx, tenant_id, project_id, &approval_reviewer).await?;

        let files = files_from_ref(&source_ref)?;
        validate_package(&files)?;
        let manifest = build_manifest(&parser_version, &files)?;
        if source_ref.get("manifest") != Some(&manifest)
            || sha256_hex(
                &serde_json::to_vec(&manifest).map_err(|e| PgError::Protocol(e.to_string()))?,
            ) != digest
        {
            return Err(PgError::SnapshotDrift("manifest".into()));
        }
        let projection = SourceProjection::parse(&files, project_id)?;
        if projection.bundle.is_some() != scoped {
            return Err(PgError::Unsupported(
                "activation API does not match the source codec".into(),
            ));
        }
        workstreams::reject_external_graph(&files)?;
        projection
            .validate_transition(&tx, tenant_id, project_id, previous_snapshot.as_deref())
            .await?;
        projection
            .install(&tx, tenant_id, project_id, &snapshot_id)
            .await?;
        let work_id = if scoped {
            None
        } else {
            Some(projection.contracts[0].work_id.as_str().to_string())
        };
        if abort {
            tx.rollback().await?;
            return Err(PgError::Protocol("injected activate abort".into()));
        }
        let next_epoch = epoch + 1;
        let next_revision = revision + 1;
        tx.execute(
            "UPDATE awr_team.projects
             SET active_snapshot_id=$1, authority_epoch=$2, project_revision=$3
             WHERE tenant_id=$4 AND id=$5 AND authority_epoch=$6",
            &[
                &snapshot_id,
                &next_epoch,
                &next_revision,
                &tenant_id,
                &project_id,
                &epoch,
            ],
        )
        .await?;
        tx.execute(
            "UPDATE awr_team.source_proposals SET state='activated'
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &proposal_id],
        )
        .await?;
        let event_id = new_id();
        let payload = json!({
            "snapshot_id": snapshot_id,
            "previous_snapshot_id": previous_snapshot,
            "parser_version": parser_version,
            "manifest_digest": digest,
        });
        tx.execute(
            "INSERT INTO awr_team.events(
                tenant_id, project_id, id, project_revision, event_index,
                event_type, actor_id, work_id, payload_json)
             VALUES ($1,$2,$3,$4,0,'source.activated',$5,$6,$7)",
            &[
                &tenant_id,
                &project_id,
                &event_id,
                &next_revision,
                &actor_id,
                &work_id,
                &payload,
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(CurrentWorkstreamSource {
            snapshot_id,
            manifest_digest: digest,
            parser_version,
            authority_epoch: next_epoch.to_string(),
            projection_hash: projection.hash,
            contract_hashes: projection.hashes,
        })
    }

    pub async fn current(
        &self,
        tenant_id: &str,
        project_id: &str,
        work_id: &str,
    ) -> PgResult<CurrentSource> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT p.active_snapshot_id, p.authority_epoch, s.manifest_digest,
                        s.parser_version, c.contract_hash
                 FROM awr_team.projects p
                 JOIN awr_team.source_snapshots s
                   ON s.tenant_id=p.tenant_id AND s.project_id=p.id
                  AND s.id=p.active_snapshot_id
                 JOIN awr_team.work_contracts c
                   ON c.tenant_id=p.tenant_id AND c.project_id=p.id
                  AND c.snapshot_id=p.active_snapshot_id AND c.work_id=$3
                 WHERE p.tenant_id=$1 AND p.id=$2",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?;
        let Some(row) = row else {
            return Err(PgError::InactiveCandidate);
        };
        Ok(CurrentSource {
            snapshot_id: row.get(0),
            authority_epoch: {
                let epoch: i64 = row.get(1);
                epoch.to_string()
            },
            manifest_digest: row.get(2),
            parser_version: row.get(3),
            contract_hash: row.get(4),
        })
    }

    pub async fn contract_for_snapshot(
        &self,
        tenant_id: &str,
        project_id: &str,
        snapshot_id: &str,
        work_id: &str,
    ) -> PgResult<String> {
        let current = self.current(tenant_id, project_id, work_id).await?;
        if current.snapshot_id != snapshot_id {
            return Err(PgError::InactiveCandidate);
        }
        Ok(current.contract_hash)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn build_manifest(parser_version: &str, files: &[(String, Vec<u8>)]) -> PgResult<Value> {
    Ok(json!({
        "schema_version": 1,
        "parser_version": parser_version,
        "files": files.iter().map(|(path, bytes)| json!({
            "path": path,
            "sha256": sha256_hex(bytes),
            "bytes": bytes.len() as u64,
        })).collect::<Vec<_>>(),
    }))
}

fn parse_contract(files: &[(String, Vec<u8>)]) -> PgResult<WorkContract> {
    let bytes = files
        .iter()
        .find(|(path, _)| path == "contract.json")
        .map(|(_, bytes)| bytes.as_slice())
        .ok_or_else(|| PgError::Protocol("contract.json required".into()))?;
    let mut contract: WorkContract =
        serde_json::from_slice(bytes).map_err(|e| PgError::Protocol(e.to_string()))?;
    if contract.work_id.as_str().is_empty() {
        contract.work_id = WorkId::new("work-a").map_err(|e| PgError::Protocol(e.to_string()))?;
    }
    contract
        .validate()
        .map_err(|e| PgError::Protocol(e.to_string()))?;
    Ok(contract)
}

async fn validate_reviewer(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    reviewer_actor_id: &str,
) -> PgResult<()> {
    crate::tx::validate_reviewer(tx, tenant_id, project_id, reviewer_actor_id).await
}

pub(crate) fn files_from_ref(source_ref: &Value) -> PgResult<Vec<(String, Vec<u8>)>> {
    let files = source_ref
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| PgError::Protocol("snapshot files missing".into()))?;
    files
        .iter()
        .map(|file| {
            let path = file
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| PgError::Protocol("file path missing".into()))?
                .to_string();
            let text = file
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| PgError::Protocol("file text missing".into()))?;
            let bytes = text.as_bytes().to_vec();
            // Fail closed on drift: the recorded digest/length describe the
            // ORIGINAL bytes, so restored content must match them exactly
            // (CR #37 P2-3).
            let expected_sha = file
                .get("sha256")
                .and_then(Value::as_str)
                .ok_or_else(|| PgError::Protocol("file sha256 missing".into()))?;
            let expected_len = file
                .get("bytes")
                .and_then(Value::as_u64)
                .ok_or_else(|| PgError::Protocol("file byte length missing".into()))?;
            if sha256_hex(&bytes) != expected_sha || bytes.len() as u64 != expected_len {
                return Err(PgError::SnapshotDrift(path));
            }
            Ok((path, bytes))
        })
        .collect()
}
