//! AWR-TMCP-030: intersect WS-015/016 delegation action sets with TMCP-010
//! product permissions. Consumes stored person/agent grants; does not reimplement
//! the person model or claim state machine.

use crate::workstream_auth::ReaderAuthority;
use crate::{PgError, PgResult};
use awr_core::{
    AgentAuthorization, AuthorizationStatus, AuthorizedAction, ExecutionSubjectKind,
    bind_runtime_identity,
};
use awr_team::{Action, RoleTemplate, template_actions};
use std::collections::BTreeSet;

/// Map one WS-016 authorized action onto TMCP-010 product actions it may enable.
/// `ManageAuthorization` maps to nothing here: child grants are WS-016 store ops,
/// never project-admin / audit product power.
pub fn tmcp_actions_for_authorized(action: AuthorizedAction) -> BTreeSet<Action> {
    use Action::*;
    let mut out = BTreeSet::new();
    match action {
        AuthorizedAction::Inspect => {
            out.insert(WorkRead);
        }
        AuthorizedAction::AcceptResponsibility
        | AuthorizedAction::OccupyCollaboratively
        | AuthorizedAction::ClaimCoordination => {
            // Coordination is not side-effect permission.
            out.insert(ClaimManageOwn);
        }
        AuthorizedAction::StartWork => {
            out.insert(SessionMaintainOwn);
            out.insert(ExecutionRequestAndReportOwn);
            out.insert(DeliverySubmitAndRequestReview);
        }
        AuthorizedAction::Review => {
            // Independent review still needs `independent_review_grant` on the
            // TMCP scope; mapping alone never sets that flag.
            out.insert(ReviewDecide);
        }
        AuthorizedAction::ManageAuthorization => {}
    }
    out
}

pub fn tmcp_actions_for_authorized_set(actions: &BTreeSet<AuthorizedAction>) -> BTreeSet<Action> {
    let mut out = BTreeSet::new();
    for action in actions {
        out.extend(tmcp_actions_for_authorized(*action));
    }
    out
}

/// Membership template ∩ explicit delegation. Templates never grant trusted
/// executor / reconcile specials (`template_grants_special` is always false).
pub fn intersect_delegation_with_template(
    role: RoleTemplate,
    delegated: &BTreeSet<Action>,
) -> BTreeSet<Action> {
    template_actions(role)
        .intersection(delegated)
        .copied()
        .collect()
}

pub fn actor_requires_explicit_delegation(actor_kind: &str) -> bool {
    actor_kind == "agent"
}

