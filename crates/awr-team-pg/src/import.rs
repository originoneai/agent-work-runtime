use crate::error::{PgError, PgResult};
use crate::tx::{bind_scope, new_id};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize)]
pub struct InspectReport {
    pub fingerprints: Vec<String>,
    pub diverged: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImportJob {
    pub id: String,
    pub import_key: String,
    pub manifest_hash: String,
    pub state: String,
    pub replayed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct BackupRecord {
    pub id: String,
    pub manifest_hash: String,
    pub coordinator_epoch: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FencingBarrier {
    pub coordinator_epoch: String,
    pub tenant_id: String,
    pub project_id: String,
    pub scope_id: String,
    pub work_id: String,
    #[serde(serialize_with = "crate::tx::ser_i64_string")]
    pub fence: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct RestoreRun {
    pub id: String,
    pub new_epoch: String,
    pub outbox_replayed: bool,
    /// Install at each resource before reconciliation clears recovery_blocked.
    /// Database restore alone cannot revoke effects at a disconnected resource.
    pub fencing_barriers: Vec<FencingBarrier>,
}

pub struct ImportStore {
    pool: crate::PgPool,
}

impl ImportStore {
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

    pub fn inspect_sources(&self, sources: &[(&str, &str)]) -> PgResult<InspectReport> {
        let fingerprints: BTreeSet<String> =
            sources.iter().map(|(_, fp)| (*fp).to_owned()).collect();
        if fingerprints.len() > 1 {
            return Err(PgError::SourceDivergence);
        }
        Ok(InspectReport {
            fingerprints: fingerprints.into_iter().collect(),
            diverged: false,
        })
    }

    pub async fn freeze(&self, tenant_id: &str, project_id: &str) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let changed = tx.execute("UPDATE awr_team.projects SET status='frozen' WHERE tenant_id=$1 AND id=$2 AND status='active'", &[&tenant_id,&project_id]).await?;
        if changed > 0 {
            lifecycle(&tx, tenant_id, project_id, "project.frozen", json!({})).await?;
        } else {
            require_status(&tx, tenant_id, project_id, &["frozen"]).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// A frozen, project-locked snapshot. All ordinary writers use the same lock.
    pub async fn export(&self, tenant_id: &str, project_id: &str) -> PgResult<Value> {
        self.export_inner(tenant_id, project_id, None).await
    }
    #[cfg(feature = "pg-tests")]
    #[doc(hidden)]
    pub async fn export_with_sync_point(
        &self,
        tenant_id: &str,
        project_id: &str,
        sync: &tokio::sync::Barrier,
    ) -> PgResult<Value> {
        self.export_inner(tenant_id, project_id, Some(sync)).await
    }
    async fn export_inner(
        &self,
        tenant_id: &str,
        project_id: &str,
        sync: Option<&tokio::sync::Barrier>,
    ) -> PgResult<Value> {
        let mut client = self.connect().await?;
        let tx = client
            .build_transaction()
            .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
            .start()
            .await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        require_status(&tx, tenant_id, project_id, &["frozen"]).await?;
        let other_scope:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM awr_team.work_scopes WHERE tenant_id=$1 AND project_id=$2 AND id<>'main')", &[&tenant_id,&project_id]).await?.get(0);
        if other_scope {
            return Err(PgError::ScopeUnsupported);
        }
        let project=tx.query_one("SELECT active_snapshot_id,project_revision,coordinator_epoch FROM awr_team.projects WHERE tenant_id=$1 AND id=$2", &[&tenant_id,&project_id]).await?;
        let snapshot: Option<String> = project.get(0);
        let works=tx.query("SELECT w.id,w.external_key,c.contract_json,c.contract_hash FROM awr_team.work_items w LEFT JOIN awr_team.work_contracts c ON c.tenant_id=w.tenant_id AND c.project_id=w.project_id AND c.work_id=w.id AND c.scope_id='main' AND c.snapshot_id=$3 WHERE w.tenant_id=$1 AND w.project_id=$2 ORDER BY w.id", &[&tenant_id,&project_id,&snapshot]).await?;
        if let Some(sync) = sync {
            sync.wait().await;
            sync.wait().await;
        }
        let items=tx.query("SELECT to_jsonb(e),a.content,a.sha256 FROM awr_team.evidence e LEFT JOIN awr_team.artifacts a ON a.tenant_id=e.tenant_id AND a.project_id=e.project_id AND a.id=e.artifact_id WHERE e.tenant_id=$1 AND e.project_id=$2 ORDER BY e.id", &[&tenant_id,&project_id]).await?;
        let evidence = items
            .iter()
            .map(|row| {
                let mut e: Value = row.get(0);
                let bytes: Option<Vec<u8>> = row.get(1);
                let digest: Option<String> = row.get(2);
                let present = e["artifact_id"].is_null()
                    || bytes
                        .as_ref()
                        .zip(digest.as_ref())
                        .map(|(b, d)| sha256_hex(b) == *d)
                        .unwrap_or(false);
                e["artifact_bytes"] = json!(bytes);
                e["bytes_present"] = json!(present);
                e
            })
            .collect::<Vec<_>>();
        let edges = tx.query("SELECT from_work_id,to_work_id,relation,required FROM awr_team.dependency_edges WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main' ORDER BY from_work_id,to_work_id,relation", &[&tenant_id,&project_id,&snapshot]).await?.iter().map(|r| json!({"from":r.get::<_,String>(0),"to":r.get::<_,String>(1),"relation":r.get::<_,String>(2),"required":r.get::<_,bool>(3)})).collect::<Vec<_>>();
        let manifest = json!({"format":"awr-team-import-v1","scopes":["main"],
            "origin":{"tenant_id":tenant_id,"project_id":project_id,"snapshot_id":snapshot,"project_revision":project.get::<_,i64>(1).to_string(),"coordinator_epoch":project.get::<_,String>(2)},
            "dependency_edges":edges,"works":works.iter().map(|r|json!({"id":r.get::<_,String>(0),"external_key":r.get::<_,String>(1),"contract":r.get::<_,Option<Value>>(2),"contract_hash":r.get::<_,Option<String>>(3)})).collect::<Vec<_>>(), "evidence":evidence});
        self.dry_run(&manifest)?;
        tx.commit().await?;
        Ok(manifest)
    }

    /// The same deterministic validation is consumed by load and activation.
    /// Metadata-only legacy manifests may be staged but can never activate.
    pub fn dry_run(&self, manifest: &Value) -> PgResult<Value> {
        if !manifest.is_object() {
            return Err(invalid("manifest must be an object"));
        }
        if let Some(format) = manifest.get("format") {
            if format != "awr-team-import-v1" {
                return Err(invalid("unsupported manifest format"));
            }
        }
        let scopes = manifest
            .get("scopes")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("scopes required"))?;
        if scopes.len() != 1 || scopes[0] != "main" {
            return Err(PgError::ScopeUnsupported);
        }
        let works = manifest
            .get("works")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("works required"))?;
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut missing_contracts = vec![];
        for w in works {
            let id = required(w, "id")?;
            let key = required(w, "external_key")?;
            if !ids.insert(id) || !keys.insert(key) {
                return Err(invalid("duplicate work identity"));
            }
            if w.get("contract").is_none_or(Value::is_null) {
                missing_contracts.push(id);
                continue;
            }
            let c: awr_team::WorkContract = serde_json::from_value(w["contract"].clone())
                .map_err(|e| invalid(&e.to_string()))?;
            let hash = c.hash().map_err(|e| invalid(&e.to_string()))?;
            if c.work_id.as_str() != id || c.external_key != key {
                return Err(invalid("contract identity mismatch"));
            }
            if let Some(h) = w.get("contract_hash").filter(|h| !h.is_null()) {
                if h != &json!(hash) {
                    return Err(invalid("contract hash mismatch"));
                }
            }
        }
        let edges = manifest_edges(manifest)?;
        crate::validate_required_graph(
            &ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            &edges,
        )?;
        let items = manifest
            .get("evidence")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("evidence array required"))?;
        let mut evidence_ids = BTreeSet::new();
        let mut missing = vec![];
        for e in items {
            let id = required(e, "id")?;
            let work = required(e, "work_id")?;
            if !evidence_ids.insert(id) || !ids.contains(work) {
                return Err(invalid("invalid evidence identity/work"));
            }
            required(e, "contract_hash")?;
            if !e.get("payload_json").is_some_and(Value::is_object) {
                return Err(invalid("evidence payload required"));
            }
            if !matches!(
                required(e, "evidence_kind")?,
                "report" | "artifact" | "review_bundle" | "confirmation"
            ) {
                return Err(invalid("invalid evidence kind"));
            }
            for key in ["input_digest", "output_digest", "execution_result_digest"] {
                if e.get(key).is_some_and(|v| !v.is_null() && !v.is_string()) {
                    return Err(invalid("evidence digest must be a string or null"));
                }
            }
            let bytes = artifact_bytes(e)?;
            if e.get("bytes_present") == Some(&json!(false))
                || (e.get("artifact_id").is_some_and(|v| !v.is_null()) && bytes.is_none())
            {
                missing.push(id);
            }
            if let Some(bytes) = bytes {
                if e.get("output_digest").and_then(Value::as_str)
                    != Some(sha256_hex(&bytes).as_str())
                {
                    return Err(PgError::EvidenceInvalid);
                }
            }
        }
        Ok(
            json!({"manifest_hash":digest_value(manifest),"missing_evidence":missing,"missing_contracts":missing_contracts,"can_activate":missing.is_empty() && missing_contracts.is_empty() && !works.is_empty()}),
        )
    }

    pub async fn load(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        import_key: &str,
        manifest: &Value,
    ) -> PgResult<ImportJob> {
        let report = self.dry_run(manifest)?;
        let manifest_hash = digest_value(manifest);
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        if let Some(row)=tx.query_opt("SELECT id,state FROM awr_team.import_jobs WHERE tenant_id=$1 AND project_id=$2 AND import_key=$3 AND manifest_hash=$4", &[&tenant_id,&project_id,&import_key,&manifest_hash]).await? {
            return Ok(ImportJob{id:row.get(0),import_key:import_key.into(),manifest_hash,state:row.get(1),replayed:true});
        }
        require_status(&tx, tenant_id, project_id, &["frozen"]).await?;
        // A single staged import owns the project until activation; existing identities
        // must not be silently overwritten by caller-supplied historical material.
        let id = new_id();
        let snapshot_id = new_id();
        tx.execute("INSERT INTO awr_team.source_snapshots(tenant_id,project_id,id,manifest_digest,source_ref_json,parser_version,created_by) VALUES($1,$2,$3,$4,$5,'awr-team-import-v1',$6)", &[&tenant_id,&project_id,&snapshot_id,&manifest_hash,manifest,&actor_id]).await?;
        tx.execute("INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status) VALUES($1,$2,'main','main','active') ON CONFLICT DO NOTHING", &[&tenant_id,&project_id]).await?;
        let works = manifest["works"].as_array().unwrap();
        // A new projection must cover all existing identities as well.
        let existing=tx.query("SELECT id,external_key FROM awr_team.work_items WHERE tenant_id=$1 AND project_id=$2", &[&tenant_id,&project_id]).await?;
        for row in existing {
            if !works.iter().any(|w| {
                w["id"] == row.get::<_, String>(0) && w["external_key"] == row.get::<_, String>(1)
            }) {
                return Err(invalid("manifest omits or remaps existing work"));
            }
        }
        for w in works {
            let work_id = required(w, "id")?;
            let key = required(w, "external_key")?;
            tx.execute("INSERT INTO awr_team.work_items(tenant_id,project_id,id,external_key) VALUES($1,$2,$3,$4) ON CONFLICT(tenant_id,project_id,id) DO NOTHING", &[&tenant_id,&project_id,&work_id,&key]).await?;
            if let Some(c) = w.get("contract").filter(|v| !v.is_null()) {
                let parsed: awr_team::WorkContract =
                    serde_json::from_value(c.clone()).map_err(|e| invalid(&e.to_string()))?;
                let hash = parsed.hash().map_err(|e| invalid(&e.to_string()))?;
                tx.execute("INSERT INTO awr_team.work_contracts(tenant_id,project_id,snapshot_id,scope_id,work_id,contract_hash,definition_state,title,contract_json) VALUES($1,$2,$3,'main',$4,$5,'enabled',$6,$7)", &[&tenant_id,&project_id,&snapshot_id,&work_id,&hash,&key,c]).await?;
            }
            tx.execute("INSERT INTO awr_team.work_runtime(tenant_id,project_id,scope_id,work_id,state) VALUES($1,$2,'main',$3,'pending') ON CONFLICT DO NOTHING", &[&tenant_id,&project_id,&work_id]).await?;
        }
        for edge in manifest_edges(manifest)? {
            tx.execute("INSERT INTO awr_team.dependency_edges(tenant_id,project_id,snapshot_id,scope_id,from_work_id,to_work_id,relation,required) VALUES($1,$2,$3,'main',$4,$5,$6,$7)", &[&tenant_id,&project_id,&snapshot_id,&edge.from,&edge.to,&edge.relation,&edge.required]).await?;
        }
        for e in manifest["evidence"].as_array().unwrap() {
            let bytes = artifact_bytes(e)?;
            let artifact_id = bytes.as_ref().map(|_| new_id());
            if let (Some(bytes), Some(artifact_id)) = (&bytes, &artifact_id) {
                tx.execute("INSERT INTO awr_team.artifacts(tenant_id,project_id,id,object_key,sha256,byte_length,media_type,state,created_by,content) VALUES($1,$2,$3,$3,$4,$5,'application/octet-stream','finalized',$6,$7)", &[&tenant_id,&project_id,artifact_id,&sha256_hex(bytes),&(bytes.len() as i64),&actor_id,bytes]).await?;
            }
            // Trust is assigned locally, never imported from a self-asserted field.
            // Original execution/reviewer identity and digest remain provenance only.
            let payload = e["payload_json"].clone();
            let digest = crate::review::evidence_digest(
                required(e, "work_id")?,
                required(e, "contract_hash")?,
                e["input_digest"].as_str(),
                e["output_digest"].as_str(),
                e["execution_result_digest"].as_str(),
                &payload,
            )?;
            tx.execute("INSERT INTO awr_team.evidence(tenant_id,project_id,id,work_id,contract_hash,evidence_kind,trust_basis,digest,payload_json,created_by,artifact_id,input_digest,output_digest,execution_result_digest) VALUES($1,$2,$3,$4,$5,$6,'caller_asserted',$7,$8,$9,$10,$11,$12,$13)", &[&tenant_id,&project_id,&required(e,"id")?,&required(e,"work_id")?,&required(e,"contract_hash")?,&required(e,"evidence_kind")?,&digest,&payload,&actor_id,&artifact_id,&e["input_digest"].as_str(),&e["output_digest"].as_str(),&e["execution_result_digest"].as_str()]).await?;
        }
        tx.execute("INSERT INTO awr_team.import_jobs(tenant_id,project_id,id,import_key,manifest_hash,state,report_json,manifest_json,snapshot_id,missing_evidence_json) VALUES($1,$2,$3,$4,$5,'loaded',$6,$7,$8,$9)", &[&tenant_id,&project_id,&id,&import_key,&manifest_hash,&report,manifest,&snapshot_id,&report["missing_evidence"]]).await?;
        tx.execute(
            "UPDATE awr_team.projects SET status='importing' WHERE tenant_id=$1 AND id=$2",
            &[&tenant_id, &project_id],
        )
        .await?;
        lifecycle(
            &tx,
            tenant_id,
            project_id,
            "import.loaded",
            json!({"job_id":id,"manifest_hash":manifest_hash,"report":report}),
        )
        .await?;
        tx.commit().await?;
        Ok(ImportJob {
            id,
            import_key: import_key.into(),
            manifest_hash,
            state: "loaded".into(),
            replayed: false,
        })
    }

    pub async fn activate(
        &self,
        tenant_id: &str,
        project_id: &str,
        job_id: &str,
        unknown_executions: bool,
    ) -> PgResult<String> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let job=tx.query_opt("SELECT state,manifest_hash,manifest_json,report_json,snapshot_id FROM awr_team.import_jobs WHERE tenant_id=$1 AND project_id=$2 AND id=$3", &[&tenant_id,&project_id,&job_id]).await?.ok_or_else(||invalid("import job not found"))?;
        if job.get::<_, String>(0) != "loaded" {
            return Err(invalid("import job not loadable"));
        }
        require_status(&tx, tenant_id, project_id, &["importing"]).await?;
        let manifest: Option<Value> = job.get(2);
        let manifest = manifest.ok_or_else(|| invalid("legacy import has no verified manifest"))?;
        let report = self.dry_run(&manifest)?;
        if digest_value(&manifest) != job.get::<_, String>(1)
            || report != job.get::<_, Value>(3)
            || report["can_activate"] != true
        {
            return Err(PgError::RestoreIncomplete);
        }
        let unresolved:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM awr_team.executions WHERE tenant_id=$1 AND project_id=$2 AND state IN ('prepared','queued','accepted','running','unknown')) OR EXISTS(SELECT 1 FROM awr_team.work_runtime WHERE tenant_id=$1 AND project_id=$2 AND recovery_blocked) OR EXISTS(SELECT 1 FROM awr_team.claims WHERE tenant_id=$1 AND project_id=$2 AND state='active')", &[&tenant_id,&project_id]).await?.get(0);
        if unknown_executions || unresolved {
            return Err(PgError::RecoveryBlocked);
        }
        let snapshot: String = job
            .get::<_, Option<String>>(4)
            .ok_or(PgError::InactiveCandidate)?;
        verify_projection(&tx, tenant_id, project_id, &snapshot, &manifest).await?;
        verify_import_evidence(&tx, tenant_id, project_id, &manifest).await?;
        let epoch = format!("epoch-{}", new_id());
        tx.execute("UPDATE awr_team.projects SET status='active',coordinator_epoch=$3,active_snapshot_id=$4,authority_epoch=authority_epoch+1 WHERE tenant_id=$1 AND id=$2", &[&tenant_id,&project_id,&epoch,&snapshot]).await?;
        tx.execute("UPDATE awr_team.import_jobs SET state='activated' WHERE tenant_id=$1 AND project_id=$2 AND id=$3", &[&tenant_id,&project_id,&job_id]).await?;
        lifecycle(
            &tx,
            tenant_id,
            project_id,
            "import.activated",
            json!({"job_id":job_id,"snapshot_id":snapshot,"epoch":epoch}),
        )
        .await?;
        tx.commit().await?;
        Ok(epoch)
    }

    /// Explicit, transactional repair of pre-schema-9 source-manifest artifacts.
    /// Only material fully reconstructed and verified against its original digest
    /// is repaired; unrelated/missing artifacts and corrupt sources still fail closed.
    pub async fn repair_source_artifacts(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> PgResult<usize> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let rows=tx.query("SELECT a.id,a.sha256,a.byte_length,a.object_key,a.media_type,a.state,s.id,s.manifest_digest,s.source_ref_json,s.parser_version FROM awr_team.artifacts a JOIN awr_team.source_snapshots s ON s.tenant_id=a.tenant_id AND s.project_id=a.project_id AND s.artifact_id=a.id WHERE a.tenant_id=$1 AND a.project_id=$2 AND a.content IS NULL ORDER BY a.id,s.id FOR UPDATE OF a", &[&tenant_id,&project_id]).await?;
        let mut repaired = BTreeSet::new();
        for row in rows {
            let id: String = row.get(0);
            let snapshot: String = row.get(6);
            let digest: String = row.get(7);
            let source: Value = row.get(8);
            let parser: String = row.get(9);
            let files =
                crate::source::files_from_ref(&source).map_err(|_| PgError::RestoreIncomplete)?;
            let manifest = crate::source::build_manifest(&parser, &files)
                .map_err(|_| PgError::RestoreIncomplete)?;
            let bytes = manifest.to_string().into_bytes();
            if !repaired.insert(id.clone())
                || source["manifest"] != manifest
                || sha256_hex(&bytes) != digest
                || row.get::<_, String>(1) != digest
                || row.get::<_, i64>(2) != files.iter().map(|(_, b)| b.len() as i64).sum::<i64>()
                || row.get::<_, String>(3) != format!("snapshots/{snapshot}")
                || row.get::<_, String>(4) != "application/json"
                || !matches!(row.get::<_, String>(5).as_str(), "finalized" | "retained")
            {
                return Err(PgError::RestoreIncomplete);
            }
            tx.execute("UPDATE awr_team.artifacts SET content=$4,byte_length=$5 WHERE tenant_id=$1 AND project_id=$2 AND id=$3 AND content IS NULL", &[&tenant_id,&project_id,&id,&bytes,&(bytes.len() as i64)]).await?;
        }
        // Also verify non-repaired objects before committing any changes.
        inventory(&tx, tenant_id, project_id).await?;
        if !repaired.is_empty() {
            lifecycle(
                &tx,
                tenant_id,
                project_id,
                "source.artifacts_repaired",
                json!({"artifact_ids":repaired}),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(repaired.len())
    }

    /// Registers a verifiable logical inventory. Physical backup remains external.
    pub async fn backup(
        &self,
        tenant_id: &str,
        project_id: &str,
        artifact_digests: &[String],
        source_digests: &[String],
    ) -> PgResult<BackupRecord> {
        let mut client = self.connect().await?;
        crate::migrate::check_schema(&client).await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let inventory = inventory(&tx, tenant_id, project_id).await?;
        for d in artifact_digests {
            if !inventory["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["sha256"] == *d)
            {
                return Err(PgError::RestoreIncomplete);
            }
        }
        for d in source_digests {
            if !inventory["sources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["manifest_digest"] == *d)
            {
                return Err(PgError::RestoreIncomplete);
            }
        }
        let epoch: String = tx
            .query_one(
                "SELECT coordinator_epoch FROM awr_team.projects WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &project_id],
            )
            .await?
            .get(0);
        let manifest = json!({"format":"awr-team-backup-v1","schema_version":crate::EXPECTED_SCHEMA_VERSION,"epoch":epoch,"inventory":inventory});
        let hash = digest_value(&manifest);
        let id = new_id();
        tx.execute("INSERT INTO awr_team.backups(tenant_id,project_id,id,manifest_hash,coordinator_epoch,schema_version,artifact_digests_json,source_digests_json,manifest_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)", &[&tenant_id,&project_id,&id,&hash,&epoch,&crate::EXPECTED_SCHEMA_VERSION,&json!(artifact_digests),&json!(source_digests),&manifest]).await?;
        lifecycle(
            &tx,
            tenant_id,
            project_id,
            "backup.recorded",
            json!({"backup_id":id,"manifest_hash":hash}),
        )
        .await?;
        tx.commit().await?;
        Ok(BackupRecord {
            id,
            manifest_hash: hash,
            coordinator_epoch: epoch,
        })
    }

    pub async fn restore(
        &self,
        tenant_id: &str,
        project_id: &str,
        backup_id: &str,
        artifacts_present: bool,
        replay_outbox: bool,
    ) -> PgResult<RestoreRun> {
        if replay_outbox {
            return Err(PgError::OutboxReplayForbidden);
        }
        let mut client = self.connect().await?;
        crate::migrate::check_schema(&client).await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let backup=tx.query_opt("SELECT manifest_hash,schema_version,manifest_json FROM awr_team.backups WHERE tenant_id=$1 AND project_id=$2 AND id=$3", &[&tenant_id,&project_id,&backup_id]).await?.ok_or(PgError::RestoreIncomplete)?;
        let manifest: Option<Value> = backup.get(2);
        let observed = match inventory(&tx, tenant_id, project_id).await {
            Ok(value) => Some(value),
            Err(PgError::RestoreIncomplete) => None,
            Err(error) => return Err(error),
        };
        let verified = artifacts_present
            && backup.get::<_, i32>(1) == crate::EXPECTED_SCHEMA_VERSION
            && manifest.as_ref().is_some_and(|m| {
                digest_value(m) == backup.get::<_, String>(0)
                    && m["schema_version"] == crate::EXPECTED_SCHEMA_VERSION
                    && observed.as_ref() == Some(&m["inventory"])
            });
        let old_epoch: String = tx
            .query_one(
                "SELECT coordinator_epoch FROM awr_team.projects WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &project_id],
            )
            .await?
            .get(0);
        let new_epoch = format!("restored-{}", new_id());
        // Already-delivered effects are uncertain, never silently cancelled/replayed.
        tx.execute("UPDATE awr_team.executions SET state='unknown',unknown_reason='restore requires resource reconciliation',cancel_requested=TRUE WHERE tenant_id=$1 AND project_id=$2 AND state IN ('prepared','queued','accepted','running')", &[&tenant_id,&project_id]).await?;
        tx.execute("UPDATE awr_team.claims SET state='revoked',lease_version=lease_version+1 WHERE tenant_id=$1 AND project_id=$2 AND state='active'", &[&tenant_id,&project_id]).await?;
        tx.execute("UPDATE awr_team.sessions SET state='interrupted',session_version=session_version+1 WHERE tenant_id=$1 AND project_id=$2 AND state='active'", &[&tenant_id,&project_id]).await?;
        tx.execute("UPDATE awr_team.work_runtime SET recovery_blocked=TRUE,last_fence=last_fence+1,work_version=work_version+1 WHERE tenant_id=$1 AND project_id=$2", &[&tenant_id,&project_id]).await?;
        tx.execute("UPDATE awr_team.resource_reservations SET state='unknown' WHERE tenant_id=$1 AND project_id=$2 AND state='reserved'", &[&tenant_id,&project_id]).await?;
        tx.execute("UPDATE awr_team.outbox SET state='failed' WHERE tenant_id=$1 AND project_id=$2 AND state IN ('pending','sending')", &[&tenant_id,&project_id]).await?;
        let project_state = if verified { "active" } else { "degraded" };
        tx.execute("UPDATE awr_team.projects SET coordinator_epoch=$3,status=$4 WHERE tenant_id=$1 AND id=$2", &[&tenant_id,&project_id,&new_epoch,&project_state]).await?;
        // Credentials are tenant-scoped in V1; revocation is deliberately conservative.
        tx.execute("UPDATE awr_team.credentials SET revoked_at=clock_timestamp() WHERE tenant_id=$1 AND revoked_at IS NULL", &[&tenant_id]).await?;
        let id = new_id();
        let mut fencing_barriers:Vec<FencingBarrier>=tx.query("SELECT scope_id,work_id,last_fence FROM awr_team.work_runtime WHERE tenant_id=$1 AND project_id=$2 ORDER BY scope_id,work_id", &[&tenant_id,&project_id]).await?.iter().map(|r|FencingBarrier{coordinator_epoch:new_epoch.clone(),tenant_id:tenant_id.into(),project_id:project_id.into(),scope_id:r.get(0),work_id:r.get(1),fence:r.get(2)}).collect();
        let run_state = if verified { "completed" } else { "blocked" };
        // Even an empty historical project can have post-backup deliveries.
        if fencing_barriers.is_empty() {
            fencing_barriers.push(FencingBarrier {
                coordinator_epoch: new_epoch.clone(),
                tenant_id: tenant_id.into(),
                project_id: project_id.into(),
                scope_id: String::new(),
                work_id: String::new(),
                fence: 0,
            });
        }
        let report = json!({"old_epoch":old_epoch,"inventory_verified":verified,"execution_recovery_required":true,"fencing_barriers":fencing_barriers});
        tx.execute("INSERT INTO awr_team.restore_runs(tenant_id,project_id,id,backup_id,new_epoch,outbox_replayed,state,report_json) VALUES($1,$2,$3,$4,$5,FALSE,$6,$7)", &[&tenant_id,&project_id,&id,&backup_id,&new_epoch,&run_state,&report]).await?;
        lifecycle(&tx,tenant_id,project_id,if verified {"restore.completed"} else {"restore.blocked"},json!({"restore_id":id,"backup_id":backup_id,"old_epoch":old_epoch,"new_epoch":new_epoch,"recovery_blocked":true,"inventory_verified":verified})).await?;
        tx.commit().await?;
        // Integrity failure still commits the protective fence/epoch boundary and
        // a blocked audit record. It never leaves a restored project writable.
        if !verified {
            return Err(PgError::RestoreIncomplete);
        }
        Ok(RestoreRun {
            id,
            new_epoch,
            outbox_replayed: false,
            fencing_barriers,
        })
    }

    pub async fn refuse_sqlite_rollback(&self, tenant_id: &str, project_id: &str) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let revision: i64 = tx
            .query_one(
                "SELECT project_revision FROM awr_team.projects WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &project_id],
            )
            .await?
            .get(0);
        tx.commit().await?;
        if revision > 0 {
            return Err(PgError::RollbackForbidden);
        }
        Ok(())
    }

    pub async fn require_epoch(
        &self,
        tenant_id: &str,
        project_id: &str,
        epoch: &str,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let current: String = tx
            .query_one(
                "SELECT coordinator_epoch FROM awr_team.projects WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &project_id],
            )
            .await?
            .get(0);
        tx.commit().await?;
        if current != epoch {
            return Err(PgError::EpochChanged);
        }
        Ok(())
    }

    pub async fn local_claim_is_not_team_lease(
        &self,
        tenant_id: &str,
        project_id: &str,
        work_id: &str,
    ) -> PgResult<bool> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let active: i64 = tx
            .query_one(
                "SELECT count(*) FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='active'",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?
            .get(0);
        tx.commit().await?;
        Ok(active == 0)
    }
}

fn invalid(message: &str) -> PgError {
    PgError::Protocol(message.into())
}
fn required<'a>(v: &'a Value, key: &str) -> PgResult<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid(&format!("{key} required")))
}
fn digest_value(v: &Value) -> String {
    // Canonical key order is stable through PostgreSQL jsonb and transport.
    sha256_hex(
        awr_team::canonical_json(v)
            .expect("JSON value is canonicalizable")
            .as_slice(),
    )
}
fn artifact_bytes(e: &Value) -> PgResult<Option<Vec<u8>>> {
    match e.get("artifact_bytes").filter(|v| !v.is_null()) {
        None => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|_| invalid("artifact_bytes must be bytes")),
    }
}
async fn lifecycle(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    kind: &str,
    payload: Value,
) -> PgResult<()> {
    crate::tx::emit_event(tx, tenant, project, "system", "", kind, payload).await?;
    Ok(())
}
async fn require_status(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    allowed: &[&str],
) -> PgResult<()> {
    let state: String = tx
        .query_one(
            "SELECT status FROM awr_team.projects WHERE tenant_id=$1 AND id=$2",
            &[&tenant, &project],
        )
        .await?
        .get(0);
    if !allowed.contains(&state.as_str()) {
        return Err(PgError::ProjectNotAvailable);
    }
    Ok(())
}
// Legacy hand-authored manifests declare only contract dependencies. Explicit
// graphs are complete: never silently supplement a missing or altered edge.
fn manifest_edges(manifest: &Value) -> PgResult<Vec<crate::DependencyEdge>> {
    let works = manifest["works"]
        .as_array()
        .ok_or_else(|| invalid("works required"))?;
    let mut contract_edges = vec![];
    let mut ids = BTreeSet::new();
    for w in works {
        let id = required(w, "id")?;
        ids.insert(id);
        if let Some(deps) = w["contract"]["required_dependencies"].as_array() {
            for dep in deps {
                contract_edges.push(crate::DependencyEdge {
                    from: id.into(),
                    to: dep
                        .as_str()
                        .ok_or_else(|| invalid("dependency id required"))?
                        .into(),
                    relation: "requires".into(),
                    required: true,
                });
            }
        }
    }
    let edges: Vec<crate::DependencyEdge> = match manifest.get("dependency_edges") {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| invalid("invalid dependency_edges"))?,
        None => contract_edges.clone(),
    };
    let mut keys = BTreeSet::new();
    for edge in &edges {
        if !ids.contains(edge.from.as_str()) || !ids.contains(edge.to.as_str()) {
            return Err(PgError::MissingDependency);
        }
        if edge.relation.trim().is_empty() || !keys.insert((&edge.from, &edge.to, &edge.relation)) {
            return Err(invalid("empty or duplicate dependency relation"));
        }
    }
    if !contract_edges.iter().all(|e| edges.contains(e)) {
        return Err(invalid(
            "explicit graph omits a required contract dependency",
        ));
    }
    Ok(edges)
}

