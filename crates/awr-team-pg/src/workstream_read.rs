//! Authenticated, scope-limited Team read operations. The transport binds the
//! tenant/project from operator configuration; request bodies contain selectors.
use crate::workstream_auth::{ReaderAuthority, authenticate};
use crate::{PgError, PgPool, PgResult};
use awr_core::{
    Id, WorkstreamAction, WorkstreamSelection, WorkstreamSessionBinding, WorkstreamWorkBinding,
    resolve_workstream,
};
use awr_team::WorkContract;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_postgres::{IsolationLevel, Transaction};

const QUERIES: &[&str] = &[
    "capabilities",
    "workstreams.list",
    "work.list",
    "work.search",
    "work.prepare",
    "events.list",
    "session.inspect",
    "work.recovery",
    "command.inspect",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamQuery {
    pub protocol_version: u32,
    pub op: String,
    pub workstream_id: Option<Id>,
    pub work_id: Option<String>,
    pub session_id: Option<String>,
    pub search: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub max_context_bytes: Option<usize>,
    pub request_id: Option<String>,
}

impl WorkstreamQuery {
    pub fn validate(&self) -> PgResult<()> {
        if self.protocol_version != 1 {
            return Err(PgError::Unsupported(
                "workstream query protocol version".into(),
            ));
        }
        if !QUERIES.contains(&self.op.as_str()) {
            return Err(PgError::Unsupported("workstream query operation".into()));
        }
        for id in [&self.work_id, &self.session_id, &self.request_id]
            .into_iter()
            .flatten()
        {
            if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
                return Err(PgError::Protocol("invalid selector".into()));
            }
        }
        if self.limit.is_some_and(|n| n == 0 || n > 100)
            || self.cursor.as_ref().is_some_and(|s| s.len() > 4096)
            || self.max_context_bytes.is_some_and(|n| n == 0 || n > 262144)
        {
            return Err(PgError::Protocol("query bounds exceeded".into()));
        }
        let paged = matches!(
            self.op.as_str(),
            "workstreams.list" | "work.list" | "work.search" | "events.list"
        );
        if !paged && (self.cursor.is_some() || self.limit.is_some())
            || self.search.is_some() != (self.op == "work.search")
            || self
                .search
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 512 || s.chars().any(char::is_control))
            || self.max_context_bytes.is_some() && self.op != "work.prepare"
            || self.request_id.is_some() != (self.op == "command.inspect")
            || matches!(self.op.as_str(), "capabilities" | "workstreams.list")
                && (self.work_id.is_some()
                    || self.session_id.is_some()
                    || self.workstream_id.is_some())
            || matches!(self.op.as_str(), "work.list" | "work.search")
                && (self.work_id.is_some() || self.session_id.is_some())
            || self.op == "session.inspect" && self.session_id.is_none()
            || matches!(
                self.op.as_str(),
                "work.prepare" | "work.recovery" | "command.inspect"
            ) && self.work_id.is_none()
                && self.session_id.is_none()
        {
            return Err(PgError::Protocol(
                "query fields do not match operation".into(),
            ));
        }
        Ok(())
    }
}

pub struct WorkstreamReadStore {
    pool: std::sync::Arc<PgPool>,
}

