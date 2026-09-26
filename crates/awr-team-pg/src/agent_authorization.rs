//! Team PG persistence for agent authorizations (WS-016).
use crate::error::{PgError, PgResult};
use crate::tx::{bind_workstream_scope, new_id};
use awr_core::{
    AgentAuthorization, AuthorizationStatus, ClaimEligibilityExplanation, ClaimEvaluationInput,
    DelegateAuthorizationRequest, ExecutionSubjectKind, IssueAuthorizationRequest, PersonId,
    RevokeAuthorizationRequest, apply_delegate, apply_revoke, explain_claim_eligibility,
    require_authorization_project, validate_issue,
};
use serde_json::Value;
use tokio_postgres::Row;

fn map_core(err: awr_core::Error) -> PgError {
    match err {
        awr_core::Error::RuleViolation(_) => PgError::Forbidden,
        awr_core::Error::NotFound(m) => PgError::Protocol(format!("not found: {m}")),
        awr_core::Error::InvalidInput(m) => PgError::Protocol(m),
        other => PgError::Protocol(other.to_string()),
    }
}

fn subject_kind_str(kind: ExecutionSubjectKind) -> &'static str {
    match kind {
        ExecutionSubjectKind::Person => "person",
        ExecutionSubjectKind::Agent => "agent",
        ExecutionSubjectKind::PlatformService => "platform_service",
    }
}