async fn verify_projection(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    snapshot: &str,
    manifest: &Value,
) -> PgResult<()> {
    let source=tx.query_opt("SELECT manifest_digest,source_ref_json FROM awr_team.source_snapshots WHERE tenant_id=$1 AND project_id=$2 AND id=$3", &[&tenant,&project,&snapshot]).await?.ok_or(PgError::InactiveCandidate)?;
    if source.get::<_, Value>(1) != *manifest
        || source.get::<_, String>(0) != digest_value(manifest)
    {
        return Err(PgError::RestoreIncomplete);
    }
    let rows=tx.query("SELECT work_id,contract_hash,contract_json FROM awr_team.work_contracts WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main'", &[&tenant,&project,&snapshot]).await?;
    let works = manifest["works"]
        .as_array()
        .ok_or(PgError::InactiveCandidate)?;
    if rows.len() != works.len() {
        return Err(PgError::InactiveCandidate);
    }
    for w in works {
        let row = rows
            .iter()
            .find(|r| w["id"] == r.get::<_, String>(0))
            .ok_or(PgError::InactiveCandidate)?;
        let c: awr_team::WorkContract = serde_json::from_value(w["contract"].clone())
            .map_err(|_| PgError::InactiveCandidate)?;
        if row.get::<_, Value>(2) != w["contract"]
            || row.get::<_, String>(1) != c.hash().map_err(|_| PgError::InactiveCandidate)?
        {
            return Err(PgError::InactiveCandidate);
        }
    }
    let expected: BTreeSet<_> = manifest_edges(manifest)?
        .into_iter()
        .map(|e| (e.from, e.to, e.relation, e.required))
        .collect();
    let edges=tx.query("SELECT from_work_id,to_work_id,relation,required FROM awr_team.dependency_edges WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main'", &[&tenant,&project,&snapshot]).await?;
    let actual: BTreeSet<(String, String, String, bool)> = edges
        .iter()
        .map(|r| (r.get(0), r.get(1), r.get(2), r.get(3)))
        .collect();
    if actual != expected {
        return Err(PgError::InactiveCandidate);
    }
    Ok(())
}
async fn verify_import_evidence(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    manifest: &Value,
) -> PgResult<()> {
    for e in manifest["evidence"]
        .as_array()
        .ok_or(PgError::EvidenceInvalid)?
    {
        let row=tx.query_opt("SELECT e.work_id,e.contract_hash,e.payload_json,e.trust_basis,a.content,to_jsonb(e),a.sha256,a.byte_length,a.state FROM awr_team.evidence e LEFT JOIN awr_team.artifacts a ON a.tenant_id=e.tenant_id AND a.project_id=e.project_id AND a.id=e.artifact_id WHERE e.tenant_id=$1 AND e.project_id=$2 AND e.id=$3", &[&tenant,&project,&required(e,"id")?]).await?.ok_or(PgError::EvidenceInvalid)?;
        let stored: Value = row.get(5);
        for key in [
            "evidence_kind",
            "input_digest",
            "output_digest",
            "execution_result_digest",
        ] {
            if stored[key] != e[key] {
                return Err(PgError::EvidenceInvalid);
            }
        }
        if !stored["execution_id"].is_null() {
            return Err(PgError::EvidenceInvalid);
        }
        let digest = crate::review::evidence_digest(
            required(e, "work_id")?,
            required(e, "contract_hash")?,
            e["input_digest"].as_str(),
            e["output_digest"].as_str(),
            e["execution_result_digest"].as_str(),
            &e["payload_json"],
        )?;
        if stored["digest"] != digest {
            return Err(PgError::EvidenceInvalid);
        }
        if let Some(bytes) = row.get::<_, Option<Vec<u8>>>(4) {
            if row.get::<_, Option<String>>(6) != Some(sha256_hex(&bytes))
                || row.get::<_, Option<i64>>(7) != Some(bytes.len() as i64)
                || !matches!(
                    row.get::<_, Option<String>>(8).as_deref(),
                    Some("finalized" | "retained")
                )
            {
                return Err(PgError::EvidenceInvalid);
            }
        } else if !stored["artifact_id"].is_null() {
            return Err(PgError::EvidenceInvalid);
        }
        if row.get::<_, String>(0) != required(e, "work_id")?
            || row.get::<_, String>(1) != required(e, "contract_hash")?
            || row.get::<_, Value>(2) != e["payload_json"]
            || row.get::<_, String>(3) != "caller_asserted"
            || row.get::<_, Option<Vec<u8>>>(4) != artifact_bytes(e)?
        {
            return Err(PgError::EvidenceInvalid);
        }
    }
    Ok(())
}
/// Full source and artifact inventory, verified against available content.
/// Comparison after physical restore detects missing objects, changed bytes,
/// changed projections, or an active-source pointer from another backup.
async fn inventory(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
) -> PgResult<Value> {
    let active: Option<String> = tx
        .query_one(
            "SELECT active_snapshot_id FROM awr_team.projects WHERE tenant_id=$1 AND id=$2",
            &[&tenant, &project],
        )
        .await?
        .get(0);
    let sources=tx.query("SELECT id,manifest_digest,source_ref_json,parser_version FROM awr_team.source_snapshots WHERE tenant_id=$1 AND project_id=$2 ORDER BY id", &[&tenant,&project]).await?;
    let mut source_records = vec![];
    for row in sources {
        let source: Value = row.get(2);
        let digest: String = row.get(1);
        let parser: String = row.get(3);
        if parser == "awr-team-import-v1" {
            if digest_value(&source) != digest {
                return Err(PgError::RestoreIncomplete);
            }
        } else {
            let files =
                crate::source::files_from_ref(&source).map_err(|_| PgError::RestoreIncomplete)?;
            let rebuilt = crate::source::build_manifest(&parser, &files)
                .map_err(|_| PgError::RestoreIncomplete)?;
            if source["manifest"] != rebuilt || sha256_hex(rebuilt.to_string().as_bytes()) != digest
            {
                return Err(PgError::RestoreIncomplete);
            }
        }
        source_records.push(json!({"id":row.get::<_,String>(0),"manifest_digest":digest,"source_ref_digest":digest_value(&source),"parser_version":parser}));
    }
    if active
        .as_ref()
        .is_some_and(|id| !source_records.iter().any(|s| s["id"] == *id))
    {
        return Err(PgError::RestoreIncomplete);
    }
    let rows=tx.query("SELECT id,sha256,byte_length,content,state FROM awr_team.artifacts WHERE tenant_id=$1 AND project_id=$2 ORDER BY id", &[&tenant,&project]).await?;
    let mut artifacts = vec![];
    for row in rows {
        let bytes: Vec<u8> = row
            .get::<_, Option<Vec<u8>>>(3)
            .ok_or(PgError::RestoreIncomplete)?;
        let digest: String = row.get(1);
        if sha256_hex(&bytes) != digest
            || bytes.len() as i64 != row.get::<_, i64>(2)
            || !matches!(row.get::<_, String>(4).as_str(), "finalized" | "retained")
        {
            return Err(PgError::RestoreIncomplete);
        }
        artifacts
            .push(json!({"id":row.get::<_,String>(0),"sha256":digest,"byte_length":bytes.len()}));
    }
    let rows=tx.query("SELECT to_jsonb(c) FROM awr_team.work_contracts c WHERE tenant_id=$1 AND project_id=$2 ORDER BY snapshot_id,scope_id,work_id", &[&tenant,&project]).await?;
    let mut contracts = vec![];
    for row in rows {
        let c: Value = row.get(0);
        let parsed: awr_team::WorkContract = serde_json::from_value(c["contract_json"].clone())
            .map_err(|_| PgError::RestoreIncomplete)?;
        if json!(parsed.hash().map_err(|_| PgError::RestoreIncomplete)?) != c["contract_hash"] {
            return Err(PgError::RestoreIncomplete);
        }
        contracts.push(c);
    }
    let edges=tx.query("SELECT to_jsonb(d) FROM awr_team.dependency_edges d WHERE tenant_id=$1 AND project_id=$2 ORDER BY snapshot_id,scope_id,from_work_id,to_work_id,relation", &[&tenant,&project]).await?.iter().map(|r|r.get::<_,Value>(0)).collect::<Vec<_>>();
    Ok(
        json!({"tenant_id":tenant,"project_id":project,"active_snapshot_id":active,"sources":source_records,"artifacts":artifacts,"contracts":contracts,"edges":edges}),
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

async fn lock_project(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
) -> PgResult<()> {
    tx.query_opt(
        "SELECT id FROM awr_team.projects WHERE tenant_id=$1 AND id=$2 FOR UPDATE",
        &[&tenant_id, &project_id],
    )
    .await?
    .ok_or(PgError::ProjectNotAvailable)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn personal_cli_does_not_depend_on_team_postgres() {
        let toml = include_str!("../../awr-cli/Cargo.toml");
        assert!(!toml.contains("awr-team-pg"));
        let store = include_str!("../../awr-store/Cargo.toml");
        assert!(!store.contains("awr-team-pg"));
    }
}