/// Load covering WS-016 grants for an agent and install the intersected TMCP
/// action set. Humans/system leave `delegated_actions = None` (full template).
/// Agents without a covering grant get an empty set (deny). Model/client/session
/// changes cannot revive revoked or expired grants.
pub(crate) async fn resolve_agent_delegation(
    tx: &tokio_postgres::Transaction<'_>,
    auth: &mut ReaderAuthority,
    project_id: &str,
    work_id: Option<&str>,
    session_id: Option<&str>,
    requested_action: Option<Action>,
    now_ms: i64,
) -> PgResult<()> {
    if !actor_requires_explicit_delegation(&auth.actor_kind) {
        auth.delegated_actions = None;
        auth.delegation_id = None;
        return Ok(());
    }

    let rows = tx
        .query(
            "SELECT body_json FROM awr_team.agent_authorizations
             WHERE tenant_id=$1 AND project_id=$2 AND subject_id=$3 AND status='active'
             ORDER BY created_at_ms ASC, id ASC
             FOR SHARE",
            &[&auth.tenant_id, &project_id, &auth.actor_id],
        )
        .await?;

    let mut candidates = Vec::new();
    let task_stream = if let Some(work) = work_id.filter(|w| !w.is_empty()) {
        crate::tx::bind_workstream_scope(tx, &auth.tenant_id, project_id).await?;
        tx.query_opt(
            "SELECT workstream_id FROM awr_team.workstream_ownership
             WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3",
            &[&auth.tenant_id, &project_id, &work],
        )
        .await?
        .map(|row| row.get::<_, String>(0))
    } else {
        None
    };

    for row in rows {
        let body: serde_json::Value = row.get(0);
        let grant: AgentAuthorization = serde_json::from_value(body)
            .map_err(|e| PgError::Protocol(format!("corrupt agent authorization: {e}")))?;
        if !matches!(grant.subject_kind, ExecutionSubjectKind::Agent) {
            continue;
        }
        if grant.subject_id != auth.actor_id {
            continue;
        }
        if !matches!(grant.status, AuthorizationStatus::Active) {
            continue;
        }
        if !grant.is_effective_at(now_ms) {
            continue;
        }
        if bind_runtime_identity(&grant, &auth.client_id, session_id, None, now_ms).is_err() {
            continue;
        }
        // Live WS-015 relationship check: a valid-looking binding_id in JSON is
        // not enough — person and binding rows must be active and match.
        if !live_person_binding_covers(tx, &auth.tenant_id, project_id, &grant, &auth.actor_id)
            .await?
        {
            continue;
        }
        if grant.scope.project_id() != project_id {
            continue;
        }

        let mapped = tmcp_actions_for_authorized_set(&grant.actions);
        let mut intersected = intersect_delegation_with_template(auth.role_template, &mapped);
        // Review is not a role-template permission. It needs both the live
        // membership grant and this covering delegation, never either alone.
        if auth.agent_review && mapped.contains(&Action::ReviewDecide) {
            intersected.insert(Action::ReviewDecide);
        }
        candidates.push((grant, intersected));
    }

    // Resolve narrowing before choosing an action. Otherwise an action removed
    // by a child could fall back to the broader parent. Compute this before
    // task filtering: an out-of-scope task cannot resurrect a narrowed parent.
    // Independent grants may
    // cover different actions, but are never unioned into synthetic authority.
    let narrowed: BTreeSet<&str> = candidates
        .iter()
        .filter_map(|(child, child_actions)| {
            let parent_id = child.parent_authorization_id.as_deref()?;
            candidates.iter().find_map(|(parent, parent_actions)| {
                (parent.id == parent_id
                    && child_actions.is_subset(parent_actions)
                    && child.scope.is_within(&parent.scope)
                    && (child_actions.len() < parent_actions.len() || child.scope != parent.scope))
                    .then_some(parent_id)
            })
        })
        .collect();
    let chosen = candidates.iter().find(|(grant, actions)| {
        !narrowed.contains(grant.id.as_str())
            && work_id
                .filter(|w| !w.is_empty())
                .is_none_or(|work| grant.covers_task(project_id, work, task_stream.as_deref()))
            && requested_action.map_or(!actions.is_empty(), |action| actions.contains(&action))
    });
    match chosen {
        Some((grant, actions)) => {
            auth.delegation_id = Some(grant.id.clone());
            auth.delegated_actions = Some(actions.clone());
        }
        None => {
            auth.delegation_id = None;
            auth.delegated_actions = Some(BTreeSet::new());
        }
    }
    Ok(())
}

/// Keep selector-free discovery inside the selected delegation, not merely
/// inside the broader access grant. Task/pool reads require an explicit work ID;
/// the query's normal validation still rejects selectors on project-wide ops.
pub(crate) async fn restrict_read_scope(
    tx: &tokio_postgres::Transaction<'_>,
    auth: &mut ReaderAuthority,
    project: &str,
    request: &crate::WorkstreamQuery,
) -> PgResult<()> {
    if !actor_requires_explicit_delegation(&auth.actor_kind) {
        return Ok(());
    }
    let id = auth.delegation_id.as_deref().ok_or(PgError::Forbidden)?;
    let row=tx.query_opt("SELECT body_json FROM awr_team.agent_authorizations WHERE tenant_id=$1 AND project_id=$2 AND id=$3 FOR SHARE",&[&auth.tenant_id,&project,&id]).await?.ok_or(PgError::Forbidden)?;
    let grant: AgentAuthorization =
        serde_json::from_value(row.get(0)).map_err(|_| PgError::Forbidden)?;
    let stream = match &grant.scope {
        awr_core::AuthorizationScope::Project { .. } => None,
        awr_core::AuthorizationScope::Workstream { workstream_id, .. } => {
            Some(workstream_id.clone())
        }
        awr_core::AuthorizationScope::Task { .. }
        | awr_core::AuthorizationScope::TaskPool { .. } => {
            let work = request.work_id.as_deref().ok_or(PgError::Forbidden)?;
            let row=tx.query_opt("SELECT workstream_id FROM awr_team.workstream_snapshot_ownership WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main' AND work_id=$4",&[&auth.tenant_id,&project,&auth.snapshot,&work]).await?.ok_or(PgError::Forbidden)?;
            let stream: String = row.get(0);
            if !grant.covers_task(project, work, Some(&stream)) {
                return Err(PgError::Forbidden);
            }
            Some(stream)
        }
    };
    if stream.is_some() && request.op == "planning.outcome" {
        return Err(PgError::Forbidden);
    }
    if let Some(stream) = stream {
        let stream = stream
            .parse::<awr_core::Id>()
            .map_err(|_| PgError::Forbidden)?;
        auth.access.grants.retain(|g| g.workstream_id == stream);
    }
    // Cursors and consumed context must not outlive the selected read grant.
    auth.binding =
        awr_team::request_hash(&serde_json::json!({"identity":auth.binding,"authorization":grant}))
            .map_err(|_| PgError::Forbidden)?;
    Ok(())
}

