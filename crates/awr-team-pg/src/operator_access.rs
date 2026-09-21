//! Explicit schema-owner provisioning. Never reachable through client HTTP/MCP.
//! No raw bearer enters a plan, audit event, receipt or database row.
use crate::{PgError, PgResult};
use awr_core::{Id, WorkstreamCatalog};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tokio_postgres::{Client, Transaction};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessActor {
    pub id: String,
    pub kind: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessGrant {
    pub workstream_id: Id,
    pub authority_version: String,
    pub read: bool,
    pub write: bool,
    pub manage: bool,
    pub attest_execution: bool,
    pub reconcile_execution: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessCredential {
    pub id: String,
    pub secret_hash: String,
    pub expires_at_unix_ms: Option<i64>,
}

/// Exact replacement of one actor/client's project grants. Other clients' grants
/// are retained. Membership is actor/project-wide; credentials are tenant-wide.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessPlan {
    pub protocol_version: u32,
    pub tenant_id: String,
    pub project_id: String,
    pub actor: AccessActor,
    pub client_id: String,
    pub role: String,
    pub grants: Vec<AccessGrant>,
    pub credential: Option<AccessCredential>,
    pub revoke_credentials: Vec<String>,
}

fn invalid() -> PgError {
    PgError::Protocol("invalid operator access plan".into())
}
fn identity(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn credential_id(s: &str) -> bool {
    identity(s)
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn version(s: &str) -> PgResult<i64> {
    if s.starts_with('0') || !s.bytes().all(|c| c.is_ascii_digit()) {
        return Err(invalid());
    }
    s.parse::<i64>().ok().filter(|n| *n > 0).ok_or_else(invalid)
}
fn hash(v: &Value) -> PgResult<String> {
    awr_team::request_hash(v).map_err(|_| invalid())
}
impl AccessPlan {
    fn validate(&self) -> PgResult<()> {
        if self.protocol_version != 1
            || ![
                &self.tenant_id,
                &self.project_id,
                &self.actor.id,
                &self.client_id,
            ]
            .iter()
            .all(|s| identity(s))
            || !matches!(self.actor.kind.as_str(), "human" | "agent" | "system")
            || self.actor.display_name.trim().is_empty()
            || self.actor.display_name.len() > 512
            || self.actor.display_name.chars().any(char::is_control)
            || !matches!(
                self.role.as_str(),
                "admin" | "worker" | "reviewer" | "reader"
            )
            || self.grants.len() > 256
            || self.revoke_credentials.len() > 256
            || serde_json::to_vec(self).map_err(|_| invalid())?.len() > 65536
        {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        for g in &self.grants {
            version(&g.authority_version)?;
            if !seen.insert(g.workstream_id)
                || !g.read
                || (g.write && self.role == "reader")
                || (g.manage && self.role != "admin")
                || (g.attest_execution && (!g.write || self.actor.kind != "system"))
                || (g.reconcile_execution && (!g.write || !g.manage || self.actor.kind == "agent"))
            {
                return Err(invalid());
            }
        }
        let mut seen = BTreeSet::new();
        if self
            .revoke_credentials
            .iter()
            .any(|id| !credential_id(id) || !seen.insert(id))
        {
            return Err(invalid());
        }
        if let Some(c) = &self.credential {
            if !credential_id(&c.id)
                || !c.secret_hash.strip_prefix("sha256:").is_some_and(hex)
                || c.expires_at_unix_ms
                    .is_some_and(|t| !(1..=253402300799999).contains(&t))
                || self.revoke_credentials.contains(&c.id)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

pub struct OperatorAccess;
impl OperatorAccess {
    pub async fn inspect(
        client: &mut Client,
        tenant: &str,
        project: &str,
        actor: &str,
        caller: &str,
    ) -> PgResult<Value> {
        if ![tenant, project, actor, caller].iter().all(|s| identity(s)) {
            return Err(invalid());
        }
        crate::check_schema(client).await?;
        let tx = client
            .build_transaction()
            .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
            .start()
            .await?;
        bind(&tx, tenant, project, false).await?;
        let state = snapshot(&tx, tenant, project, actor, caller).await?;
        let result = json!({"state_digest":hash(&state)?,"state":state});
        tx.commit().await?;
        Ok(result)
    }

    pub async fn preview(client: &mut Client, plan: &AccessPlan) -> PgResult<Value> {
        plan.validate()?;
        crate::check_schema(client).await?;
        let tx = client
            .build_transaction()
            .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
            .start()
            .await?;
        bind(&tx, &plan.tenant_id, &plan.project_id, false).await?;
        let state = snapshot(
            &tx,
            &plan.tenant_id,
            &plan.project_id,
            &plan.actor.id,
            &plan.client_id,
        )
        .await?;
        validate_current(&tx, plan, &state).await?;
        let result = json!({"applied":false,"state_digest":hash(&state)?,"current":state,
            "desired":public_plan(plan),"grant_semantics":"replace_selected_actor_client_project_grants",
            "membership_scope":"all_clients_of_actor_in_project","credential_revocation_scope":"all_projects_in_tenant_using_this_credential"});
        tx.commit().await?;
        Ok(result)
    }

    pub async fn outcome(
        client: &mut Client,
        tenant: &str,
        project: &str,
        request: &str,
    ) -> PgResult<Value> {
        if ![tenant, project, request].iter().all(|s| identity(s)) {
            return Err(invalid());
        }
        crate::check_schema(client).await?;
        let tx = client
            .build_transaction()
            .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
            .start()
            .await?;
        bind(&tx, tenant, project, false).await?;
        let row = tx.query_opt("SELECT result_json FROM awr_team.access_changes WHERE tenant_id=$1 AND project_id=$2 AND request_id=$3",
            &[&tenant,&project,&request]).await?;
        let result = match row {
            Some(r) => json!({"outcome":"committed","receipt":r.get::<_,Value>(0)}),
            None => json!({"outcome":"unknown"}),
        };
        tx.commit().await?;
        Ok(result)
    }

    pub async fn apply(
        client: &mut Client,
        plan: &AccessPlan,
        request: &str,
        expected_state: &str,
    ) -> PgResult<Value> {
        plan.validate()?;
        if !identity(request) || !hex(expected_state) {
            return Err(invalid());
        }
        let intent_hash = hash(
            &json!({"protocol":"awr-operator-access-v1","plan":plan,"expected_state":expected_state}),
        )?;
        crate::check_schema(client).await?;
        let tx = client.transaction().await?;
        let operator = bind(&tx, &plan.tenant_id, &plan.project_id, true).await?;
        if let Some(r) = tx.query_opt("SELECT request_hash,result_json FROM awr_team.access_changes WHERE tenant_id=$1 AND project_id=$2 AND request_id=$3",
            &[&plan.tenant_id,&plan.project_id,&request]).await? {
            if r.get::<_,String>(0) != intent_hash { return Err(PgError::IdempotencyConflict); }
            let receipt: Value = r.get(1);
            tx.commit().await?;
            return Ok(json!({"replayed":true,"receipt":receipt}));
        }
        // One actor's credentials and membership can be shared by projects.
        // Serialize its operator changes before acquiring any shared row locks.
        tx.query_opt(
            "SELECT id FROM awr_team.actors WHERE tenant_id=$1 AND id=$2 FOR UPDATE",
            &[&plan.tenant_id, &plan.actor.id],
        )
        .await?;
        let before = snapshot(
            &tx,
            &plan.tenant_id,
            &plan.project_id,
            &plan.actor.id,
            &plan.client_id,
        )
        .await?;
        if hash(&before)? != expected_state {
            return Err(PgError::PreconditionsChanged);
        }
        validate_current(&tx, plan, &before).await?;
        apply_policy(&tx, plan).await?;
        let after = snapshot(
            &tx,
            &plan.tenant_id,
            &plan.project_id,
            &plan.actor.id,
            &plan.client_id,
        )
        .await?;
        let revision: i64 = tx.query_opt("UPDATE awr_team.projects SET project_revision=project_revision+1 WHERE tenant_id=$1 AND id=$2
            AND project_revision<9223372036854775807 RETURNING project_revision", &[&plan.tenant_id,&plan.project_id]).await?
            .ok_or(PgError::PreconditionsChanged)?.get(0);
        let receipt = json!({"protocol":"awr-operator-access-v1","request_id":request,"request_hash":intent_hash,
            "operator_role":operator,"tenant_id":plan.tenant_id,"project_id":plan.project_id,
            "actor_id":plan.actor.id,"client_id":plan.client_id,"before_digest":expected_state,"after_digest":hash(&after)?,
            "project_revision":revision.to_string(),"desired":public_plan(plan),
            "previous_policy":policy(&before),"current_policy":policy(&after),
            "state_basis":"at_commit","execution_authorized":false});
        tx.execute("INSERT INTO awr_team.access_changes(tenant_id,project_id,request_id,request_hash,operator_role,result_json)
            VALUES($1,$2,$3,$4,$5,$6)",&[&plan.tenant_id,&plan.project_id,&request,&intent_hash,&operator,&receipt]).await?;
        // No grant contents, credential hashes or raw tokens in client-visible events.
        let event = json!({"operator_role":operator,"actor_id":plan.actor.id,"client_id":plan.client_id,"request_id":request});
        tx.execute("INSERT INTO awr_team.events(tenant_id,project_id,id,project_revision,event_index,event_type,actor_id,payload_json)
            VALUES($1,$2,$3,$4,0,'access.changed',$5,$6)",&[&plan.tenant_id,&plan.project_id,&crate::tx::new_id(),&revision,&plan.actor.id,&event]).await?;
        tx.commit().await?;
        Ok(json!({"replayed":false,"receipt":receipt}))
    }
}

async fn bind(tx: &Transaction<'_>, tenant: &str, project: &str, write: bool) -> PgResult<String> {
    let role = tx
        .query_one(
            "SELECT current_user::text,pg_has_role(current_user,n.nspowner,'USAGE')
        FROM pg_namespace n WHERE n.nspname='awr_team'",
            &[],
        )
        .await?;
    if !role.get::<_, bool>(1) {
        return Err(PgError::Forbidden);
    }
    crate::tx::bind_workstream_scope(tx, tenant, project).await?;
    let mode = tx.query_opt("SELECT enabled FROM awr_team.workstream_modes WHERE tenant_id=$1 AND project_id=$2 FOR SHARE", &[&tenant,&project]).await?
        .ok_or(PgError::Forbidden)?;
    if !mode.get::<_, bool>(0) {
        return Err(PgError::Unsupported(
            "operator access requires enabled workstreams".into(),
        ));
    }
    let sql = if write {
        "SELECT id FROM awr_team.projects WHERE tenant_id=$1 AND id=$2 FOR UPDATE"
    } else {
        "SELECT id FROM awr_team.projects WHERE tenant_id=$1 AND id=$2 FOR SHARE"
    };
    tx.query_opt(sql, &[&tenant, &project])
        .await?
        .ok_or(PgError::Forbidden)?;
    Ok(role.get(0))
}

async fn snapshot(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    actor: &str,
    caller: &str,
) -> PgResult<Value> {
    let p=tx.query_one("SELECT active_snapshot_id,coordinator_epoch FROM awr_team.projects WHERE tenant_id=$1 AND id=$2",&[&tenant,&project]).await?;
    let catalog=tx.query_one("SELECT catalog_json FROM awr_team.workstream_catalogs WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3",
        &[&tenant,&project,&p.get::<_,Option<String>>(0)]).await?.get::<_,Value>(0);
    let tenant_status: String = tx
        .query_one(
            "SELECT status FROM awr_team.tenants WHERE id=$1 FOR SHARE",
            &[&tenant],
        )
        .await?
        .get(0);
    let a=tx.query_opt("SELECT kind,display_name,status FROM awr_team.actors WHERE tenant_id=$1 AND id=$2 FOR SHARE",&[&tenant,&actor]).await?
        .map(|r|json!({"kind":r.get::<_,String>(0),"display_name":r.get::<_,String>(1),"status":r.get::<_,String>(2)}));
    let member=tx.query_opt("SELECT role,membership_version FROM awr_team.project_memberships WHERE tenant_id=$1 AND project_id=$2 AND actor_id=$3 FOR SHARE",
        &[&tenant,&project,&actor]).await?.map(|r|json!({"role":r.get::<_,String>(0),"version":r.get::<_,i64>(1).to_string()}));
    let grants=tx.query("SELECT workstream_id,authority_version,can_read,can_write,can_manage,can_attest_execution,can_reconcile_execution,active,grant_version
        FROM awr_team.workstream_grants WHERE tenant_id=$1 AND project_id=$2 AND actor_id=$3 AND client_id=$4 ORDER BY workstream_id FOR SHARE",
        &[&tenant,&project,&actor,&caller]).await?.iter().map(|r|json!({"workstream_id":r.get::<_,String>(0),"authority_version":r.get::<_,i64>(1).to_string(),
        "read":r.get::<_,bool>(2),"write":r.get::<_,bool>(3),"manage":r.get::<_,bool>(4),"attest_execution":r.get::<_,bool>(5),"reconcile_execution":r.get::<_,bool>(6),
        "active":r.get::<_,bool>(7),"version":r.get::<_,i64>(8).to_string()})).collect::<Vec<_>>();
    let credentials=tx.query("SELECT id,(extract(epoch FROM expires_at)*1000)::bigint,(extract(epoch FROM revoked_at)*1000)::bigint
        FROM awr_team.credentials WHERE tenant_id=$1 AND actor_id=$2 AND client_id=$3 ORDER BY id FOR SHARE",&[&tenant,&actor,&caller]).await?.iter()
        .map(|r|json!({"id":r.get::<_,String>(0),"expires_at_unix_ms":r.get::<_,Option<i64>>(1),"revoked_at_unix_ms":r.get::<_,Option<i64>>(2)})).collect::<Vec<_>>();
    Ok(
        json!({"tenant_id":tenant,"project_id":project,"actor_id":actor,"client_id":caller,"tenant_status":tenant_status,
        "source_snapshot_id":p.get::<_,Option<String>>(0),"coordinator_epoch":p.get::<_,String>(1),"catalog":catalog,
        "actor":a,"membership":member,"grants":grants,"credentials":credentials}),
    )
}

async fn validate_current(tx: &Transaction<'_>, plan: &AccessPlan, state: &Value) -> PgResult<()> {
    if state["tenant_status"] != "active" {
        return Err(PgError::Forbidden);
    }
    let actor = &state["actor"];
    if !actor.is_null()
        && (actor["kind"] != plan.actor.kind
            || actor["display_name"] != plan.actor.display_name
            || actor["status"] != "active")
    {
        return Err(PgError::PreconditionsChanged);
    }
    let catalog: WorkstreamCatalog =
        serde_json::from_value(state["catalog"].clone()).map_err(|_| PgError::SourceDivergence)?;
    if catalog.project_id != plan.project_id {
        return Err(PgError::SourceDivergence);
    }
    for g in &plan.grants {
        if catalog.get(g.workstream_id)?.authority_version != version(&g.authority_version)? as u64
        {
            return Err(PgError::PreconditionsChanged);
        }
    }
    if let Some(c) = &plan.credential {
        let prior=tx.query_opt("SELECT actor_id,client_id,secret_hash,(extract(epoch FROM expires_at)*1000)::bigint,revoked_at IS NOT NULL
            FROM awr_team.credentials WHERE tenant_id=$1 AND id=$2 FOR SHARE",&[&plan.tenant_id,&c.id]).await?;
        if let Some(r) = prior {
            if r.get::<_, String>(0) != plan.actor.id
                || r.get::<_, String>(1) != plan.client_id
                || r.get::<_, String>(2) != c.secret_hash
                || r.get::<_, Option<i64>>(3) != c.expires_at_unix_ms
                || r.get::<_, bool>(4)
            {
                return Err(PgError::PreconditionsChanged);
            }
        }
        if let Some(expiry) = c.expires_at_unix_ms {
            let valid: bool = tx
                .query_one(
                    "SELECT to_timestamp($1::bigint::double precision/1000)>clock_timestamp()",
                    &[&expiry],
                )
                .await?
                .get(0);
            if !valid {
                return Err(PgError::PreconditionsChanged);
            }
        }
    }
    for id in &plan.revoke_credentials {
        if !state["credentials"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == *id)
        {
            return Err(PgError::Forbidden);
        }
    }
    Ok(())
}

fn public_plan(plan: &AccessPlan) -> Value {
    let mut v = serde_json::to_value(plan).expect("serializable access plan");
    if let Some(c) = v.get_mut("credential").and_then(Value::as_object_mut) {
        c.remove("secret_hash");
    }
    v
}

fn policy(state: &Value) -> Value {
    json!({"actor":state["actor"],"membership":state["membership"],
        "grants":state["grants"],"credentials":state["credentials"]})
}

async fn apply_policy(tx: &Transaction<'_>, p: &AccessPlan) -> PgResult<()> {
    tx.execute("INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status) VALUES($1,$2,$3,$4,'active') ON CONFLICT DO NOTHING",
        &[&p.tenant_id,&p.actor.id,&p.actor.kind,&p.actor.display_name]).await?;
    tx.execute("INSERT INTO awr_team.project_memberships(tenant_id,project_id,actor_id,role) VALUES($1,$2,$3,$4)
        ON CONFLICT(tenant_id,project_id,actor_id) DO UPDATE SET role=EXCLUDED.role,membership_version=awr_team.project_memberships.membership_version+1
        WHERE awr_team.project_memberships.role<>EXCLUDED.role",&[&p.tenant_id,&p.project_id,&p.actor.id,&p.role]).await?;
    let ids = p
        .grants
        .iter()
        .map(|g| g.workstream_id.to_string())
        .collect::<Vec<_>>();
    tx.execute("UPDATE awr_team.workstream_grants SET active=false,grant_version=grant_version+1
        WHERE tenant_id=$1 AND project_id=$2 AND actor_id=$3 AND client_id=$4 AND active AND NOT(workstream_id=ANY($5))",
        &[&p.tenant_id,&p.project_id,&p.actor.id,&p.client_id,&ids]).await?;
    for g in &p.grants {
        tx.execute("INSERT INTO awr_team.workstream_grants(tenant_id,project_id,actor_id,client_id,workstream_id,authority_version,
            can_read,can_write,can_manage,can_attest_execution,can_reconcile_execution) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
            ON CONFLICT(tenant_id,project_id,actor_id,client_id,workstream_id) DO UPDATE SET authority_version=EXCLUDED.authority_version,
            can_read=EXCLUDED.can_read,can_write=EXCLUDED.can_write,can_manage=EXCLUDED.can_manage,can_attest_execution=EXCLUDED.can_attest_execution,
            can_reconcile_execution=EXCLUDED.can_reconcile_execution,active=true,grant_version=awr_team.workstream_grants.grant_version+1
            WHERE (awr_team.workstream_grants.authority_version,awr_team.workstream_grants.can_read,awr_team.workstream_grants.can_write,
                awr_team.workstream_grants.can_manage,awr_team.workstream_grants.can_attest_execution,awr_team.workstream_grants.can_reconcile_execution,awr_team.workstream_grants.active)
            IS DISTINCT FROM (EXCLUDED.authority_version,EXCLUDED.can_read,EXCLUDED.can_write,EXCLUDED.can_manage,EXCLUDED.can_attest_execution,EXCLUDED.can_reconcile_execution,true)",
            &[&p.tenant_id,&p.project_id,&p.actor.id,&p.client_id,&g.workstream_id.to_string(),&version(&g.authority_version)?,
              &g.read,&g.write,&g.manage,&g.attest_execution,&g.reconcile_execution]).await?;
    }
    if let Some(c) = &p.credential {
        tx.execute("INSERT INTO awr_team.credentials(tenant_id,id,actor_id,client_id,secret_hash,expires_at)
            VALUES($1,$2,$3,$4,$5,to_timestamp($6::bigint::double precision/1000)) ON CONFLICT DO NOTHING",
            &[&p.tenant_id,&c.id,&p.actor.id,&p.client_id,&c.secret_hash,&c.expires_at_unix_ms]).await?;
    }
    for id in &p.revoke_credentials {
        tx.execute("UPDATE awr_team.credentials SET revoked_at=clock_timestamp() WHERE tenant_id=$1 AND id=$2 AND actor_id=$3 AND client_id=$4 AND revoked_at IS NULL",
            &[&p.tenant_id,id,&p.actor.id,&p.client_id]).await?;
    }
    // Recheck concurrent absent-row insertions; never accept someone else's actor
    // identity or a credential collision through ON CONFLICT DO NOTHING.
    let after = snapshot(tx, &p.tenant_id, &p.project_id, &p.actor.id, &p.client_id).await?;
    validate_current(tx, p, &after).await?;
    Ok(())
}
