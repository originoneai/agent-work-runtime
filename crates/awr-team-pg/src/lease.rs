use crate::error::{PgError, PgResult};
use crate::tx::{bind_scope, new_id};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_postgres::error::SqlState;

use crate::tx::{de_i64_flex, ser_i64_string};

#[derive(Clone, Debug, Serialize)]
pub struct SessionRecord {
    pub id: String,
    pub actor_id: String,
    pub client_id: String,
    pub conversation_id: String,
    pub work_id: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimRecord {
    pub id: String,
    pub session_id: String,
    pub actor_id: String,
    pub work_id: String,
    #[serde(serialize_with = "ser_i64_string", deserialize_with = "de_i64_flex")]
    pub fence: i64,
    #[serde(serialize_with = "ser_i64_string", deserialize_with = "de_i64_flex")]
    pub lease_version: i64,
    pub expires_at: String,
    pub state: String,
    pub replayed: bool,
}

pub struct LeaseStore {
    pool: crate::PgPool,
}

impl LeaseStore {
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

    pub async fn start_session(
        &self,
        tenant_id: &str,
        project_id: &str,
        actor_id: &str,
        client_id: &str,
        conversation_id: &str,
        scope_id: &str,
        work_id: &str,
    ) -> PgResult<SessionRecord> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let id = new_id();
        tx.execute(
            "INSERT INTO awr_team.sessions(
                tenant_id, project_id, id, scope_id, work_id, actor_id, client_id,
                conversation_id, state)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'active')",
            &[
                &tenant_id,
                &project_id,
                &id,
                &scope_id,
                &work_id,
                &actor_id,
                &client_id,
                &conversation_id,
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(SessionRecord {
            id,
            actor_id: actor_id.into(),
            client_id: client_id.into(),
            conversation_id: conversation_id.into(),
            work_id: work_id.into(),
            state: "active".into(),
        })
    }