fn status_str(status: AuthorizationStatus) -> &'static str {
    match status {
        AuthorizationStatus::Active => "active",
        AuthorizationStatus::Revoked => "revoked",
        AuthorizationStatus::Expired => "expired",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AuthorizationReceipt {
    pub request_key: String,
    pub authorization_id: String,
    pub op: &'static str,
    pub event_id: String,
    pub replayed: bool,
}

pub struct AuthorizationStore {
    pool: crate::PgPool,
}

impl AuthorizationStore {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            pool: crate::PgPool::new(url),
        }
    }

    pub fn from_config(config: tokio_postgres::Config) -> Self {
        Self {
            pool: crate::PgPool::from_config(config),
        }
    }

    async fn connect(&self) -> PgResult<crate::PgClient> {
        self.pool.get().await
    }

    pub async fn get(
        &self,
        tenant: &str,
        project: &str,
        authorization_id: &str,
    ) -> PgResult<Option<AgentAuthorization>> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_workstream_scope(&tx, tenant, project).await?;
        let row = tx
            .query_opt(
                "SELECT body_json FROM awr_team.agent_authorizations
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant, &project, &authorization_id],
            )
            .await?;
        let found = match row {
            None => None,
            Some(row) => decode_body(row)?,
        };
        tx.commit().await?;
        Ok(found)
    }

    pub async fn list(
        &self,
        tenant: &str,
        project: &str,
        responsible_person: Option<&PersonId>,
        subject_id: Option<&str>,
        active_only: bool,
    ) -> PgResult<Vec<AgentAuthorization>> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_workstream_scope(&tx, tenant, project).await?;
        let rows = tx
            .query(
                "SELECT body_json, responsible_person_id, subject_id, status
                 FROM awr_team.agent_authorizations
                 WHERE tenant_id=$1 AND project_id=$2
                 ORDER BY created_at_ms ASC",
                &[&tenant, &project],
            )
            .await?;
        let mut out = Vec::new();
        for row in rows {
            let responsible: String = row.get(1);
            let subject: String = row.get(2);
            let status: String = row.get(3);
            if let Some(person) = responsible_person {
                if responsible != person.as_str() {
                    continue;
                }
            }
            if let Some(sid) = subject_id {
                if subject != sid {
                    continue;
                }
            }
            if active_only && status != "active" {
                continue;
            }
            let body: Value = row.get(0);
            out.push(
                serde_json::from_value(body)
                    .map_err(|e| PgError::Protocol(format!("corrupt agent authorization: {e}")))?,
            );
        }
        tx.commit().await?;
        Ok(out)
    }

    pub async fn issue(
        &self,
        tenant: &str,
        project: &str,
        req: &IssueAuthorizationRequest,
    ) -> PgResult<(AgentAuthorization, AuthorizationReceipt)> {
        validate_issue(req).map_err(map_core)?;
        require_authorization_project(&req.authorization, project).map_err(map_core)?;
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_workstream_scope(&tx, tenant, project).await?;
        lock_project(&tx, tenant, project).await?;
        let result = issue_in_tx(&tx, tenant, project, req).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn revoke(
        &self,
        tenant: &str,
        project: &str,
        req: &RevokeAuthorizationRequest,
    ) -> PgResult<(AgentAuthorization, AuthorizationReceipt)> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_workstream_scope(&tx, tenant, project).await?;
        lock_project(&tx, tenant, project).await?;
        if let Some(receipt) = load_receipt_tx(&tx, tenant, project, &req.request_key).await? {
            if receipt.op != "revoke" || receipt.authorization_id != req.authorization_id {
                return Err(PgError::IdempotencyConflict);
            }
            let auth = load_auth_tx(&tx, tenant, project, &receipt.authorization_id)
                .await?
                .ok_or_else(|| PgError::Protocol("authorization missing for receipt".into()))?;
            if auth.revoked_by.as_ref() != Some(&req.revoked_by)
                || auth.revoked_at_ms != Some(req.revoked_at_ms)
            {
                return Err(PgError::IdempotencyConflict);
            }
            tx.commit().await?;
            return Ok((
                auth,
                AuthorizationReceipt {
                    replayed: true,
                    ..receipt
                },
            ));
        }
        let current = load_auth_tx(&tx, tenant, project, &req.authorization_id)
            .await?
            .ok_or_else(|| PgError::Protocol("authorization not found".into()))?;
        let next = apply_revoke(&current, req).map_err(map_core)?;
        persist_auth_tx(&tx, tenant, project, &next).await?;
        let event_id = new_id();
        save_receipt_tx(
            &tx,
            tenant,
            project,
            &req.request_key,
            &next.id,
            "revoke",
            &event_id,
        )
        .await?;
        tx.commit().await?;
        Ok((
            next,
            AuthorizationReceipt {
                request_key: req.request_key.clone(),
                authorization_id: req.authorization_id.clone(),
                op: "revoke",
                event_id,
                replayed: false,
            },
        ))
    }

    pub async fn delegate(
        &self,
        tenant: &str,
        project: &str,
        req: &DelegateAuthorizationRequest,
        now_ms: i64,
    ) -> PgResult<(AgentAuthorization, AuthorizationReceipt)> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_workstream_scope(&tx, tenant, project).await?;
        lock_project(&tx, tenant, project).await?;
        require_authorization_project(&req.child, project).map_err(map_core)?;
        if let Some(receipt) = load_receipt_tx(&tx, tenant, project, &req.request_key).await? {
            if receipt.op != "delegate" || receipt.authorization_id != req.child.id {
                return Err(PgError::IdempotencyConflict);
            }
            let auth = load_auth_tx(&tx, tenant, project, &receipt.authorization_id)
                .await?
                .ok_or_else(|| PgError::Protocol("authorization missing for receipt".into()))?;
            let mut expected = req.child.clone();
            expected.parent_authorization_id = Some(req.parent_authorization_id.clone());
            if auth != expected {
                return Err(PgError::IdempotencyConflict);
            }
            tx.commit().await?;
            return Ok((
                auth,
                AuthorizationReceipt {
                    replayed: true,
                    ..receipt
                },
            ));
        }
        let parent = load_auth_tx(&tx, tenant, project, &req.parent_authorization_id)
            .await?
            .ok_or_else(|| PgError::Protocol("parent authorization not found".into()))?;
        let child = apply_delegate(&parent, req, now_ms).map_err(map_core)?;
        if load_auth_tx(&tx, tenant, project, &child.id)
            .await?
            .is_some()
        {
            return Err(PgError::Protocol(
                "child authorization id already exists".into(),
            ));
        }
        write_auth_tx(&tx, tenant, project, &child, true).await?;
        let event_id = new_id();
        save_receipt_tx(
            &tx,
            tenant,
            project,
            &req.request_key,
            &child.id,
            "delegate",
            &event_id,
        )
        .await?;
        tx.commit().await?;
        Ok((
            child.clone(),
            AuthorizationReceipt {
                request_key: req.request_key.clone(),
                authorization_id: child.id,
                op: "delegate",
                event_id,
                replayed: false,
            },
        ))
    }

    pub fn explain_claim(
        &self,
        input: &ClaimEvaluationInput<'_>,
    ) -> PgResult<ClaimEligibilityExplanation> {
        explain_claim_eligibility(input).map_err(map_core)
    }
}

/// Shared initial-issue transaction; caller owns the project lock and commit.
pub(crate) async fn issue_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    req: &IssueAuthorizationRequest,
) -> PgResult<(AgentAuthorization, AuthorizationReceipt)> {
    validate_issue(req).map_err(map_core)?;
    require_authorization_project(&req.authorization, project).map_err(map_core)?;
    if let Some(receipt) = load_receipt_tx(tx, tenant, project, &req.request_key).await? {
        if receipt.op != "issue" || receipt.authorization_id != req.authorization.id {
            return Err(PgError::IdempotencyConflict);
        }
        let auth = load_auth_tx(tx, tenant, project, &receipt.authorization_id)
            .await?
            .ok_or_else(|| PgError::Protocol("authorization missing for receipt".into()))?;
        if auth != req.authorization {
            return Err(PgError::IdempotencyConflict);
        }
        return Ok((
            auth,
            AuthorizationReceipt {
                replayed: true,
                ..receipt
            },
        ));
    }
    if load_auth_tx(tx, tenant, project, &req.authorization.id)
        .await?
        .is_some()
    {
        return Err(PgError::Protocol(
            "authorization id already exists; use a new id or revoke the existing grant".into(),
        ));
    }
    write_auth_tx(tx, tenant, project, &req.authorization, true).await?;
    let event_id = new_id();
    save_receipt_tx(
        tx,
        tenant,
        project,
        &req.request_key,
        &req.authorization.id,
        "issue",
        &event_id,
    )
    .await?;
    Ok((
        req.authorization.clone(),
        AuthorizationReceipt {
            request_key: req.request_key.clone(),
            authorization_id: req.authorization.id.clone(),
            op: "issue",
            event_id,
            replayed: false,
        },
    ))
}