impl WorkstreamReadStore {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            pool: std::sync::Arc::new(PgPool::new(url)),
        }
    }
    pub fn from_config(config: tokio_postgres::Config) -> Self {
        Self {
            pool: std::sync::Arc::new(PgPool::from_config(config)),
        }
    }

    /// Use the same pool and authority boundary for durable session commands.
    pub fn commands(&self) -> crate::WorkstreamCommandStore {
        crate::WorkstreamCommandStore::from_pool(self.pool.clone())
    }

    pub async fn query(
        &self,
        tenant: &str,
        project: &str,
        bearer: &str,
        request: WorkstreamQuery,
    ) -> PgResult<Value> {
        request.validate()?;
        let mut client = self.pool.get().await?;
        crate::check_schema(&client).await?;
        let tx = client
            .build_transaction()
            .isolation_level(IsolationLevel::RepeatableRead)
            .start()
            .await?;
        let auth = authenticate(&tx, tenant, project, bearer).await?;
        let result = read(&tx, tenant, project, &auth, &request).await?;
        if serde_json::to_vec(&result)
            .map_err(|_| PgError::SourceDivergence)?
            .len()
            > 1_048_576
        {
            return Err(PgError::ResponseTooLarge);
        }
        // No mutation or reusable authority object escapes this transaction.
        tx.commit().await?;
        Ok(result)
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Cursor {
    binding: String,
    key: String,
    revision: String,
    index: i32,
}

fn hash(value: &Value) -> PgResult<String> {
    awr_team::request_hash(value).map_err(|_| PgError::SourceDivergence)
}

fn cursor(request: &WorkstreamQuery, binding: &str) -> PgResult<Cursor> {
    let c = match &request.cursor {
        Some(raw) => serde_json::from_str::<Cursor>(raw).map_err(|_| PgError::CursorExpired)?,
        None => Cursor {
            binding: binding.into(),
            revision: "0".into(),
            index: -1,
            ..Default::default()
        },
    };
    if c.binding != binding || c.revision.parse::<i64>().is_err() {
        return Err(PgError::CursorExpired);
    }
    Ok(c)
}

fn next_cursor(binding: &str, key: &str, revision: i64, index: i32) -> Value {
    Value::String(
        serde_json::to_string(&Cursor {
            binding: binding.into(),
            key: key.into(),
            revision: revision.to_string(),
            index,
        })
        .expect("cursor serialization"),
    )
}

pub(crate) async fn work_binding(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    work: &str,
) -> PgResult<(WorkstreamWorkBinding, i64)> {
    let row = tx
        .query_opt(
            "SELECT workstream_id,ownership_version FROM awr_team.workstream_snapshot_ownership
        WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND work_id=$4",
            &[&tenant, &project, &auth.snapshot, &work],
        )
        .await?
        .ok_or(PgError::Forbidden)?;
    let id: Id = row
        .get::<_, String>(0)
        .parse()
        .map_err(|_| PgError::SourceDivergence)?;
    auth.access
        .authorize(&auth.catalog, id, WorkstreamAction::Read)
        .map_err(|_| PgError::Forbidden)?;
    Ok((
        WorkstreamWorkBinding {
            project_id: project.into(),
            work_item_id: work.into(),
            workstream_id: id,
        },
        row.get(1),
    ))
}

pub(crate) async fn read(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &ReaderAuthority,
    q: &WorkstreamQuery,
) -> PgResult<Value> {
    let visible: Vec<_> = auth
        .catalog
        .workstreams
        .iter()
        .filter(|s| {
            auth.access
                .authorize(&auth.catalog, s.id, WorkstreamAction::Read)
                .is_ok()
        })
        .collect();
    if visible.is_empty() {
        return Err(PgError::Forbidden);
    }
    if q.op == "capabilities" {
        return Ok(
            json!({"protocol":"awr-team-workstream","protocol_version":1,"queries":QUERIES,
        "commands":crate::workstream_command::COMMANDS,"scope_id":"main","authentication":"bearer_per_request","authorization":"transactional_workstream_grants",
        "command_preconditions":"project_revision_v1","command_status_query":"command.inspect",
        "dependency_exports":false,"execution_admission":false,"artifact_content":false}),
        );
    }
    if q.op == "workstreams.list" {
        let binding = hash(
            &json!({"auth":auth.binding,"snapshot":auth.snapshot,"op":q.op,
            "grants":visible.iter().map(|s|(s.id,s.authority_version,auth.grant_versions[&s.id])).collect::<Vec<_>>()}),
        )?;
        let c = cursor(q, &binding)?;
        let limit = usize::from(q.limit.unwrap_or(50));
        let mut sorted = visible.clone();
        sorted.sort_by_key(|s| s.id);
        let rows: Vec<_> = sorted
            .into_iter()
            .filter(|s| s.id.to_string() > c.key)
            .take(limit + 1)
            .collect();
        let more = rows.len() > limit;
        let items: Vec<_> = rows.into_iter().take(limit).collect();
        let next = if more {
            next_cursor(&binding, &items.last().expect("page").id.to_string(), 0, -1)
        } else {
            Value::Null
        };
        return Ok(json!({"items":items,"total":visible.len(),"next_cursor":next}));
    }
    let mut selection = WorkstreamSelection {
        explicit: q.workstream_id,
        ..Default::default()
    };
    if let Some(work) = &q.work_id {
        selection.work = Some(work_binding(tx, tenant, project, auth, work).await?.0);
    }
    if let Some(session) = &q.session_id {
        let row = tx
            .query_opt(
                "SELECT work_id,workstream_id,ownership_version FROM awr_team.sessions
            WHERE tenant_id=$1 AND project_id=$2 AND id=$3 AND scope_id='main'",
                &[&tenant, &project, &session],
            )
            .await?
            .ok_or(PgError::Forbidden)?;
        let work: String = row.get(0);
        let (binding, generation) = work_binding(tx, tenant, project, auth, &work).await?;
        if row.get::<_, Option<String>>(1) != Some(binding.workstream_id.to_string())
            || row.get::<_, Option<i64>>(2) != Some(generation)
        {
            return Err(PgError::Forbidden);
        }
        selection.session = Some(WorkstreamSessionBinding {
            session_id: session.clone(),
            work: binding,
        });
    }
    let resolved = resolve_workstream(
        &auth.catalog,
        &auth.access,
        &selection,
        WorkstreamAction::Read,
    )?;
    let stream = resolved.workstream_id.to_string();
    let binding = hash(
        &json!({"auth":auth.binding,"snapshot":auth.snapshot,"epoch":auth.epoch,"stream":stream,
        "authority":resolved.authority_version,"grant":auth.grant_versions[&resolved.workstream_id],"op":q.op,
        "work":resolved.work_item_id,"session":q.session_id,"search":q.search}),
    )?;
    let c = cursor(q, &binding)?;
    let limit = i64::from(q.limit.unwrap_or(50));
    let data = match q.op.as_str() {
        "command.inspect" => {
            let work = resolved.work_item_id.as_deref().ok_or(PgError::Forbidden)?;
            let (_, ownership) = work_binding(tx, tenant, project, auth, work).await?;
            crate::workstream_command::inspect(
                tx,
                tenant,
                project,
                auth,
                q.request_id.as_deref().ok_or(PgError::Forbidden)?,
                work,
                &stream,
                ownership,
            )
            .await?
        }
        "work.list" | "work.search" => {
            let term = q.search.clone().unwrap_or_default();
            // Filter before both the page and count. Search has no global corpus
            // or rank, so private stream contents cannot affect visible results.
            let count:i64=tx.query_one("SELECT count(*) FROM awr_team.work_contracts c JOIN awr_team.workstream_snapshot_ownership o
                USING(tenant_id,project_id,snapshot_id,scope_id,work_id)
                WHERE c.tenant_id=$1 AND c.project_id=$2 AND c.snapshot_id=$3 AND o.workstream_id=$4
                AND strpos(lower(c.title),lower($5))>0", &[&tenant,&project,&auth.snapshot,&stream,&term]).await?.get(0);
            let rows=tx.query("SELECT c.work_id,c.title,c.contract_hash FROM awr_team.work_contracts c JOIN awr_team.workstream_snapshot_ownership o
                USING(tenant_id,project_id,snapshot_id,scope_id,work_id)
                WHERE c.tenant_id=$1 AND c.project_id=$2 AND c.snapshot_id=$3 AND o.workstream_id=$4 AND c.work_id>$5
                AND strpos(lower(c.title),lower($6))>0 ORDER BY c.work_id LIMIT $7",
                &[&tenant,&project,&auth.snapshot,&stream,&c.key,&term,&(limit+1)]).await?;
            let more = rows.len() > limit as usize;
            let items:Vec<_>=rows.iter().take(limit as usize).map(|r|json!({"work_id":r.get::<_,String>(0),"title":r.get::<_,String>(1),"contract_hash":r.get::<_,String>(2)})).collect();
            let next = if more {
                next_cursor(
                    &binding,
                    items.last().unwrap()["work_id"].as_str().unwrap(),
                    0,
                    -1,
                )
            } else {
                Value::Null
            };
            json!({"items":items,"total":count,"next_cursor":next})
        }
        "work.prepare" => {
            let work = resolved
                .work_item_id
                .as_ref()
                .ok_or_else(|| PgError::Protocol("work required".into()))?;
            let row=tx.query_one("SELECT contract_json,contract_hash FROM awr_team.work_contracts
                WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main' AND work_id=$4",
                &[&tenant,&project,&auth.snapshot,&work]).await?;
            let mut contract: WorkContract =
                serde_json::from_value(row.get(0)).map_err(|_| PgError::SourceDivergence)?;
            let contract_hash: String = row.get(1);
            if contract.work_id.as_str() != work
                || contract.hash().map_err(|_| PgError::SourceDivergence)? != contract_hash
            {
                return Err(PgError::SourceDivergence);
            }
            let allowed=tx.query("SELECT work_id FROM awr_team.workstream_snapshot_ownership
                WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND workstream_id=$4 AND work_id=ANY($5)",
                &[&tenant,&project,&auth.snapshot,&stream,&contract.required_dependencies]).await?;
            let ids: std::collections::BTreeSet<String> =
                allowed.iter().map(|r| r.get(0)).collect();
            let missing = contract
                .required_dependencies
                .iter()
                .any(|d| !ids.contains(d));
            contract.required_dependencies.retain(|d| ids.contains(d));
            let mut reasons = Vec::new();
            if missing {
                reasons.push("dependency_export_unavailable");
            }
            if contract.goals.is_empty() {
                reasons.push("missing_goals");
            }
            if contract.hard_rules.is_empty() {
                reasons.push("missing_hard_rules");
            }
            let runtime = tx.query_opt("SELECT state,work_version,last_fence,recovery_blocked,selected_completion_id
                FROM awr_team.work_runtime WHERE tenant_id=$1 AND project_id=$2 AND scope_id='main' AND work_id=$3",
                &[&tenant,&project,&work]).await?.map(|r| json!({"state":r.get::<_,String>(0),
                    "work_version":r.get::<_,i64>(1).to_string(),"last_fence":r.get::<_,i64>(2).to_string(),
                    "recovery_blocked":r.get::<_,bool>(3),"selected_completion_id":r.get::<_,Option<String>>(4)}));
            let (_, ownership) = work_binding(tx, tenant, project, auth, work).await?;
            let mut data = json!({"work_id":work,"contract_hash":contract_hash,"visible_contract":contract,
                "runtime":runtime,"ownership_version":ownership.to_string(),
                "dependency_export_unavailable":missing,"context_complete":reasons.is_empty(),"completeness_reasons":reasons,"execution_admission":"not_evaluated"});
            // Source snapshots and project audit revisions may advance because
            // of unrelated work. Bind actual selected facts, not those cursors.
            let context_hash = hash(&json!({"domain":"awr-team-workstream-context-v1",
                "reader":auth.binding,"epoch":auth.epoch,"project_status":auth.project_status,
                "workstream":auth.catalog.get(resolved.workstream_id)?,
                "grant_version":auth.grant_versions[&resolved.workstream_id],"data":data}))?;
            data["context_hash"] = json!(context_hash);
            data["context_hash_protocol"] = json!("awr-team-workstream-context-v1");
            if serde_json::to_vec(&data)
                .map_err(|_| PgError::SourceDivergence)?
                .len()
                > q.max_context_bytes.unwrap_or(65536)
            {
                return Err(PgError::ContextIncomplete);
            }
            data
        }
        "events.list" => {
            let after: i64 = c.revision.parse().map_err(|_| PgError::CursorExpired)?;
            let work = resolved.work_item_id.as_deref();
            let rows=tx.query("SELECT e.id,e.project_revision,e.event_index,e.event_type,e.work_id FROM awr_team.events e
                WHERE e.tenant_id=$1 AND e.project_id=$2 AND e.workstream_id=$3
                AND ($4::text IS NULL OR e.work_id=$4) AND (e.project_revision,e.event_index)>($5,$6)
                AND (e.work_id IS NULL OR EXISTS(SELECT 1 FROM awr_team.workstream_snapshot_ownership o
                    WHERE o.tenant_id=e.tenant_id AND o.project_id=e.project_id AND o.snapshot_id=$8
                    AND o.work_id=e.work_id AND o.workstream_id=e.workstream_id))
                ORDER BY e.project_revision,e.event_index LIMIT $7", &[&tenant,&project,&stream,&work,&after,&c.index,&(limit+1),&auth.snapshot]).await?;
            let more = rows.len() > limit as usize;
            let items:Vec<_>=rows.iter().take(limit as usize).map(|r|json!({"id":r.get::<_,String>(0),"project_revision":r.get::<_,i64>(1).to_string(),
                "event_index":r.get::<_,i32>(2),"event_type":r.get::<_,String>(3),"work_id":r.get::<_,Option<String>>(4)})).collect();
            let next = if more {
                let r = &rows[limit as usize - 1];
                next_cursor(&binding, "", r.get(1), r.get(2))
            } else {
                Value::Null
            };
            json!({"items":items,"next_cursor":next,"payloads_included":false})
        }
        "session.inspect" | "work.recovery" => {
            let work = resolved.work_item_id.as_ref().ok_or(PgError::Forbidden)?;
            let session = q.session_id.as_deref();
            let current_contract: String = tx.query_one("SELECT contract_hash FROM awr_team.work_contracts
                WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main' AND work_id=$4",
                &[&tenant,&project,&auth.snapshot,&work]).await?.get(0);
            let rows=tx.query("SELECT s.id,s.state,s.session_version,c.id,c.context_hash,c.contract_hash,c.next_action,c.open_loops_json
                FROM awr_team.sessions s JOIN awr_team.workstream_snapshot_ownership o
                  ON o.tenant_id=s.tenant_id AND o.project_id=s.project_id AND o.work_id=s.work_id AND o.snapshot_id=$3
                  AND o.workstream_id=s.workstream_id AND o.ownership_version=s.ownership_version AND o.scope_id=s.scope_id
                LEFT JOIN awr_team.checkpoints c ON c.tenant_id=s.tenant_id AND c.project_id=s.project_id
                  AND c.session_id=s.id AND c.id=s.latest_checkpoint_id
                WHERE s.tenant_id=$1 AND s.project_id=$2 AND s.workstream_id=$4 AND s.work_id=$5
                  AND ($6::text IS NULL OR s.id=$6) ORDER BY c.created_at DESC NULLS LAST,s.id DESC LIMIT 2",
                &[&tenant,&project,&auth.snapshot,&stream,&work,&session]).await?;
            let items:Vec<_>=rows.iter().map(|r|json!({"session_id":r.get::<_,String>(0),"state":r.get::<_,String>(1),"session_version":r.get::<_,i64>(2).to_string(),
                "checkpoint_id":r.get::<_,Option<String>>(3),"context_hash":r.get::<_,Option<String>>(4),"contract_hash":r.get::<_,Option<String>>(5),
                "contract_matches_current":r.get::<_,Option<String>>(5).map(|hash| hash==current_contract),
                "next_action":r.get::<_,Option<String>>(6),"open_loops":r.get::<_,Option<Value>>(7)})).collect();
            json!({"items":items,"current_contract_hash":current_contract,"automatic_resume":false})
        }
        _ => return Err(PgError::Unsupported("workstream query operation".into())),
    };
    Ok(
        json!({"protocol_version":1,"workstream_id":stream,"authority_version":resolved.authority_version.to_string(),"scope_id":"main","selection_basis":resolved.basis,
        "coordinator_epoch":auth.epoch,"project_status":auth.project_status,"source_snapshot_id":auth.snapshot,"project_revision":auth.revision.to_string(),"data":data}),
    )
}
