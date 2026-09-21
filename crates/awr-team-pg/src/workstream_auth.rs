use crate::{PgError, PgResult};
use awr_core::{Id, WorkstreamAccess, WorkstreamCatalog, WorkstreamGrant};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tokio_postgres::Transaction;

/// Hash a high-entropy operator-issued bearer. No raw credential is stored.
/// Format: `awr1.<credential id>.<64 lowercase hexadecimal characters>`.
pub fn workstream_credential_hash(token: &str) -> PgResult<String> {
    token_id(token)?;
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(format!("awr-team-credential-v1:{token}"))
    ))
}

fn token_id(token: &str) -> PgResult<&str> {
    if token.len() > 199 {
        return Err(PgError::Forbidden);
    }
    let parts: Vec<_> = token.split('.').collect();
    if parts.len() != 3
        || parts[0] != "awr1"
        || parts[1].is_empty()
        || parts[1].len() > 128
        || !parts[1]
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        || parts[2].len() != 64
        || !parts[2]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(PgError::Forbidden);
    }
    Ok(parts[1])
}

pub(crate) struct ReaderAuthority {
    pub access: WorkstreamAccess,
    pub catalog: WorkstreamCatalog,
    pub snapshot: String,
    pub epoch: String,
    pub project_status: String,
    pub revision: i64,
    pub binding: String,
    pub grant_versions: BTreeMap<Id, i64>,
}

/// Resolve every security fact in the action transaction. Row locks make a
/// concurrent disable/revocation linearize before or after the read, never in
/// its middle. No caller-supplied actor, client or grant is accepted as proof.
pub(crate) async fn authenticate(
    tx: &Transaction<'_>,
    tenant: &str,
    project: &str,
    token: &str,
) -> PgResult<ReaderAuthority> {
    let credential_id = token_id(token)?;
    let hash = workstream_credential_hash(token)?;
    crate::tx::bind_workstream_scope(tx, tenant, project).await?;
    let mode = tx
        .query_opt(
            "SELECT enabled FROM awr_team.workstream_modes
        WHERE tenant_id=$1 AND project_id=$2 FOR SHARE",
            &[&tenant, &project],
        )
        .await?
        .ok_or(PgError::Forbidden)?;
    // Lock the project before identity/policy records; source/admin protocols
    // use the same admission -> project order.
    let p = tx
        .query_opt(
            "SELECT active_snapshot_id,coordinator_epoch,project_revision,status FROM awr_team.projects
        WHERE tenant_id=$1 AND id=$2 FOR SHARE",
            &[&tenant, &project],
        )
        .await?
        .ok_or(PgError::Forbidden)?;
    let identity = tx.query_opt("SELECT c.actor_id,c.client_id,m.membership_version,m.role
        FROM awr_team.credentials c
        JOIN awr_team.tenants t ON t.id=c.tenant_id
        JOIN awr_team.actors a ON a.tenant_id=c.tenant_id AND a.id=c.actor_id
        JOIN awr_team.project_memberships m ON m.tenant_id=c.tenant_id AND m.actor_id=c.actor_id AND m.project_id=$2
        WHERE c.tenant_id=$1 AND c.id=$3 AND c.secret_hash=$4
          AND c.revoked_at IS NULL AND (c.expires_at IS NULL OR c.expires_at>clock_timestamp())
          AND t.status='active' AND a.status='active'
        FOR SHARE OF t,a,c,m", &[&tenant,&project,&credential_id,&hash]).await?.ok_or(PgError::Forbidden)?;
    if !mode.get::<_, bool>(0) {
        return Err(PgError::Unsupported(
            "project has not enabled workstreams".into(),
        ));
    }
    let actor: String = identity.get(0);
    let client: String = identity.get(1);
    let membership: i64 = identity.get(2);
    let role: String = identity.get(3);
    let snapshot: String = p
        .get::<_, Option<String>>(0)
        .ok_or(PgError::InactiveCandidate)?;
    let row = tx
        .query_opt(
            "SELECT catalog_json FROM awr_team.workstream_catalogs
        WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3",
            &[&tenant, &project, &snapshot],
        )
        .await?
        .ok_or(PgError::InactiveCandidate)?;
    let catalog: WorkstreamCatalog =
        serde_json::from_value(row.get(0)).map_err(|_| PgError::SourceDivergence)?;
    catalog.validate()?;
    if catalog.project_id != project {
        return Err(PgError::SourceDivergence);
    }
    let rows = tx.query("SELECT workstream_id,authority_version,can_read,can_write,can_manage,grant_version
        FROM awr_team.workstream_grants WHERE tenant_id=$1 AND project_id=$2 AND actor_id=$3 AND client_id=$4 AND active
        ORDER BY workstream_id FOR SHARE", &[&tenant,&project,&actor,&client]).await?;
    let mut grants = Vec::new();
    let mut grant_versions = BTreeMap::new();
    for row in rows {
        let id: Id = row
            .get::<_, String>(0)
            .parse()
            .map_err(|_| PgError::Forbidden)?;
        let authority: i64 = row.get(1);
        grants.push(WorkstreamGrant {
            workstream_id: id,
            authority_version: authority.try_into().map_err(|_| PgError::Forbidden)?,
            read: row.get(2),
            write: row.get::<_, bool>(3) && role != "reader",
            manage: row.get::<_, bool>(4) && role == "admin",
        });
        grant_versions.insert(id, row.get(5));
    }
    let binding = awr_team::request_hash(
        &json!({"tenant":tenant,"project":project,"credential":credential_id,
        "actor":actor,"client":client,"membership":membership,"role":role}),
    )
    .map_err(|_| PgError::Forbidden)?;
    let access = WorkstreamAccess {
        project_id: project.into(),
        subject: binding.clone(),
        grants,
    };
    access.validate()?;
    Ok(ReaderAuthority {
        access,
        catalog,
        snapshot,
        epoch: p.get(1),
        revision: p.get(2),
        project_status: p.get(3),
        binding,
        grant_versions,
    })
}