pub(crate) async fn lock_project(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
) -> PgResult<()> {
    tx.query_opt(
        "SELECT id FROM awr_team.projects WHERE tenant_id=$1 AND id=$2 FOR UPDATE",
        &[&tenant, &project],
    )
    .await?
    .ok_or(PgError::Forbidden)?;
    Ok(())
}

fn decode_body(row: Row) -> PgResult<Option<AgentAuthorization>> {
    let body: Value = row.get(0);
    Ok(Some(serde_json::from_value(body).map_err(|e| {
        PgError::Protocol(format!("corrupt agent authorization: {e}"))
    })?))
}

async fn load_auth_tx(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    id: &str,
) -> PgResult<Option<AgentAuthorization>> {
    let row = tx
        .query_opt(
            "SELECT body_json FROM awr_team.agent_authorizations
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant, &project, &id],
        )
        .await?;
    match row {
        None => Ok(None),
        Some(row) => decode_body(row),
    }
}

async fn persist_auth_tx(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &AgentAuthorization,
) -> PgResult<()> {
    write_auth_tx(tx, tenant, project, auth, false).await
}

async fn write_auth_tx(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    auth: &AgentAuthorization,
    fresh: bool,
) -> PgResult<()> {
    let body = serde_json::to_value(auth).map_err(|e| PgError::Protocol(e.to_string()))?;
    let sql = "INSERT INTO awr_team.agent_authorizations(
            tenant_id,project_id,id,authorizer_person_id,responsible_person_id,subject_kind,subject_id,
            client_id,session_id,model_id,status,expires_at_ms,revoked_at_ms,revoked_by,
            parent_authorization_id,maintainer_person_id,binding_id,created_at_ms,body_json)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
         ON CONFLICT(tenant_id,project_id,id) DO UPDATE SET
            status=EXCLUDED.status,
            expires_at_ms=EXCLUDED.expires_at_ms,
            revoked_at_ms=EXCLUDED.revoked_at_ms,
            revoked_by=EXCLUDED.revoked_by,
            parent_authorization_id=EXCLUDED.parent_authorization_id,
            body_json=EXCLUDED.body_json";
    let sql = if fresh {
        sql.split(" ON CONFLICT").next().unwrap_or(sql).trim_end()
    } else {
        sql
    };
    tx.execute(
        sql,
        &[
            &tenant,
            &project,
            &auth.id,
            &auth.authorizer_person_id.as_str(),
            &auth.responsible_person_id.as_str(),
            &subject_kind_str(auth.subject_kind),
            &auth.subject_id,
            &auth.client_id,
            &auth.session_id,
            &auth.model_id,
            &status_str(auth.status),
            &auth.expires_at_ms,
            &auth.revoked_at_ms,
            &auth.revoked_by.as_ref().map(|p| p.as_str().to_string()),
            &auth.parent_authorization_id,
            &auth
                .maintainer_person_id
                .as_ref()
                .map(|p| p.as_str().to_string()),
            &auth.binding_id,
            &auth.created_at_ms,
            &body,
        ],
    )
    .await?;
    Ok(())
}

async fn load_receipt_tx(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    request_key: &str,
) -> PgResult<Option<AuthorizationReceipt>> {
    let row = tx
        .query_opt(
            "SELECT authorization_id,op,event_id FROM awr_team.agent_authorization_receipts
             WHERE tenant_id=$1 AND project_id=$2 AND request_key=$3",
            &[&tenant, &project, &request_key],
        )
        .await?;
    Ok(row.map(|r| {
        let op: String = r.get(1);
        AuthorizationReceipt {
            request_key: request_key.into(),
            authorization_id: r.get(0),
            op: match op.as_str() {
                "issue" => "issue",
                "revoke" => "revoke",
                "delegate" => "delegate",
                _ => "unknown",
            },
            event_id: r.get(2),
            replayed: false,
        }
    }))
}

async fn save_receipt_tx(
    tx: &tokio_postgres::Transaction<'_>,
    tenant: &str,
    project: &str,
    request_key: &str,
    authorization_id: &str,
    op: &str,
    event_id: &str,
) -> PgResult<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    tx.execute(
        "INSERT INTO awr_team.agent_authorization_receipts(
            tenant_id,project_id,request_key,authorization_id,op,event_id,replayed,created_at_ms)
         VALUES($1,$2,$3,$4,$5,$6,FALSE,$7)",
        &[
            &tenant,
            &project,
            &request_key,
            &authorization_id,
            &op,
            &event_id,
            &now,
        ],
    )
    .await?;
    Ok(())
}
