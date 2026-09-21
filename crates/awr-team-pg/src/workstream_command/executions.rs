//! Execution intents and caller-managed admission under live authority.
//! Preparation never dispatches. Admission is not physical effect confinement.
mod lifecycle;
use super::*;
use tokio_postgres::Row;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prepare {
    session_id: String,
    expected_session_version: String,
    claim_id: String,
    expected_fence: String,
    expected_lease_version: String,
    expected_work_version: String,
    input_digest: String,
    declared_scope: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cancel {
    session_id: String,
    expected_session_version: String,
    execution_id: String,
    expected_execution_version: String,
}
pub(super) enum Action {
    Prepare(Prepare),
    Cancel(Cancel),
    Start(lifecycle::Start),
    Report(lifecycle::Report),
}
impl Action {
    pub(super) fn parse(op: &str, args: Value) -> PgResult<Self> {
        let action = match op {
            "execution.start" => Self::Start(lifecycle::Start::parse(args)?),
            "execution.report" => Self::Report(lifecycle::Report::parse(args)?),
            "execution.prepare" => {
                let a: Prepare = serde_json::from_value(args).map_err(|_| invalid())?;
                if !identity(&a.claim_id)
                    || !digest(&a.input_digest)
                    || version(&a.expected_fence)? == 0
                    || version(&a.expected_lease_version)? == 0
                    || version(&a.expected_work_version)? == 0
                    || a.declared_scope.len() > 128
                    || a.declared_scope.iter().any(|p| !canonical_path(p))
                {
                    return Err(invalid());
                }
                let mut unique = a.declared_scope.clone();
                unique.sort();
                unique.dedup();
                if unique.len() != a.declared_scope.len() {
                    return Err(invalid());
                }
                Self::Prepare(a)
            }
            "execution.cancel" => {
                let a: Cancel = serde_json::from_value(args).map_err(|_| invalid())?;
                if !identity(&a.execution_id) || version(&a.expected_execution_version)? == 0 {
                    return Err(invalid());
                }
                Self::Cancel(a)
            }
            _ => return Err(invalid()),
        };
        let (id, v) = action.session();
        if !identity(id) || version(v)? == 0 {
            return Err(invalid());
        }
        Ok(action)
    }
    fn session(&self) -> (&str, &str) {
        match self {
            Self::Prepare(a) => (&a.session_id, &a.expected_session_version),
            Self::Cancel(a) => (&a.session_id, &a.expected_session_version),
            Self::Start(a) => (&a.session_id, &a.expected_session_version),
            Self::Report(a) => (&a.session_id, &a.expected_session_version),
        }
    }
    pub(super) fn requires_active_stream(&self) -> bool {
        matches!(self, Self::Prepare(_) | Self::Start(_))
    }
}

// The remote coordinator can validate portable lexical scope, not inspect the
// executor's filesystem. Reject aliases instead of claiming physical confinement.
fn canonical_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 4096
        && !p.chars().any(char::is_control)
        && !p.contains(['\\', ':'])
        && p.split('/').all(|s| !matches!(s, "" | "." | ".."))
}

pub(super) async fn apply(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    command: &WorkstreamCommand,
    ownership: i64,
    contract: &awr_team::WorkContract,
    action: Action,
) -> PgResult<Applied> {
    let (sid, version) = action.session();
    session(tx, tenant, project, auth, command, sid, version, ownership).await?;
    let data = match action {
        Action::Prepare(a) => {
            prepare(tx, tenant, project, auth, command, ownership, contract, a).await?
        }
        Action::Cancel(a) => cancel(tx, tenant, project, auth, command, ownership, a).await?,
        Action::Start(a) => {
            lifecycle::start(tx, tenant, project, auth, command, ownership, contract, a).await?
        }
        Action::Report(a) => {
            lifecycle::report(tx, tenant, project, auth, command, ownership, a).await?
        }
    };
    Ok(Applied {
        data,
        preceding_events: vec![],
    })
}