async fn live_person_binding_covers(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
    grant: &AgentAuthorization,
    agent_actor_id: &str,
) -> PgResult<bool> {
    let Some(binding_id) = grant.binding_id.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(false);
    };
    let row = tx
        .query_opt(
            "SELECT b.status, b.person_id, b.agent_id, p.status
             FROM awr_team.person_agent_bindings b
             JOIN awr_team.persons p
               ON p.tenant_id=b.tenant_id AND p.project_id=b.project_id AND p.id=b.person_id
             WHERE b.tenant_id=$1 AND b.project_id=$2 AND b.id=$3
             FOR SHARE OF b, p",
            &[&tenant_id, &project_id, &binding_id],
        )
        .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let binding_status: String = row.get(0);
    let person_id: String = row.get(1);
    let agent_id: String = row.get(2);
    let person_status: String = row.get(3);
    if binding_status != "active" || person_status != "active" {
        return Ok(false);
    }
    if agent_id != agent_actor_id {
        return Ok(false);
    }
    if person_id != grant.responsible_person_id.as_str() {
        return Ok(false);
    }
    Ok(true)
}

/// Side-effecting execution requires StartWork-mapped TMCP execution permission.
pub(crate) fn execution_side_effect_permitted(auth: &ReaderAuthority) -> bool {
    match &auth.delegated_actions {
        Some(actions) => actions.contains(&Action::ExecutionRequestAndReportOwn),
        None => {
            template_actions(auth.role_template).contains(&Action::ExecutionRequestAndReportOwn)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awr_core::AuthorizationScope;

    #[test]
    fn admin_agent_only_gets_explicit_work_slice() {
        let delegated = tmcp_actions_for_authorized_set(&BTreeSet::from([
            AuthorizedAction::ClaimCoordination,
            AuthorizedAction::StartWork,
        ]));
        let effective = intersect_delegation_with_template(RoleTemplate::ProjectAdmin, &delegated);
        assert!(effective.contains(&Action::ClaimManageOwn));
        assert!(effective.contains(&Action::ExecutionRequestAndReportOwn));
        assert!(effective.contains(&Action::SessionMaintainOwn));
        assert!(!effective.contains(&Action::AccessManageProject));
        assert!(!effective.contains(&Action::PlanningPublish));
        assert!(!effective.contains(&Action::AuditReadProject));
        assert!(!effective.contains(&Action::PlanningApprove));
    }

    #[test]
    fn claim_coordination_is_not_side_effect_permission() {
        let delegated =
            tmcp_actions_for_authorized_set(&BTreeSet::from([AuthorizedAction::ClaimCoordination]));
        let effective = intersect_delegation_with_template(RoleTemplate::Developer, &delegated);
        assert_eq!(effective, BTreeSet::from([Action::ClaimManageOwn]));
        assert!(!effective.contains(&Action::ExecutionRequestAndReportOwn));
    }

    #[test]
    fn manage_authorization_does_not_map_to_project_admin_actions() {
        let delegated = tmcp_actions_for_authorized_set(&BTreeSet::from([
            AuthorizedAction::ManageAuthorization,
        ]));
        assert!(delegated.is_empty());
        let effective = intersect_delegation_with_template(RoleTemplate::ProjectAdmin, &delegated);
        assert!(effective.is_empty());
    }

    #[test]
    fn product_roles_never_grant_special_executor_authority() {
        for role in RoleTemplate::all() {
            assert!(!awr_team::template_grants_special(
                role,
                awr_team::SpecialAuthority::TrustedExecutorAttestation
            ));
            assert!(!awr_team::template_grants_special(
                role,
                awr_team::SpecialAuthority::ExecutionReconciliation
            ));
        }
    }

    #[test]
    fn child_intersection_only_narrows() {
        let parent = intersect_delegation_with_template(
            RoleTemplate::ProjectAdmin,
            &tmcp_actions_for_authorized_set(&BTreeSet::from([
                AuthorizedAction::StartWork,
                AuthorizedAction::ClaimCoordination,
                AuthorizedAction::Inspect,
            ])),
        );
        let child = intersect_delegation_with_template(
            RoleTemplate::ProjectAdmin,
            &tmcp_actions_for_authorized_set(&BTreeSet::from([AuthorizedAction::StartWork])),
        );
        assert!(child.is_subset(&parent));
        assert!(child.len() < parent.len());
    }

    #[test]
    fn authorization_scope_exposes_project_id() {
        let scope = AuthorizationScope::Project {
            project_id: "p".into(),
        };
        assert_eq!(scope.project_id(), "p");
    }
}