    pub async fn claim(
        &self,
        tenant_id: &str,
        project_id: &str,
        session_id: &str,
        actor_id: &str,
        client_id: &str,
        request_id: &str,
        ttl_seconds: i32,
    ) -> PgResult<ClaimRecord> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let op = "claim.acquire";
        let op_args = json!({"session_id": session_id, "ttl_seconds": ttl_seconds});
        let request_hash = canonical_op_hash(op, request_id, &op_args)?;
        if let Some(existing) =
            load_operation(&tx, tenant_id, project_id, actor_id, client_id, request_id).await?
        {
            if existing.0 != request_hash {
                return Err(PgError::IdempotencyConflict);
            }
            return replay_claim(&existing.1);
        }
        let session = tx
            .query_opt(
                "SELECT work_id, scope_id, actor_id, client_id, conversation_id, state
                 FROM awr_team.sessions
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &session_id],
            )
            .await?
            .ok_or(PgError::SessionNotFound)?;
        let work_id: String = session.get(0);
        let scope_id: String = session.get(1);
        let session_actor: String = session.get(2);
        let session_client: String = session.get(3);
        let state: String = session.get(5);
        if session_actor != actor_id || session_client != client_id || state != "active" {
            return Err(PgError::Forbidden);
        }
        expire_due(&tx, tenant_id, project_id, &scope_id, &work_id).await?;
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
        let open_wait: i64 = tx
            .query_one(
                "SELECT count(*) FROM awr_team.wait_items
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='open'",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?
            .get(0);
        if open_wait > 0 {
            return Err(PgError::WaitOpen);
        }
        let held = tx
            .query_opt(
                "SELECT actor_id FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4
                   AND state='active'",
                &[&tenant_id, &project_id, &scope_id, &work_id],
            )
            .await?;
        if let Some(row) = held {
            let holder: String = row.get(0);
            if holder != actor_id {
                return Err(PgError::ClaimHeld);
            }
        }
        tx.execute(
            "INSERT INTO awr_team.work_runtime(
                tenant_id, project_id, scope_id, work_id, state, work_version, last_fence)
             VALUES ($1,$2,$3,$4,'claimed',1,1)
             ON CONFLICT (tenant_id, project_id, scope_id, work_id)
             DO UPDATE SET last_fence = awr_team.work_runtime.last_fence + 1, state='claimed'",
            &[&tenant_id, &project_id, &scope_id, &work_id],
        )
        .await?;
        let fence: i64 = tx
            .query_one(
                "SELECT last_fence FROM awr_team.work_runtime
                 WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4",
                &[&tenant_id, &project_id, &scope_id, &work_id],
            )
            .await?
            .get(0);
        let claim_id = new_id();
        let insert = tx.execute(
            "INSERT INTO awr_team.claims(
                tenant_id, project_id, id, scope_id, work_id, session_id, actor_id,
                fence, lease_version, expires_at, state)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1, clock_timestamp() + make_interval(secs => $9), 'active')",
            &[
                &tenant_id,
                &project_id,
                &claim_id,
                &scope_id,
                &work_id,
                &session_id,
                &actor_id,
                &fence,
                &(ttl_seconds as f64),
            ],
        )
        .await;
        if let Err(error) = insert {
            if error.code() == Some(&SqlState::UNIQUE_VIOLATION) {
                return Err(PgError::ClaimHeld);
            }
            return Err(error.into());
        }
        let expires_at: String = tx
            .query_one(
                "SELECT expires_at::text FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?
            .get(0);
        let result = json!({
            "id": claim_id,
            "session_id": session_id,
            "actor_id": actor_id,
            "work_id": work_id,
            "fence": fence.to_string(),
            "lease_version": "1",
            "expires_at": expires_at,
            "state": "active",
        });
        // Lease state changes are observable: revision and event commit in
        // the SAME transaction (CR #39 P2-5). Replays return before any
        // mutation, so they never duplicate events.
        let committed_revision = emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            &work_id,
            "claim.acquired",
            json!({
                "claim_id": claim_id,
                "session_id": session_id,
                "scope_id": scope_id,
                "fence": fence.to_string(),
            }),
        )
        .await?;
        store_operation(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            client_id,
            request_id,
            op,
            &request_hash,
            committed_revision,
            &result,
        )
        .await?;
        tx.commit().await?;
        Ok(ClaimRecord {
            id: claim_id,
            session_id: session_id.into(),
            actor_id: actor_id.into(),
            work_id,
            fence,
            lease_version: 1,
            expires_at,
            state: "active".into(),
            replayed: false,
        })
    }

    pub async fn renew(
        &self,
        tenant_id: &str,
        project_id: &str,
        claim_id: &str,
        actor_id: &str,
        client_id: &str,
        request_id: &str,
        ttl_seconds: i32,
    ) -> PgResult<ClaimRecord> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let op = "claim.renew";
        let op_args = json!({"claim_id": claim_id, "ttl_seconds": ttl_seconds});
        let request_hash = canonical_op_hash(op, request_id, &op_args)?;
        if let Some(existing) =
            load_operation(&tx, tenant_id, project_id, actor_id, client_id, request_id).await?
        {
            if existing.0 != request_hash {
                return Err(PgError::IdempotencyConflict);
            }
            return replay_claim(&existing.1);
        }
        let row = tx
            .query_opt(
                "SELECT session_id, actor_id, work_id, fence, lease_version, expires_at::text, state, scope_id
                 FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3
                 FOR UPDATE",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("claim not found".into()))?;
        let session_id: String = row.get(0);
        let holder: String = row.get(1);
        let work_id: String = row.get(2);
        let fence: i64 = row.get(3);
        let lease_version: i64 = row.get(4);
        let state: String = row.get(6);
        let scope_id: String = row.get(7);
        if holder != actor_id {
            return Err(PgError::Forbidden);
        }
        // Renew must keep the session binding established at acquire time:
        // same actor via a DIFFERENT client is not a renew, it is a takeover
        // and must go through handoff (CR #39 P2-3).
        let binding = tx
            .query_opt(
                "SELECT actor_id, client_id, state FROM awr_team.sessions
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &session_id],
            )
            .await?
            .ok_or(PgError::SessionNotFound)?;
        let bound_actor: String = binding.get(0);
        let bound_client: String = binding.get(1);
        let session_state: String = binding.get(2);
        if bound_actor != actor_id || bound_client != client_id || session_state != "active" {
            return Err(PgError::Forbidden);
        }
        expire_due(&tx, tenant_id, project_id, &scope_id, &work_id).await?;
        let still: String = tx
            .query_one(
                "SELECT state FROM awr_team.claims WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?
            .get(0);
        if still != "active" || state != "active" {
            return Err(PgError::LeaseExpired);
        }
        tx.execute(
            "UPDATE awr_team.claims
             SET lease_version = lease_version + 1,
                 expires_at = clock_timestamp() + make_interval(secs => $4)
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3 AND state='active'",
            &[&tenant_id, &project_id, &claim_id, &(ttl_seconds as f64)],
        )
        .await?;
        let updated = tx
            .query_one(
                "SELECT lease_version, expires_at::text FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?;
        let next_version: i64 = updated.get(0);
        let expires_at: String = updated.get(1);
        let result = json!({
            "id": claim_id,
            "session_id": session_id,
            "actor_id": actor_id,
            "work_id": work_id,
            "fence": fence.to_string(),
            "lease_version": next_version.to_string(),
            "expires_at": expires_at,
            "state": "active",
        });
        let committed_revision: i64 = tx
            .query_one(
                "SELECT project_revision FROM awr_team.projects WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &project_id],
            )
            .await?
            .get(0);
        store_operation(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            client_id,
            request_id,
            op,
            &request_hash,
            committed_revision,
            &result,
        )
        .await?;
        tx.commit().await?;
        let _ = lease_version;
        Ok(ClaimRecord {
            id: claim_id.into(),
            session_id,
            actor_id: actor_id.into(),
            work_id,
            fence,
            lease_version: next_version,
            expires_at,
            state: "active".into(),
            replayed: false,
        })
    }

    pub async fn release(
        &self,
        tenant_id: &str,
        project_id: &str,
        claim_id: &str,
        actor_id: &str,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT actor_id, state FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("claim not found".into()))?;
        let holder: String = row.get(0);
        let state: String = row.get(1);
        if holder != actor_id {
            return Err(PgError::Forbidden);
        }
        if state != "active" {
            return Err(PgError::LeaseExpired);
        }
        tx.execute(
            "UPDATE awr_team.claims SET state='released'
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &claim_id],
        )
        .await?;
        let work_id: String = tx
            .query_one(
                "SELECT work_id FROM awr_team.claims WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?
            .get(0);
        emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            &work_id,
            "claim.released",
            json!({"claim_id": claim_id}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn expire_due(
        &self,
        tenant_id: &str,
        project_id: &str,
        scope_id: &str,
        work_id: &str,
    ) -> PgResult<u64> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let count = expire_due(&tx, tenant_id, project_id, scope_id, work_id).await?;
        tx.commit().await?;
        Ok(count)
    }

    pub async fn handoff(
        &self,
        tenant_id: &str,
        project_id: &str,
        claim_id: &str,
        actor_id: &str,
        successor_actor_id: &str,
        successor_client_id: &str,
        conversation_id: &str,
    ) -> PgResult<ClaimRecord> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT session_id, actor_id, work_id, scope_id, state
                 FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3 FOR UPDATE",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("claim not found".into()))?;
        let old_session: String = row.get(0);
        let holder: String = row.get(1);
        let work_id: String = row.get(2);
        let scope_id: String = row.get(3);
        let state: String = row.get(4);
        if holder != actor_id {
            return Err(PgError::Forbidden);
        }
        expire_due(&tx, tenant_id, project_id, &scope_id, &work_id).await?;
        // The state cached before the expiry sweep is NOT authoritative:
        // an active-looking row whose expires_at already passed was just
        // flipped to 'expired'. Re-read after the sweep (CR #39 P2-2).
        let still: String = tx
            .query_one(
                "SELECT state FROM awr_team.claims WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &claim_id],
            )
            .await?
            .get(0);
        if state != "active" || still != "active" {
            return Err(PgError::LeaseExpired);
        }
        tx.execute(
            "UPDATE awr_team.claims SET state='handed_off'
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &claim_id],
        )
        .await?;
        tx.execute(
            "UPDATE awr_team.sessions SET state='ended'
             WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
            &[&tenant_id, &project_id, &old_session],
        )
        .await?;
        tx.execute(
            "UPDATE awr_team.work_runtime
             SET last_fence = last_fence + 1
             WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4",
            &[&tenant_id, &project_id, &scope_id, &work_id],
        )
        .await?;
        let fence: i64 = tx
            .query_one(
                "SELECT last_fence FROM awr_team.work_runtime
                 WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4",
                &[&tenant_id, &project_id, &scope_id, &work_id],
            )
            .await?
            .get(0);
        let session_id = new_id();
        tx.execute(
            "INSERT INTO awr_team.sessions(
                tenant_id, project_id, id, scope_id, work_id, actor_id, client_id,
                conversation_id, predecessor_id, state)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'active')",
            &[
                &tenant_id,
                &project_id,
                &session_id,
                &scope_id,
                &work_id,
                &successor_actor_id,
                &successor_client_id,
                &conversation_id,
                &old_session,
            ],
        )
        .await?;
        let new_claim = new_id();
        tx.execute(
            "INSERT INTO awr_team.claims(
                tenant_id, project_id, id, scope_id, work_id, session_id, actor_id,
                fence, lease_version, expires_at, state)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,1, clock_timestamp() + interval '60 seconds', 'active')",
            &[
                &tenant_id,
                &project_id,
                &new_claim,
                &scope_id,
                &work_id,
                &session_id,
                &successor_actor_id,
                &fence,
            ],
        )
        .await?;
        let expires_at: String = tx
            .query_one(
                "SELECT expires_at::text FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &new_claim],
            )
            .await?
            .get(0);
        emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            &work_id,
            "claim.handed_off",
            json!({
                "old_claim_id": claim_id,
                "new_claim_id": new_claim,
                "from": actor_id,
                "to": successor_actor_id,
                "scope_id": scope_id,
                "fence": fence.to_string(),
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(ClaimRecord {
            id: new_claim,
            session_id,
            actor_id: successor_actor_id.into(),
            work_id,
            fence,
            lease_version: 1,
            expires_at,
            state: "active".into(),
            replayed: false,
        })
    }

    pub async fn wait(
        &self,
        tenant_id: &str,
        project_id: &str,
        session_id: &str,
        actor_id: &str,
        question: &str,
    ) -> PgResult<String> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let claim = tx
            .query_opt(
                "SELECT id, expires_at::text FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND session_id=$3
                   AND actor_id=$4 AND state='active'
                   AND expires_at > clock_timestamp()",
                &[&tenant_id, &project_id, &session_id, &actor_id],
            )
            .await?
            .ok_or(PgError::LeaseExpired)?;
        let expires_before: String = claim.get(1);
        let work_id: String = tx
            .query_one(
                "SELECT work_id FROM awr_team.sessions
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &session_id],
            )
            .await?
            .get(0);
        let wait_id = new_id();
        tx.execute(
            "INSERT INTO awr_team.wait_items(
                tenant_id, project_id, id, session_id, work_id, question, state)
             VALUES ($1,$2,$3,$4,$5,$6,'open')",
            &[
                &tenant_id,
                &project_id,
                &wait_id,
                &session_id,
                &work_id,
                &question,
            ],
        )
        .await?;
        let expires_after: String = tx
            .query_one(
                "SELECT expires_at::text FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND session_id=$3 AND state='active'",
                &[&tenant_id, &project_id, &session_id],
            )
            .await?
            .get(0);
        if expires_after != expires_before {
            return Err(PgError::Protocol("wait must not renew the lease".into()));
        }
        emit_event(
            &tx,
            tenant_id,
            project_id,
            actor_id,
            &work_id,
            "wait.opened",
            json!({"wait_id": wait_id, "session_id": session_id}),
        )
        .await?;
        tx.commit().await?;
        Ok(wait_id)
    }

    /// Record a reply. The replier identity comes from the trusted calling
    /// context: the actor answering a wait is usually NOT the actor who
    /// opened it, and the session id is not an actor (CR #56 P2-2).
    pub async fn reply(
        &self,
        tenant_id: &str,
        project_id: &str,
        wait_id: &str,
        replier_actor_id: &str,
        reply: &str,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let updated = tx
            .execute(
                "UPDATE awr_team.wait_items SET state='replied', reply=$4
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3 AND state='open'",
                &[&tenant_id, &project_id, &wait_id, &reply],
            )
            .await?;
        if updated != 1 {
            return Err(PgError::Protocol("wait not open".into()));
        }
        let wait_row = tx
            .query_one(
                "SELECT work_id, session_id FROM awr_team.wait_items
                 WHERE tenant_id=$1 AND project_id=$2 AND id=$3",
                &[&tenant_id, &project_id, &wait_id],
            )
            .await?;
        let wait_work: (String, String) = (wait_row.get(0), wait_row.get(1));
        emit_event(
            &tx,
            tenant_id,
            project_id,
            replier_actor_id,
            &wait_work.0,
            "wait.replied",
            json!({"wait_id": wait_id, "session_id": wait_work.1}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_recovery_blocked(
        &self,
        tenant_id: &str,
        project_id: &str,
        scope_id: &str,
        work_id: &str,
        blocked: bool,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        tx.execute(
            "INSERT INTO awr_team.work_runtime(
                tenant_id, project_id, scope_id, work_id, state, work_version, last_fence, recovery_blocked)
             VALUES ($1,$2,$3,$4,'blocked',1,0,$5)
             ON CONFLICT (tenant_id, project_id, scope_id, work_id)
             DO UPDATE SET recovery_blocked=$5",
            &[&tenant_id, &project_id, &scope_id, &work_id, &blocked],
        )
        .await?;
        // Changing whether a work may execute is an observable state
        // transition (CR #56 P2-3).
        emit_event(
            &tx,
            tenant_id,
            project_id,
            "system",
            work_id,
            if blocked {
                "work.recovery_blocked"
            } else {
                "work.recovery_unblocked"
            },
            json!({"scope_id": scope_id, "blocked": blocked}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Check the fence of the caller's claim. The scope is explicit so two
    /// legitimate claims in different scopes cannot produce a multi-row
    /// error (CR #39 P2-4). Validity uses the database current time: a
    /// claim past its expires_at is not valid even before the expiry sweep
    /// runs (CR #39 P2-2).
    ///
    /// NOTE: calling this check and writing in a LATER transaction does not
    /// guarantee no handoff happened in between; the real write gate must
    /// live inside the writing transaction.
    pub async fn require_fence(
        &self,
        tenant_id: &str,
        project_id: &str,
        scope_id: &str,
        work_id: &str,
        actor_id: &str,
        fence: i64,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT fence, actor_id, state FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4
                   AND state='active' AND expires_at > clock_timestamp()",
                &[&tenant_id, &project_id, &scope_id, &work_id],
            )
            .await?
            .ok_or(PgError::LeaseExpired)?;
        let current: i64 = row.get(0);
        let holder: String = row.get(1);
        if holder != actor_id || current != fence {
            return Err(PgError::StaleFence);
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn lock_project(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
) -> PgResult<()> {
    crate::tx::lock_active_project(tx, tenant_id, project_id).await
}

/// Expire due claims. Every claim that actually transitions is returned
/// AND reported as a claim.expired event in the same transaction; sweeps
/// that change nothing emit nothing (CR #56 P2-3).
async fn expire_due(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    scope_id: &str,
    work_id: &str,
) -> PgResult<u64> {
    let expired = tx
        .query(
            "UPDATE awr_team.claims SET state='expired'
             WHERE tenant_id=$1 AND project_id=$2 AND scope_id=$3 AND work_id=$4
               AND state='active' AND expires_at <= clock_timestamp()
             RETURNING id, actor_id",
            &[&tenant_id, &project_id, &scope_id, &work_id],
        )
        .await?;
    for row in &expired {
        let claim_id: String = row.get(0);
        emit_event(
            tx,
            tenant_id,
            project_id,
            "system",
            work_id,
            "claim.expired",
            json!({"claim_id": claim_id, "scope_id": scope_id}),
        )
        .await?;
    }
    Ok(expired.len() as u64)
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

/// Canonical request identity: op, request id AND the meaningful arguments
/// (target session/claim, TTL). Two calls sharing the operation key but
/// differing here are NOT replays (CR #39 P2-1). Receipts written by the
/// pre-fix format never match this hash and fail closed as
/// IdempotencyConflict instead of replaying an unchecked result.
fn canonical_op_hash(op: &str, request_id: &str, args: &Value) -> PgResult<String> {
    awr_team::request_hash(&json!({
        "op": op,
        "request_id": request_id,
        "args": args,
    }))
    .map_err(|e| PgError::Protocol(e.to_string()))
}

use crate::tx::emit_event;

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
    // Receipts always carry the committed project revision: other stores
    // sharing the operations table must never read a NULL here (CR #56 P2-1).
    let op_id = new_id();
    tx.execute(
        "INSERT INTO awr_team.operations(
            tenant_id, project_id, id, actor_id, client_id, request_id, op,
            request_hash, state, committed_project_revision, result_json)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'committed',$9,$10)",
        &[
            &tenant_id,
            &project_id,
            &op_id,
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

/// Strict replay: a stored receipt must contain every field; defaults are
/// never invented (the old code fabricated an "active empty claim" from a
/// foreign receipt — CR #39 P2-1). fence/lease_version accept the legacy
/// numeric form and the current decimal-string form (CR #39 P2-6).
fn replay_claim(result: &Value) -> PgResult<ClaimRecord> {
    let required = |key: &str| -> PgResult<String> {
        result
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| PgError::Protocol(format!("stored receipt missing {key}")))
    };
    let flex_i64 = |key: &str| -> PgResult<i64> {
        match result.get(key) {
            Some(Value::String(s)) => s
                .parse()
                .map_err(|_| PgError::Protocol(format!("stored receipt has invalid {key}"))),
            Some(Value::Number(n)) => n
                .as_i64()
                .ok_or_else(|| PgError::Protocol(format!("stored receipt has invalid {key}"))),
            _ => Err(PgError::Protocol(format!("stored receipt missing {key}"))),
        }
    };
    Ok(ClaimRecord {
        id: required("id")?,
        session_id: required("session_id")?,
        actor_id: required("actor_id")?,
        work_id: required("work_id")?,
        fence: flex_i64("fence")?,
        lease_version: flex_i64("lease_version")?,
        expires_at: required("expires_at")?,
        state: required("state")?,
        replayed: true,
    })
}