async fn prepare(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    command: &WorkstreamCommand,
    ownership: i64,
    contract: &awr_team::WorkContract,
    a: Prepare,
) -> PgResult<Value> {
    require_enabled(tx, tenant, project, auth, command).await?;
    let fence = claims::require_live(
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
        "",
    )
    .await?;
    require_paths(contract, &a.declared_scope)?;
    let id = crate::tx::new_id();
    let declared = json!(a.declared_scope);
    tx.execute("INSERT INTO awr_team.executions(tenant_id,project_id,id,work_id,session_id,claim_id,fence,
        contract_hash,input_digest,executor_actor_id,state,effect_key,fencing_class,declared_scope_json,
        scope_id,coordinator_epoch,workstream_id,ownership_version,executor_client_id)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'prepared',$3,'uncontrolled',$11,'main',$12,$13,$14,$15)",
        &[&tenant,&project,&id,&command.work_id,&a.session_id,&a.claim_id,&fence,
          &command.expected_contract_hash,&a.input_digest,&auth.actor_id,&declared,&auth.epoch,
          &command.workstream_id.to_string(),&ownership,&auth.client_id]).await?;
    // No outbox row: an intent cannot be mistaken for an admitted dispatch.
    let work_version = advance_work(tx, tenant, project, &command.work_id).await?;
    Ok(
        json!({"execution_id":id,"execution_version":"1","session_id":a.session_id,"claim_id":a.claim_id,
        "fence":fence.to_string(),"state":"prepared","effect_key":id,"input_digest":a.input_digest,
        "contract_hash":command.expected_contract_hash,"work_version":work_version.to_string(),
        "dispatched":false,"admission":"not_evaluated","fencing_class":"uncontrolled",
        "exactly_once_supported":false,"scope_validation":"lexical_contract_only"}),
    )
}

async fn require_enabled(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    command: &WorkstreamCommand,
) -> PgResult<()> {
    let enabled: bool = tx.query_one("SELECT c.definition_state='enabled' AND s.status='active'
        FROM awr_team.work_contracts c JOIN awr_team.work_scopes s
          ON s.tenant_id=c.tenant_id AND s.project_id=c.project_id AND s.id=c.scope_id
        WHERE c.tenant_id=$1 AND c.project_id=$2 AND c.snapshot_id=$3 AND c.scope_id='main' AND c.work_id=$4",
        &[&tenant,&project,&auth.snapshot,&command.work_id]).await?.get(0);
    if !enabled {
        return Err(PgError::PreconditionsChanged);
    }
    Ok(())
}

async fn require_ready(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    work: &str,
    expected_version: &str,
    except_execution: &str,
) -> PgResult<()> {
    let r = tx.query_one("SELECT state,work_version,recovery_blocked,selected_completion_id
        FROM awr_team.work_runtime WHERE tenant_id=$1 AND project_id=$2 AND scope_id='main' AND work_id=$3 FOR UPDATE",
        &[&tenant,&project,&work]).await?;
    if r.get::<_, i64>(1) != version(expected_version)?
        || r.get::<_, i64>(1) == i64::MAX
        || matches!(
            r.get::<_, String>(0).as_str(),
            "completed" | "cancelled" | "archived"
        )
        || r.get::<_, Option<String>>(3).is_some()
    {
        return Err(PgError::PreconditionsChanged);
    }
    let unresolved: bool = tx.query_one("SELECT
        EXISTS(SELECT 1 FROM awr_team.executions WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3
            AND id<>$4 AND state NOT IN ('succeeded','failed','cancelled')) OR
        EXISTS(SELECT 1 FROM awr_team.resource_reservations WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='unknown')",
        &[&tenant,&project,&work,&except_execution]).await?.get(0);
    if r.get::<_, bool>(2) || unresolved {
        return Err(PgError::RecoveryBlocked);
    }
    let waiting: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM awr_team.wait_items
        WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='open')",
            &[&tenant, &project, &work],
        )
        .await?
        .get(0);
    if waiting {
        return Err(PgError::WaitOpen);
    }
    Ok(())
}

