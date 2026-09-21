//! Caller-managed execution: a transactional admission decision, not a remote
//! process supervisor. Unverified observations cannot settle external effects.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Start {
    pub(super) session_id: String,
    pub(super) expected_session_version: String,
    execution_id: String,
    expected_execution_version: String,
    expected_work_version: String,
    claim_id: String,
    expected_fence: String,
    expected_lease_version: String,
    execution_mode: String,
    expected_input_digest: Option<String>,
}
impl Start {
    pub(super) fn parse(args: Value) -> PgResult<Self> {
        let a: Self = serde_json::from_value(args).map_err(|_| invalid())?;
        if !identity(&a.execution_id)
            || !identity(&a.claim_id)
            || version(&a.expected_execution_version)? == 0
            || version(&a.expected_work_version)? == 0
            || version(&a.expected_fence)? == 0
            || version(&a.expected_lease_version)? == 0
        {
            return Err(invalid());
        }
        if !matches!(
            a.execution_mode.as_str(),
            "caller_managed" | "reference_write_v1"
        ) {
            return Err(PgError::Unsupported("execution mode".into()));
        }
        if a.expected_input_digest.as_ref().is_some_and(|s| !digest(s))
            || (a.execution_mode == "reference_write_v1" && a.expected_input_digest.is_none())
        {
            return Err(invalid());
        }
        Ok(a)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    pub(super) session_id: String,
    pub(super) expected_session_version: String,
    execution_id: String,
    expected_execution_version: String,
    outcome: String,
    output_digest: Option<String>,
    observed_paths: Vec<String>,
    note: String,
}
impl Report {
    pub(super) fn parse(args: Value) -> PgResult<Self> {
        let a: Self = serde_json::from_value(args).map_err(|_| invalid())?;
        if !identity(&a.execution_id)
            || version(&a.expected_execution_version)? == 0
            || !matches!(
                a.outcome.as_str(),
                "succeeded" | "failed" | "cancelled" | "unknown"
            )
            || a.output_digest.as_ref().is_some_and(|s| !digest(s))
            || (a.outcome == "succeeded" && a.output_digest.is_none())
            || a.observed_paths.len() > 128
            || a.observed_paths.iter().any(|p| !canonical_path(p))
            || a.note.trim().is_empty()
            || a.note.len() > 4096
            || a.note.contains('\0')
        {
            return Err(invalid());
        }
        Ok(a)
    }
}

async fn owned_execution(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    command: &WorkstreamCommand,
    ownership: i64,
    session: &str,
    execution: &str,
    expected_version: &str,
) -> PgResult<Row> {
    let r = load(tx, tenant, project, execution).await?;
    require_binding(
        &r,
        &command.work_id,
        &command.workstream_id.to_string(),
        ownership,
    )?;
    if r.get::<_, Option<String>>("session_id").as_deref() != Some(session)
        || r.get::<_, String>("executor_actor_id") != auth.actor_id
        || r.get::<_, Option<String>>("executor_client_id").as_deref() != Some(&auth.client_id)
    {
        return Err(PgError::Forbidden);
    }
    if r.get::<_, Option<String>>("coordinator_epoch").as_deref() != Some(&auth.epoch) {
        return Err(PgError::EpochChanged);
    }
    let v: i64 = r.get("execution_version");
    if v != version(expected_version)? || v == i64::MAX {
        return Err(PgError::PreconditionsChanged);
    }
    Ok(r)
}

pub(super) async fn start(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    command: &WorkstreamCommand,
    ownership: i64,
    contract: &awr_team::WorkContract,
    a: Start,
) -> PgResult<Value> {
    let r = owned_execution(
        tx,
        tenant,
        project,
        auth,
        command,
        ownership,
        &a.session_id,
        &a.execution_id,
        &a.expected_execution_version,
    )
    .await?;
    if a.expected_input_digest
        .as_ref()
        .is_some_and(|input| r.get::<_, Option<String>>("input_digest").as_ref() != Some(input))
    {
        return Err(PgError::PreconditionsChanged);
    }
    // The bundled adapter may attest only when the operator delegated that
    // authority before admission. A mode name cannot grant executor trust.
    if a.execution_mode == "reference_write_v1"
        && !auth
            .execution_access
            .get(&command.workstream_id)
            .is_some_and(|a| a.attest)
    {
        return Err(PgError::Forbidden);
    }
    if r.get::<_, String>("state") != "prepared"
        || r.get::<_, bool>("cancel_requested")
        || r.get::<_, String>("contract_hash") != command.expected_contract_hash
        || r.get::<_, String>("fencing_class") != "uncontrolled"
    {
        return Err(PgError::PreconditionsChanged);
    }
    if r.get::<_, Option<String>>("claim_id").as_deref() != Some(&a.claim_id) {
        return Err(PgError::Forbidden);
    }
    require_enabled(tx, tenant, project, auth, command).await?;
    claims::require_live(
        tx,
        tenant,
        project,
        auth,
        command,
        ownership,
        &a.session_id,
        &a.claim_id,
        &a.expected_fence,
        &a.expected_lease_version,
    )
    .await?;
    require_ready(
        tx,
        tenant,
        project,
        &command.work_id,
        &a.expected_work_version,
        &a.execution_id,
    )
    .await?;
    // An intent already exposed via another path cannot become a new dispatch.
    let exposed: bool = tx.query_one("SELECT
        EXISTS(SELECT 1 FROM awr_team.outbox WHERE tenant_id=$1 AND project_id=$2 AND aggregate_id=$3) OR
        EXISTS(SELECT 1 FROM awr_team.execution_receipts WHERE tenant_id=$1 AND project_id=$2 AND execution_id=$3)",
        &[&tenant,&project,&a.execution_id]).await?.get(0);
    if exposed {
        return Err(PgError::RecoveryBlocked);
    }
    // Cross-stream receipts need explicit export/adoption. A grant to both
    // streams does not implicitly create such a delivery contract.
    for upstream in &contract.required_dependencies {
        let row = tx.query_opt("SELECT workstream_id FROM awr_team.workstream_snapshot_ownership
            WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main' AND work_id=$4",
            &[&tenant,&project,&auth.snapshot,upstream]).await?;
        if row.is_none_or(|r| r.get::<_, String>(0) != command.workstream_id.to_string()) {
            return Err(PgError::BindingInvalid);
        }
    }
    let (covered, dependencies) = crate::review::required_dependencies_covered(
        tx,
        tenant,
        project,
        &command.work_id,
        "main",
        &json!(contract),
    )
    .await?;
    if !covered {
        return Err(PgError::BindingInvalid);
    }
    let paths: Vec<String> = serde_json::from_value(r.get("declared_scope_json"))
        .map_err(|_| PgError::SourceDivergence)?;
    if paths.len() > 128 {
        return Err(PgError::SourceDivergence);
    }
    require_paths(contract, &paths)?;
    // Existing project locking serializes check+reserve+start. Prefixes are
    // conservative within this project's lexical namespace, not OS locks.
    let existing = tx
        .query(
            "SELECT resource_kind,canonical_key FROM awr_team.resource_reservations
        WHERE tenant_id=$1 AND project_id=$2 AND state IN ('reserved','unknown')",
            &[&tenant, &project],
        )
        .await?;
    if paths.iter().any(|p| {
        existing.iter().any(|r| {
            crate::graph::paths_conflict(
                "prefix",
                p,
                &r.get::<_, String>(0),
                &r.get::<_, String>(1),
            )
        })
    }) {
        return Err(PgError::ResourceConflict);
    }
    let mut resources = vec![];
    for path in &paths {
        let id = crate::tx::new_id();
        tx.execute("INSERT INTO awr_team.resource_reservations(tenant_id,project_id,id,work_id,resource_kind,canonical_key,state,execution_id)
            VALUES($1,$2,$3,$4,'prefix',$5,'reserved',$6)", &[&tenant,&project,&id,&command.work_id,path,&a.execution_id]).await?;
        resources.push(json!({"reservation_id":id,"kind":"prefix","key":path}));
    }
    // Time advances during dependency/resource checks even while rows are locked.
    claims::require_live(
        tx,
        tenant,
        project,
        auth,
        command,
        ownership,
        &a.session_id,
        &a.claim_id,
        &a.expected_fence,
        &a.expected_lease_version,
    )
    .await?;
    let attestation_grant = auth
        .execution_access
        .get(&command.workstream_id)
        .filter(|a| a.attest)
        .map(|_| auth.grant_versions[&command.workstream_id]);
    tx.execute(
        "UPDATE awr_team.executions SET state='running',execution_version=execution_version+1,attestation_grant_version=$4
        WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
        &[&tenant, &project, &a.execution_id, &attestation_grant],
    )
    .await?;
    let work_version = advance_work(tx, tenant, project, &command.work_id).await?;
    let remaining: i64 = tx
        .query_one(
            "SELECT floor(extract(epoch FROM (expires_at-clock_timestamp()))*1000)::bigint
        FROM awr_team.claims WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant, &project, &a.claim_id],
        )
        .await?
        .get(0);
    if remaining <= 0 {
        return Err(PgError::PreconditionsChanged);
    }
    Ok(
        json!({"execution_id":a.execution_id,"execution_version":(r.get::<_,i64>("execution_version")+1).to_string(),
        "session_id":a.session_id,"claim_id":a.claim_id,"state":"running","fence":a.expected_fence,
        "effect_key":r.get::<_,Option<String>>("effect_key"),"work_version":work_version.to_string(),
        "admission":"granted_at_commit","execution_mode":a.execution_mode,"dispatched":false,
        "input_digest":r.get::<_,Option<String>>("input_digest"),"declared_scope":paths,
        "lease_remaining_ms":remaining.to_string(),
        "fencing_class":"uncontrolled","exactly_once_supported":false,"scope_validation":"lexical_contract_only",
        "dependency_receipts":dependencies,"resources":resources,
        "result_authority":if attestation_grant.is_some() {"trusted_executor"} else {"caller_asserted"},
        "next_action":"Execute once under the current lease; report observations. A replay or unknown response never authorizes another start."}),
    )
}

pub(super) async fn report(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    command: &WorkstreamCommand,
    ownership: i64,
    a: Report,
) -> PgResult<Value> {
    let r = owned_execution(
        tx,
        tenant,
        project,
        auth,
        command,
        ownership,
        &a.session_id,
        &a.execution_id,
        &a.expected_execution_version,
    )
    .await?;
    if !matches!(r.get::<_, String>("state").as_str(), "running" | "unknown") {
        return Err(PgError::PreconditionsChanged);
    }
    let paths: Vec<String> = serde_json::from_value(r.get("declared_scope_json"))
        .map_err(|_| PgError::SourceDivergence)?;
    let exceeded = a.observed_paths.iter().any(|p| {
        !paths
            .iter()
            .any(|s| canonical_path(s) && crate::graph::path_within_scope(s, p))
    });
    let id = crate::tx::new_id();
    let payload = json!({"outcome":a.outcome,"output_digest":a.output_digest,"observed_paths":a.observed_paths,
        "note":a.note,"client_id":auth.client_id,"session_id":a.session_id,
        "workstream_id":command.workstream_id,"ownership_version":ownership.to_string(),
        "execution_version":a.expected_execution_version,"coordinator_epoch":auth.epoch,
        "contract_hash":r.get::<_,String>("contract_hash"),"scope_violation":exceeded});
    let hash = awr_team::request_hash(&payload).map_err(|_| invalid())?;
    tx.execute("INSERT INTO awr_team.execution_receipts(tenant_id,project_id,id,execution_id,reporter_actor_id,receipt_kind,digest,payload_json)
        VALUES($1,$2,$3,$4,$5,'caller_asserted',$6,$7)",
        &[&tenant,&project,&id,&a.execution_id,&auth.actor_id,&hash,&payload]).await?;
    // Even a claimed success/stop is unverified. Persist its original facts,
    // including scope violations, without releasing effects or accepting work.
    tx.execute(
        "UPDATE awr_team.executions SET state='unknown',execution_version=execution_version+1,
        unknown_reason='caller_report_requires_reconciliation'
        WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
        &[&tenant, &project, &a.execution_id],
    )
    .await?;
    tx.execute(
        "UPDATE awr_team.work_runtime SET recovery_blocked=true
        WHERE tenant_id=$1 AND project_id=$2 AND scope_id='main' AND work_id=$3",
        &[&tenant, &project, &command.work_id],
    )
    .await?;
    tx.execute(
        "UPDATE awr_team.resource_reservations SET state='unknown'
        WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='reserved'",
        &[&tenant, &project, &command.work_id],
    )
    .await?;
    let work_version = advance_work(tx, tenant, project, &command.work_id).await?;
    Ok(
        json!({"execution_id":a.execution_id,"execution_version":(r.get::<_,i64>("execution_version")+1).to_string(),
        "session_id":a.session_id,"state":"unknown","receipt_id":id,"receipt_kind":"caller_asserted",
        "reported_outcome":a.outcome,"scope_violation":exceeded,"recovery_blocked":true,
        "work_version":work_version.to_string(),"work_completed":false,"resource_release_performed":false,
        "reconciliation_supported":true,
        "next_action":"Have an authorized recovery operator inspect the receipt and reconcile actual effects. Do not retry or complete while recovery is blocked."}),
    )
}