fn require_paths(contract: &awr_team::WorkContract, paths: &[String]) -> PgResult<()> {
    if paths.iter().any(|p| {
        !canonical_path(p)
            || !contract
                .scope_paths
                .iter()
                .any(|s| canonical_path(s) && crate::graph::path_within_scope(s, p))
    }) {
        return Err(PgError::ScopeExceeded);
    }
    Ok(())
}

async fn cancel(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    command: &WorkstreamCommand,
    ownership: i64,
    a: Cancel,
) -> PgResult<Value> {
    let r = load(tx, tenant, project, &a.execution_id).await?;
    require_binding(
        &r,
        &command.work_id,
        &command.workstream_id.to_string(),
        ownership,
    )?;
    if r.get::<_, Option<String>>("session_id").as_deref() != Some(&a.session_id)
        || r.get::<_, String>("executor_actor_id") != auth.actor_id
        || r.get::<_, Option<String>>("executor_client_id").as_deref() != Some(&auth.client_id)
    {
        return Err(PgError::Forbidden);
    }
    if r.get::<_, Option<String>>("coordinator_epoch").as_deref() != Some(&auth.epoch) {
        return Err(PgError::EpochChanged);
    }
    let ev: i64 = r.get("execution_version");
    if ev != version(&a.expected_execution_version)? || ev == i64::MAX {
        return Err(PgError::PreconditionsChanged);
    }
    let state: String = r.get("state");
    if matches!(state.as_str(), "succeeded" | "failed" | "cancelled") {
        return Err(PgError::PreconditionsChanged);
    }
    // Only an intent never exposed to a dispatcher/executor can be cancelled
    // synchronously. An acknowledged request is not a confirmed external stop.
    let exposed: bool = tx.query_one("SELECT
        EXISTS(SELECT 1 FROM awr_team.outbox WHERE tenant_id=$1 AND project_id=$2 AND aggregate_id=$3) OR
        EXISTS(SELECT 1 FROM awr_team.execution_receipts WHERE tenant_id=$1 AND project_id=$2 AND execution_id=$3)",
        &[&tenant,&project,&a.execution_id]).await?.get(0);
    let stopped = state == "prepared" && !exposed;
    let next = if stopped { "cancelled" } else { &state };
    tx.execute("UPDATE awr_team.executions SET state=$4,cancel_requested=true,execution_version=execution_version+1
        WHERE tenant_id=$1 AND project_id=$2 AND id=$3",&[&tenant,&project,&a.execution_id,&next]).await?;
    let work_version = advance_work(tx, tenant, project, &command.work_id).await?;
    Ok(
        json!({"execution_id":a.execution_id,"execution_version":(ev+1).to_string(),"session_id":a.session_id,
        "state":next,"cancel_requested":true,"stop_confirmed":stopped,
        "work_version":work_version.to_string(),"resource_release_performed":false}),
    )
}

async fn advance_work(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    work: &str,
) -> PgResult<i64> {
    Ok(tx
        .query_opt(
            "UPDATE awr_team.work_runtime SET work_version=work_version+1
        WHERE tenant_id=$1 AND project_id=$2 AND scope_id='main' AND work_id=$3
          AND work_version>=0 AND work_version<9223372036854775807 RETURNING work_version",
            &[&tenant, &project, &work],
        )
        .await?
        .ok_or(PgError::PreconditionsChanged)?
        .get(0))
}

async fn load(tx: &Transaction<'_>, tenant: &str, project: &str, id: &str) -> PgResult<Row> {
    tx.query_opt("SELECT e.*,s.actor_id AS session_actor,s.client_id AS session_client,
        s.work_id AS session_work,s.workstream_id AS session_stream,s.ownership_version AS session_ownership,
        c.work_id AS claim_work,c.session_id AS claim_session,c.actor_id AS claim_actor,
        c.workstream_id AS claim_stream,c.ownership_version AS claim_ownership,c.fence AS claim_fence,
        c.coordinator_epoch AS claim_epoch,
        (c.state='active' AND c.expires_at>clock_timestamp() AND s.state='active' AND w.last_fence=e.fence) AS lease_live,
        w.recovery_blocked
        FROM awr_team.executions e
        JOIN awr_team.sessions s ON s.tenant_id=e.tenant_id AND s.project_id=e.project_id AND s.id=e.session_id AND s.scope_id=e.scope_id
        JOIN awr_team.claims c ON c.tenant_id=e.tenant_id AND c.project_id=e.project_id AND c.id=e.claim_id AND c.scope_id=e.scope_id
        JOIN awr_team.work_runtime w ON w.tenant_id=e.tenant_id AND w.project_id=e.project_id AND w.work_id=e.work_id AND w.scope_id=e.scope_id
        WHERE e.tenant_id=$1 AND e.project_id=$2 AND e.id=$3 AND e.scope_id='main'",
        &[&tenant,&project,&id]).await?.ok_or(PgError::Forbidden)
}

fn require_binding(r: &Row, work: &str, stream: &str, ownership: i64) -> PgResult<()> {
    if r.get::<_, String>("work_id") != work
        || r.get::<_, String>("session_work") != work
        || r.get::<_, String>("claim_work") != work
        || r.get::<_, Option<String>>("session_id").as_deref()
            != Some(r.get::<_, String>("claim_session").as_str())
        || r.get::<_, String>("executor_actor_id") != r.get::<_, String>("session_actor")
        || r.get::<_, String>("executor_actor_id") != r.get::<_, String>("claim_actor")
        || r.get::<_, Option<String>>("executor_client_id").as_deref()
            != Some(r.get::<_, String>("session_client").as_str())
        || ["workstream_id", "session_stream", "claim_stream"]
            .iter()
            .any(|k| r.get::<_, Option<String>>(*k).as_deref() != Some(stream))
        || ["ownership_version", "session_ownership", "claim_ownership"]
            .iter()
            .any(|k| r.get::<_, Option<i64>>(*k) != Some(ownership))
        || r.get::<_, i64>("fence") != r.get::<_, i64>("claim_fence")
        || r.get::<_, Option<String>>("coordinator_epoch").is_none()
        || r.get::<_, Option<String>>("coordinator_epoch")
            != r.get::<_, Option<String>>("claim_epoch")
    {
        return Err(PgError::Forbidden);
    }
    Ok(())
}

pub(crate) async fn inspect(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    work: &str,
    stream: &str,
    ownership: i64,
    id: &str,
    session: Option<&str>,
) -> PgResult<Value> {
    let r = load(tx, tenant, project, id).await?;
    require_binding(&r, work, stream, ownership)?;
    if session.is_some_and(|s| r.get::<_, Option<String>>("session_id").as_deref() != Some(s)) {
        return Err(PgError::Forbidden);
    }
    let epoch_matches =
        r.get::<_, Option<String>>("coordinator_epoch").as_deref() == Some(&auth.epoch);
    let current_hash: String = tx
        .query_one(
            "SELECT contract_hash FROM awr_team.work_contracts
        WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main' AND work_id=$4",
            &[&tenant, &project, &auth.snapshot, &work],
        )
        .await?
        .get(0);
    Ok(
        json!({"execution_id":id,"execution_version":r.get::<_,i64>("execution_version").to_string(),
        "work_id":work,"session_id":r.get::<_,Option<String>>("session_id"),"claim_id":r.get::<_,Option<String>>("claim_id"),
        "fence":r.get::<_,i64>("fence").to_string(),"state":r.get::<_,String>("state"),
        "cancel_requested":r.get::<_,bool>("cancel_requested"),"contract_hash":r.get::<_,String>("contract_hash"),
        "contract_matches_current":r.get::<_,String>("contract_hash")==current_hash,
        "epoch_matches_current":epoch_matches,"lease_live":r.get::<_,bool>("lease_live")&&epoch_matches,
        "owned_by_client":r.get::<_,String>("executor_actor_id")==auth.actor_id
            && r.get::<_,Option<String>>("executor_client_id").as_deref()==Some(&auth.client_id),
        "recovery_blocked":r.get::<_,bool>("recovery_blocked"),
        "execution_authorized":false,"automatic_resume":false}),
    )
}
