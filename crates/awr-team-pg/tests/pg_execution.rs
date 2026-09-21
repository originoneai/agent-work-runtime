#![cfg(feature = "pg-tests")]
//! TEAM-P7 execution protocol tests on real PostgreSQL + real filesystem.
//! Fixture isolation comes from tests/common (CR #41 P2-13).

use awr_team_pg::{
    CrashPoint, ExecutionStore, GraphStore, LeaseStore, PgError, ReferenceRunner,
    exactly_once_supported,
};
use serde_json::json;
use std::sync::MutexGuard;
use tokio_postgres::Client;

mod common;
use common::{fresh_team_schema, test_config, with_app_role};

const TENANT: &str = "tenant-a";
const PROJECT: &str = "project-a";
const ACTOR: &str = "actor-a";
const OTHER: &str = "actor-b";
const RUNNER: &str = "runner-a";
const CLIENT: &str = "client-a";

async fn setup() -> (
    MutexGuard<'static, ()>,
    Client,
    ExecutionStore,
    LeaseStore,
    GraphStore,
    String,
) {
    let (guard, admin, db) = fresh_team_schema().await;
    admin
        .batch_execute(
            "INSERT INTO awr_team.tenants(id,name,status) VALUES ('tenant-a','A','active');
             INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status) VALUES
                ('tenant-a','actor-a','agent','A','active'),
                ('tenant-a','actor-b','agent','B','active'),
                ('tenant-a','runner-a','system','Runner','active'),
                ('tenant-a','runner-b','system','Runner B','active');
             INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status)
                VALUES ('tenant-a','project-a','alpha','team','epoch-1','active');
             INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
                VALUES ('tenant-a','project-a','main','main','active');
             INSERT INTO awr_team.work_items(tenant_id,project_id,id,external_key)
                VALUES ('tenant-a','project-a','work-a','W');",
        )
        .await
        .unwrap();
    let config = with_app_role(&test_config(), &db);
    (
        guard,
        admin,
        ExecutionStore::from_config(config.clone()),
        LeaseStore::from_config(config.clone()),
        GraphStore::from_config(config),
        db,
    )
}

async fn claimed(leases: &LeaseStore) -> (String, String) {
    let session = leases
        .start_session(TENANT, PROJECT, ACTOR, CLIENT, "conv", "main", "work-a")
        .await
        .unwrap();
    let claim = leases
        .claim(TENANT, PROJECT, &session.id, ACTOR, CLIENT, "claim-1", 3600)
        .await
        .unwrap();
    (session.id, claim.id)
}

fn writes_in_scope() -> serde_json::Value {
    json!([{"path": "src/foo/a.rs", "content": "fn a() {}"}])
}

async fn prepare_default(store: &ExecutionStore, claim: &str, request: &str) -> String {
    store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            request,
            claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &writes_in_scope(),
        )
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn request_id_replay_returns_the_committed_execution() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let first = store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-1",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &writes_in_scope(),
        )
        .await
        .unwrap();
    let replay = store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-1",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &writes_in_scope(),
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(first.id, replay.id);
    assert_eq!(first.effect_key, replay.effect_key);
}

// CR #41 P2-5: same request key with a different payload is a conflict.
#[tokio::test]
async fn prepare_replay_rejects_changed_payload() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-x",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &json!([{"path": "src/foo/a.rs", "content": "one"}]),
        )
        .await
        .unwrap();
    let err = store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-x",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &json!([{"path": "src/foo/a.rs", "content": "two"}]),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::IdempotencyConflict), "got {err}");
}

#[tokio::test]
async fn unknown_outcome_blocks_redispatch_and_keeps_reservations() {
    let (_lock, admin, store, leases, graph, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    graph
        .reserve(TENANT, PROJECT, "work-a", "prefix", "src/foo")
        .await
        .unwrap();
    let execution = prepare_default(&store, &claim, "prep-u").await;
    let delivery = store
        .claim_dispatch(TENANT, PROJECT)
        .await
        .unwrap()
        .unwrap();
    let root = std::env::temp_dir().join(format!("awr-p7-{}", delivery.execution_id));
    let runner = ReferenceRunner::new(&root);
    let outcome = runner.handle_delivery(&delivery, CrashPoint::AfterJournalBeforeEffect);
    assert!(outcome.unknown);
    store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &delivery.execution_id,
            "unknown",
            json!({"unknown_reason": "crash-after-journal"}),
            &[],
        )
        .await
        .unwrap();
    // The unacked delivery must NOT be claimable again (CR #41 P2-6).
    let again = store.claim_dispatch(TENANT, PROJECT).await.unwrap();
    assert!(again.is_none(), "unknown execution redispatched: {again:?}");
    let blocked = leases
        .claim(
            TENANT,
            PROJECT,
            &leases
                .start_session(TENANT, PROJECT, ACTOR, CLIENT, "c2", "main", "work-a")
                .await
                .unwrap()
                .id,
            ACTOR,
            CLIENT,
            "claim-2",
            60,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        blocked,
        PgError::ClaimHeld | PgError::RecoveryBlocked
    ));
    let _ = admin;
    let _ = execution;
}

// CR #41 P2-6: a cancelled execution is never dispatched again.
#[tokio::test]
async fn cancelled_execution_is_not_redispatched() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-c").await;
    store
        .cancel(TENANT, PROJECT, ACTOR, CLIENT, "cancel-1", &execution)
        .await
        .unwrap();
    let delivery = store.claim_dispatch(TENANT, PROJECT).await.unwrap();
    assert!(
        delivery.is_none(),
        "cancelled execution dispatched: {delivery:?}"
    );
}

#[tokio::test]
async fn duplicate_outbox_delivery_reuses_effect_key() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-d").await;
    let delivery = store
        .claim_dispatch(TENANT, PROJECT)
        .await
        .unwrap()
        .unwrap();
    let root = std::env::temp_dir().join(format!("awr-p7-d-{}", delivery.execution_id));
    let runner = ReferenceRunner::new(&root);
    let first = runner.handle_delivery(&delivery, CrashPoint::None);
    let second = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(first.effect_key, second.effect_key);
    assert_eq!(first.output_digest, second.output_digest);
    let _ = execution;
}

#[tokio::test]
async fn crash_before_journal_is_not_started_and_late_cancel_is_not_cancelled() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-crash").await;
    let delivery = store
        .claim_dispatch(TENANT, PROJECT)
        .await
        .unwrap()
        .unwrap();
    let root = std::env::temp_dir().join(format!("awr-p7-c-{}", delivery.execution_id));
    let runner = ReferenceRunner::new(&root);
    let outcome = runner.handle_delivery(&delivery, CrashPoint::BeforeJournal);
    assert!(!outcome.started);
    store
        .accept(TENANT, PROJECT, &execution, delivery.fence)
        .await
        .unwrap();
    store
        .start(TENANT, PROJECT, &execution, delivery.fence)
        .await
        .unwrap();
    store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d1"}),
            &["src/foo".into()],
        )
        .await
        .unwrap();
    // A cancel request after a real success must not erase the success.
    let record = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "cancelled",
            json!({}),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(record.state, "succeeded");
    assert!(record.cancel_requested);
}

#[tokio::test]
async fn observed_paths_outside_scope_are_rejected_and_uncontrolled_has_no_exactly_once() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let prepared = store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-s",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "uncontrolled",
            &["src/foo".into()],
            &json!([
                {"path": "src/foo/a.rs", "content": "a"},
                {"path": "README.md", "content": "leak"}
            ]),
        )
        .await
        .unwrap();
    assert!(!exactly_once_supported(&prepared.fencing_class));
    let delivery = store
        .claim_dispatch(TENANT, PROJECT)
        .await
        .unwrap()
        .unwrap();
    let root = std::env::temp_dir().join(format!("awr-p7-s-{}", prepared.id));
    let runner = ReferenceRunner::new(&root);
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert!(outcome.scope_violation);
    assert!(!outcome.exactly_once_supported);
    store
        .accept(TENANT, PROJECT, &prepared.id, delivery.fence)
        .await
        .unwrap();
    let err = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &prepared.id,
            "succeeded",
            json!({"output_digest": outcome.output_digest}),
            &outcome.observed_paths,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::ScopeExceeded));
}

// CR #41 P2-10: an ancestor of the declared scope is NOT inside it.
#[tokio::test]
async fn scope_check_is_directional_containment() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let prepared = store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-dir",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &writes_in_scope(),
        )
        .await
        .unwrap();
    store
        .accept(TENANT, PROJECT, &prepared.id, 1)
        .await
        .unwrap();
    let err = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &prepared.id,
            "succeeded",
            json!({"output_digest": "d"}),
            &["src".into()],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::ScopeExceeded), "got {err}");
}

#[tokio::test]
async fn stale_fence_is_rejected_and_agent_cannot_mint_trusted_receipts() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-f").await;
    let err = store
        .accept(TENANT, PROJECT, &execution, 9)
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::StaleFence));
    let err = store
        .report(
            TENANT,
            PROJECT,
            ACTOR,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({}),
            &[],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::Forbidden));
}

// CR #41 P1-2: after a REAL handoff, the old fence cannot start the
// execution that was prepared with it.
#[tokio::test]
async fn handoff_invalidates_the_prepared_executions_fence() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-h").await;
    let handed = leases
        .handoff(TENANT, PROJECT, &claim, ACTOR, OTHER, "client-b", "next")
        .await
        .unwrap();
    assert!(handed.fence > 1);
    let err = store
        .accept(TENANT, PROJECT, &execution, 1)
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::StaleFence), "got {err}");
    let record = store.get(TENANT, PROJECT, &execution).await.unwrap();
    assert_eq!(record.state, "prepared", "stale execution still started");
}

// CR #41 P2-7: receipt authority is bound to the delegated executor; the
// reconcile kind requires a system coordinator.
#[tokio::test]
async fn receipt_authority_is_bound_and_uniform() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-a").await;
    // An agent cannot use the reconcile receipt kind at all.
    let err = store
        .report(
            TENANT,
            PROJECT,
            ACTOR,
            "reconcile",
            &execution,
            "succeeded",
            json!({}),
            &[],
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, PgError::Forbidden),
        "agent reconcile: got {err}"
    );
    // A system actor that is NOT the delegated executor cannot report
    // trusted_executor either.
    let err = store
        .report(
            TENANT,
            PROJECT,
            "runner-b",
            "trusted_executor",
            &execution,
            "succeeded",
            json!({}),
            &[],
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, PgError::Forbidden),
        "wrong executor: got {err}"
    );
}

// CR #41 P2-8: a conflicting late report cannot overwrite a terminal fact;
// the audit receipt is kept.
#[tokio::test]
async fn conflicting_late_report_cannot_overwrite_terminal_state() {
    let (_lock, admin, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-l").await;
    store.accept(TENANT, PROJECT, &execution, 1).await.unwrap();
    store.start(TENANT, PROJECT, &execution, 1).await.unwrap();
    store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d1"}),
            &["src/foo".into()],
        )
        .await
        .unwrap();
    let err = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "failed",
            json!({"output_digest": "d2"}),
            &["src/foo".into()],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::Protocol(_)), "got {err}");
    let record = store.get(TENANT, PROJECT, &execution).await.unwrap();
    assert_eq!(record.state, "succeeded", "late report overwrote the fact");
    let audits: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.execution_receipts WHERE execution_id=$1",
            &[&execution],
        )
        .await
        .unwrap()
        .get(0);
    assert!(audits >= 2, "late conflicting report was not audited");
}

// CR #41 P2-9: reconcile must not lift the block while another unknown
// execution remains on the same work.
#[tokio::test]
async fn reconcile_keeps_block_while_other_unknowns_remain() {
    let (_lock, admin, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let e1 = prepare_default(&store, &claim, "prep-r1").await;
    let e2 = prepare_default(&store, &claim, "prep-r2").await;
    // Both effects began before uncertainty was observed. Once the first
    // unknown report lands, the recovery gate correctly refuses new starts.
    for id in [&e1, &e2] {
        store.accept(TENANT, PROJECT, id, 1).await.unwrap();
        store.start(TENANT, PROJECT, id, 1).await.unwrap();
    }
    for (id, req) in [(&e1, "u1"), (&e2, "u2")] {
        store
            .report(
                TENANT,
                PROJECT,
                RUNNER,
                "trusted_executor",
                id,
                "unknown",
                json!({"unknown_reason": req}),
                &[],
            )
            .await
            .unwrap();
    }
    let err = store
        .reconcile(TENANT, PROJECT, RUNNER, &e1, "succeeded", json!({}), true)
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::Protocol(_)), "got {err}");
    let blocked: bool = admin
        .query_one(
            "SELECT recovery_blocked FROM awr_team.work_runtime WHERE work_id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        blocked,
        "reconcile lifted the block with unknowns remaining"
    );
    // After BOTH settle, clearing is allowed.
    store
        .reconcile(TENANT, PROJECT, RUNNER, &e2, "failed", json!({}), false)
        .await
        .unwrap();
    store
        .reconcile(TENANT, PROJECT, RUNNER, &e1, "succeeded", json!({}), true)
        .await
        .unwrap();
    let blocked: bool = admin
        .query_one(
            "SELECT recovery_blocked FROM awr_team.work_runtime WHERE work_id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!blocked);
}

// CR #41 P2-11: lifecycle transitions emit ordered events; prepare replay
// does not duplicate them; receipts carry the committed revision.
#[tokio::test]
async fn execution_lifecycle_emits_events_and_revisioned_receipts() {
    let (_lock, admin, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-e").await;
    store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-e",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &writes_in_scope(),
        )
        .await
        .unwrap();
    store.accept(TENANT, PROJECT, &execution, 1).await.unwrap();
    store.start(TENANT, PROJECT, &execution, 1).await.unwrap();
    store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d"}),
            &["src/foo".into()],
        )
        .await
        .unwrap();
    let events: Vec<(String, i64)> = admin
        .query(
            "SELECT event_type, project_revision FROM awr_team.events
             WHERE project_id='project-a' AND event_type LIKE 'execution.%'
             ORDER BY project_revision",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    let claim_events: Vec<(String, i64)> = admin
        .query(
            "SELECT event_type, project_revision FROM awr_team.events
             WHERE project_id='project-a' AND event_type LIKE 'claim.%'
             ORDER BY project_revision",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    let mut all = claim_events;
    all.extend(events.clone());
    assert_eq!(
        events,
        vec![
            ("execution.prepared".to_string(), 2),
            ("execution.accepted".to_string(), 3),
            ("execution.running".to_string(), 4),
            ("execution.reported".to_string(), 5),
        ],
        "lifecycle events out of order (claim events: {all:?})"
    );
    let prepared_events: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.events WHERE event_type='execution.prepared'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(prepared_events, 1, "prepare replay duplicated an event");
    let revision: Option<i64> = admin
        .query_one(
            "SELECT committed_project_revision FROM awr_team.operations WHERE request_id='prep-e'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(revision, Some(2), "receipt lacks the committed revision");
}

// CR #41 P2-12: fence is a decimal string in records, outbox payloads and
// prepare results; legacy numeric receipts still replay.
#[tokio::test]
async fn fence_is_decimal_string_everywhere() {
    let (_lock, admin, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    // The execution fence comes from the CLAIM row (and its runtime mirror);
    // raise both after the claim exists.
    admin
        .execute(
            "UPDATE awr_team.claims SET fence=9007199254740993 WHERE id=$1",
            &[&claim],
        )
        .await
        .unwrap();
    admin
        .batch_execute(
            "UPDATE awr_team.work_runtime SET last_fence=9007199254740993 WHERE work_id='work-a'",
        )
        .await
        .unwrap();
    let prepared = store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-big",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &writes_in_scope(),
        )
        .await
        .unwrap();
    assert_eq!(prepared.fence, 9_007_199_254_740_993);
    let serialized = serde_json::to_value(&prepared).unwrap();
    assert_eq!(serialized["fence"], json!("9007199254740993"));
    let delivery = store
        .claim_dispatch(TENANT, PROJECT)
        .await
        .unwrap()
        .unwrap();
    let delivery_json = serde_json::to_value(&delivery).unwrap();
    assert_eq!(delivery_json["fence"], json!("9007199254740993"));
    let receipt: serde_json::Value = admin
        .query_one(
            "SELECT result_json FROM awr_team.operations WHERE request_id='prep-big'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(receipt["fence"], json!("9007199254740993"));
    // Legacy numeric receipt form still replays.
    admin
        .execute(
            "UPDATE awr_team.operations
             SET result_json = jsonb_set(result_json, '{fence}', '9007199254740993'::jsonb)
             WHERE request_id='prep-big'",
            &[],
        )
        .await
        .unwrap();
    let replay = store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-big",
            &claim,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &writes_in_scope(),
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.fence, prepared.fence);
}

#[tokio::test]
async fn receipts_are_append_only_for_the_app_role() {
    let (_lock, _, store, leases, _, db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-ro").await;
    store.accept(TENANT, PROJECT, &execution, 1).await.unwrap();
    store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d"}),
            &["src/foo".into()],
        )
        .await
        .unwrap();
    let app = common::app_client(&db).await;
    let update = app
        .execute(
            "UPDATE awr_team.execution_receipts SET receipt_kind='forged'",
            &[],
        )
        .await;
    assert!(update.is_err(), "app role modified a receipt");
    let delete = app
        .execute("DELETE FROM awr_team.execution_receipts", &[])
        .await;
    assert!(delete.is_err(), "app role deleted a receipt");
}

// ---------- Runner filesystem regressions (CR #41 P1-1, P2-3, P2-4) ----------

fn delivery_for(writes: serde_json::Value, scope: &[&str]) -> awr_team_pg::OutboxDelivery {
    delivery_named("exec-fs-1", writes, scope)
}

fn delivery_named(
    id: &str,
    writes: serde_json::Value,
    scope: &[&str],
) -> awr_team_pg::OutboxDelivery {
    delivery_on_work(id, "work-a", writes, scope)
}

fn delivery_on_work(
    id: &str,
    work_id: &str,
    writes: serde_json::Value,
    scope: &[&str],
) -> awr_team_pg::OutboxDelivery {
    awr_team_pg::OutboxDelivery {
        coordinator_epoch: "epoch-1".into(),
        outbox_id: format!("ob-{id}"),
        execution_id: id.into(),
        effect_key: id.into(),
        tenant_id: "tenant-a".into(),
        project_id: "project-a".into(),
        work_id: work_id.into(),
        scope_id: "main".into(),
        fence: 1,
        fencing_class: "hard_fence".into(),
        declared_scope: json!(scope),
        payload: json!({"writes": writes, "effect_key": "exec-fs-1"}),
        delivery_attempts: 1,
    }
}

// CR #41 P1-1: traversal, absolute paths and symlink escapes are refused
// BEFORE any side effect; the outside sentinel stays untouched.
#[tokio::test]
async fn runner_refuses_worktree_escapes_before_writing() {
    let base = std::env::temp_dir().join(format!("awr-p7-boundary-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let sentinel = outside.join("sentinel.txt");
    std::fs::write(&sentinel, "original").unwrap();
    // traversal
    let runner = ReferenceRunner::new(base.join("r1"));
    let delivery = delivery_named(
        "exec-traversal",
        json!([{"path": "../../outside/sentinel.txt", "content": "tampered"}]),
        &["src"],
    );
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(outcome.state, "failed");
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "original");
    // absolute path (independent root + execution id so it cannot ride on
    // the previous journal)
    let runner = ReferenceRunner::new(base.join("r2"));
    let delivery = delivery_named(
        "exec-absolute",
        json!([{"path": sentinel.to_string_lossy(), "content": "tampered"}]),
        &["src"],
    );
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(outcome.state, "failed");
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "original");
}

// Positive control for confined writes; real file effects are unix-only
// (CR #58 r7).
#[cfg(unix)]
#[tokio::test]
async fn runner_writes_in_scope_unix() {
    let base = std::env::temp_dir().join(format!("awr-p7-inscope-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let outcome = runner.handle_delivery(
        &delivery_named(
            "exec-ok",
            json!([{"path": "src/ok.txt", "content": "ok"}]),
            &["src"],
        ),
        CrashPoint::None,
    );
    assert_eq!(outcome.state, "succeeded");
    assert_eq!(
        std::fs::read_to_string(base.join("worktree/src/ok.txt")).unwrap(),
        "ok"
    );
}

// Symlink escapes (directory-level AND file-level) are refused; the outside
// file stays untouched. Unix-only scenario (CR #58 P2-9).
#[cfg(unix)]
#[tokio::test]
async fn runner_refuses_symlink_escapes_unix() {
    let base = std::env::temp_dir().join(format!("awr-p7-links-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let sentinel = outside.join("sentinel.txt");
    std::fs::write(&sentinel, "original").unwrap();
    // directory-level symlink: worktree/src -> outside
    let root = base.join("r3");
    let worktree = root.join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::os::unix::fs::symlink(&outside, worktree.join("src")).unwrap();
    let runner = ReferenceRunner::new(&root);
    let delivery = delivery_named(
        "exec-dirsymlink",
        json!([{"path": "src/sentinel.txt", "content": "tampered"}]),
        &["src"],
    );
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(
        outcome.state, "failed",
        "dir symlink not refused: {outcome:?}"
    );
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "original");
    // FILE-level symlink: worktree/src is real, the target file links outside
    let root = base.join("r5");
    let file_dir = root.join("worktree/src");
    std::fs::create_dir_all(&file_dir).unwrap();
    std::os::unix::fs::symlink(&sentinel, file_dir.join("target.txt")).unwrap();
    let runner = ReferenceRunner::new(&root);
    let delivery = delivery_named(
        "exec-filesymlink",
        json!([{"path": "src/target.txt", "content": "tampered"}]),
        &["src"],
    );
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(
        outcome.state, "failed",
        "file symlink not refused: {outcome:?}"
    );
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "original");
}

// CR #41 P2-4: a failed write is reported as failed with the error, never
// as succeeded; only real writes are observed.
#[tokio::test]
async fn runner_propagates_write_failures() {
    let base = std::env::temp_dir().join(format!("awr-p7-io-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // Make the target path a DIRECTORY so fs::write must fail.
    let worktree = base.join("worktree");
    std::fs::create_dir_all(worktree.join("src/out.txt")).unwrap();
    let runner = ReferenceRunner::new(&base);
    let delivery = delivery_for(json!([{"path": "src/out.txt", "content": "x"}]), &["src"]);
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(outcome.state, "failed", "got {outcome:?}");
    assert!(outcome.error.is_some(), "write failure not recorded");
    assert!(outcome.observed_paths.is_empty());
}

// CR #41 P2-3: a corrupt journal is unknown, never "never executed".
#[tokio::test]
async fn runner_treats_corrupt_journal_as_unknown_not_unexecuted() {
    let base = std::env::temp_dir().join(format!("awr-p7-journal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let journal_dir = base.join("journal");
    std::fs::create_dir_all(&journal_dir).unwrap();
    std::fs::write(journal_dir.join("exec-fs-1.json"), "{corrupted").unwrap();
    let runner = ReferenceRunner::new(&base);
    let delivery = delivery_for(json!([{"path": "src/a.txt", "content": "x"}]), &["src"]);
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert!(
        outcome.unknown,
        "corrupt journal was re-executed: {outcome:?}"
    );
    assert!(
        !base.join("worktree/src/a.txt").exists(),
        "effects ran despite corrupt journal"
    );
}

// CR #58 P1: resource-end fencing — a stale delivery (older fence) must not
// overwrite a newer result.
#[cfg(unix)]
#[tokio::test]
async fn runner_rejects_stale_fencing_tokens() {
    let base = std::env::temp_dir().join(format!("awr-p7-fence-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let mut newer = delivery_named(
        "exec-new",
        json!([{"path": "src/out.txt", "content": "new"}]),
        &["src"],
    );
    newer.fence = 2;
    let outcome = runner.handle_delivery(&newer, CrashPoint::None);
    assert_eq!(outcome.state, "succeeded");
    let mut stale = delivery_named(
        "exec-old",
        json!([{"path": "src/out.txt", "content": "old"}]),
        &["src"],
    );
    stale.fence = 1;
    let outcome = runner.handle_delivery(&stale, CrashPoint::None);
    assert_eq!(outcome.state, "failed", "stale fence accepted: {outcome:?}");
    let content = std::fs::read_to_string(base.join("worktree/src/out.txt")).unwrap();
    assert_eq!(content, "new", "stale execution overwrote the newer result");
}

// CR #58 P2-3: a non-terminal journal means the previous handler died; the
// delivery goes to the unknown recovery path and effects do not re-run.
#[tokio::test]
async fn runner_recovers_crashed_journal_as_unknown() {
    let base = std::env::temp_dir().join(format!("awr-p7-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let delivery = delivery_named(
        "exec-crash",
        json!([{"path": "src/a.txt", "content": "x"}]),
        &["src"],
    );
    // Simulate a crash after the accepted journal but before the final record.
    let mut accepted = runner.base_outcome(&delivery, "accepted");
    accepted.state = "accepted".into();
    std::fs::create_dir_all(base.join("journal")).unwrap();
    std::fs::write(
        base.join("journal/exec-crash.json"),
        serde_json::to_vec(&accepted).unwrap(),
    )
    .unwrap();
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert!(
        outcome.unknown,
        "crashed journal not recovered as unknown: {outcome:?}"
    );
    assert!(
        !base.join("worktree/src/a.txt").exists(),
        "effects re-ran for a crashed journal"
    );
}

// CR #58 P2-4: a mid-plan failure records complete writes, touched files,
// and never reports the attempt as "not executed".
#[cfg(unix)]
#[tokio::test]
async fn runner_records_partial_side_effects_honestly() {
    let base = std::env::temp_dir().join(format!("awr-p7-partial-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // Second target is a directory: first write lands, second must fail.
    std::fs::create_dir_all(base.join("worktree/src/blocked.txt")).unwrap();
    let runner = ReferenceRunner::new(&base);
    let outcome = runner.handle_delivery(
        &delivery_named(
            "exec-partial",
            json!([
                {"path": "src/good.txt", "content": "ok"},
                {"path": "src/blocked.txt", "content": "x"}
            ]),
            &["src"],
        ),
        CrashPoint::None,
    );
    assert_eq!(outcome.state, "failed");
    assert!(outcome.started, "partial execution reported as not started");
    assert_eq!(outcome.observed_paths, vec!["src/good.txt".to_string()]);
    assert_eq!(outcome.partial_paths, vec!["src/blocked.txt".to_string()]);
}

// CR #58 P2-5: same-outcome reports with diverging facts cannot rewrite the
// terminal result, and out-of-scope observed paths cannot flip it to failed.
#[tokio::test]
async fn terminal_same_outcome_is_semantic_idempotency() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-ti").await;
    store.accept(TENANT, PROJECT, &execution, 1).await.unwrap();
    store.start(TENANT, PROJECT, &execution, 1).await.unwrap();
    store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d1"}),
            &["src/foo".into()],
        )
        .await
        .unwrap();
    // Identical replay: allowed, no error.
    store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d1"}),
            &["src/foo".into()],
        )
        .await
        .unwrap();
    // Diverging digest: audited, refused, digest stays d1.
    let err = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d2"}),
            &["src/foo".into()],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::Protocol(_)), "got {err}");
    // Out-of-scope paths on a terminal execution: also refused, NOT failed.
    let err = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d1"}),
            &["src".into()],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::Protocol(_)), "got {err}");
    let record = store.get(TENANT, PROJECT, &execution).await.unwrap();
    assert_eq!(record.state, "succeeded");
}

// CR #58 P2-6: report-time scope checks reject parent components even
// inside the declared prefix.
#[tokio::test]
async fn report_scope_rejects_parent_components() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let prepared = prepare_default(&store, &claim, "prep-dd").await;
    store.accept(TENANT, PROJECT, &prepared, 1).await.unwrap();
    let err = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &prepared,
            "succeeded",
            json!({"output_digest": "d"}),
            &["src/foo/../bar/a.rs".into()],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::ScopeExceeded), "got {err}");
}

// CR #58 P2-7: same-state transitions are idempotent (no duplicate events),
// and the scope-fail path emits its lifecycle event exactly once.
#[tokio::test]
async fn lifecycle_events_cover_early_exits_and_stay_idempotent() {
    let (_lock, admin, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-ev").await;
    store.accept(TENANT, PROJECT, &execution, 1).await.unwrap();
    store.accept(TENANT, PROJECT, &execution, 1).await.unwrap();
    let accepted_events: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.events WHERE event_type='execution.accepted'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(accepted_events, 1, "repeated accept emitted again");
    // Scope-fail report emits execution.reported for the failed outcome.
    store.start(TENANT, PROJECT, &execution, 1).await.unwrap();
    let err = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "succeeded",
            json!({"output_digest": "d"}),
            &["src".into()],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::ScopeExceeded));
    let reported: Vec<String> = admin
        .query(
            "SELECT payload_json->>'outcome' FROM awr_team.events WHERE event_type='execution.reported'",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(
        reported,
        vec!["failed".to_string()],
        "early exit event missing: {reported:?}"
    );
}

// CR #58 P2-8: nested outbox payload and cancel receipt fences are strings.
#[tokio::test]
async fn nested_fences_are_decimal_strings() {
    let (_lock, admin, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-nest").await;
    let delivery = store
        .claim_dispatch(TENANT, PROJECT)
        .await
        .unwrap()
        .unwrap();
    assert!(
        delivery.payload["fence"].is_string(),
        "outbox payload fence is not a string: {}",
        delivery.payload["fence"]
    );
    let stored: serde_json::Value = admin
        .query_one(
            "SELECT payload_json FROM awr_team.outbox WHERE aggregate_id=$1",
            &[&execution],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        stored["fence"].is_string(),
        "stored outbox fence is not a string"
    );
    store
        .cancel(TENANT, PROJECT, ACTOR, CLIENT, "cancel-nest", &execution)
        .await
        .unwrap();
    let receipt: serde_json::Value = admin
        .query_one(
            "SELECT result_json FROM awr_team.operations WHERE request_id='cancel-nest'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        receipt["fence"].is_string(),
        "cancel receipt fence is not a string"
    );
}

// CR #58 r3 P2: the fence ledger is PER WORK — a lower fence on a different
// work is fine, the same work's older fence is refused.
#[cfg(unix)]
#[tokio::test]
async fn runner_fence_ledger_is_scoped_per_work() {
    let base = std::env::temp_dir().join(format!("awr-p7-fenceperwork-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let mut a2 = delivery_on_work(
        "exec-a2",
        "work-a",
        json!([{"path": "src/a.txt", "content": "a2"}]),
        &["src"],
    );
    a2.fence = 2;
    assert_eq!(
        runner.handle_delivery(&a2, CrashPoint::None).state,
        "succeeded"
    );
    // Different work, unrelated counter: fence 1 is fine.
    let mut b1 = delivery_on_work(
        "exec-b1",
        "work-b",
        json!([{"path": "src/b.txt", "content": "b1"}]),
        &["src"],
    );
    b1.fence = 1;
    let outcome = runner.handle_delivery(&b1, CrashPoint::None);
    assert_eq!(
        outcome.state, "succeeded",
        "different work falsely refused: {outcome:?}"
    );
    // Same work, older fence: refused, and the newer result stays.
    let mut a1 = delivery_on_work(
        "exec-a1",
        "work-a",
        json!([{"path": "src/a.txt", "content": "a1"}]),
        &["src"],
    );
    a1.fence = 1;
    let outcome = runner.handle_delivery(&a1, CrashPoint::None);
    assert_eq!(outcome.state, "failed");
    assert_eq!(
        std::fs::read_to_string(base.join("worktree/src/a.txt")).unwrap(),
        "a2"
    );
}

// CR #58 r3 P2: a corrupt fence ledger refuses side effects instead of
// pretending the token state is unknown-safe.
#[tokio::test]
async fn runner_refuses_side_effects_with_corrupt_fence_ledger() {
    let base = std::env::temp_dir().join(format!("awr-p7-fencecorrupt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("journal/fencing")).unwrap();
    let key = awr_team_pg::fence_key("tenant-a", "project-a", "main", "work-a");
    std::fs::write(
        base.join("journal/fencing").join(format!("ledger-{key}")),
        "not-a-number",
    )
    .unwrap();
    let runner = ReferenceRunner::new(&base);
    let delivery = delivery_on_work(
        "exec-cf",
        "work-a",
        json!([{"path": "src/x.txt", "content": "x"}]),
        &["src"],
    );
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(
        outcome.state, "failed",
        "corrupt ledger allowed effects: {outcome:?}"
    );
    assert!(outcome.error.is_some());
    assert!(!base.join("worktree/src/x.txt").exists());
}

// CR #58 r3 P1: a held fence lock means another handler is mid-effects; the
// delivery must wait/fail, not write past it.
#[tokio::test]
async fn runner_does_not_write_past_a_held_fence_lock() {
    let base = std::env::temp_dir().join(format!("awr-p7-fencelock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("journal/fencing")).unwrap();
    let key = awr_team_pg::fence_key("tenant-a", "project-a", "main", "work-a");
    let lock_path = base.join("journal/fencing").join(format!("lock-{key}"));
    let holder = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    std::fs::File::lock(&holder).unwrap();
    let runner = ReferenceRunner::new(&base);
    let delivery = delivery_on_work(
        "exec-busy",
        "work-a",
        json!([{"path": "src/x.txt", "content": "x"}]),
        &["src"],
    );
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(
        outcome.state, "failed",
        "wrote past a held fence lock: {outcome:?}"
    );
    assert!(!base.join("worktree/src/x.txt").exists());
}

// CR #58 r3 P2-4: a duplicate delivery seeing an in-flight journal with a
// held recovery lock returns it UNCHANGED (no death declaration, no
// overwrite of a possibly-completed record).
#[tokio::test]
async fn duplicate_delivery_does_not_overwrite_an_inflight_journal() {
    let base = std::env::temp_dir().join(format!("awr-p7-dup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("journal")).unwrap();
    let runner = ReferenceRunner::new(&base);
    let delivery = delivery_named(
        "exec-dup",
        json!([{"path": "src/a.txt", "content": "x"}]),
        &["src"],
    );
    let mut inflight = runner.base_outcome(&delivery, "accepted");
    inflight.state = "accepted".into();
    let journal = base.join("journal/exec-dup.json");
    std::fs::write(&journal, serde_json::to_vec(&inflight).unwrap()).unwrap();
    // A recovery lock is held by another handler.
    std::fs::create_dir_all(base.join("journal/locks")).unwrap();
    let holder = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(base.join("journal/locks/exec-dup.lock"))
        .unwrap();
    std::fs::File::lock(&holder).unwrap();
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(
        outcome.state, "accepted",
        "in-flight journal was disturbed: {outcome:?}"
    );
    assert!(!outcome.unknown);
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
    assert_eq!(persisted["state"], "accepted", "journal was overwritten");
}

// CR #58 r3 P2-4: terminal records discovered during recovery win over any
// stale local copy (no overwrite of a completed result).
#[tokio::test]
async fn recovery_never_overwrites_a_terminal_journal() {
    let base = std::env::temp_dir().join(format!("awr-p7-term-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("journal")).unwrap();
    let runner = ReferenceRunner::new(&base);
    let delivery = delivery_named("exec-term", json!([]), &["src"]);
    let mut terminal = runner.base_outcome(&delivery, "succeeded");
    terminal.state = "succeeded".into();
    terminal.started = true;
    std::fs::write(
        base.join("journal/exec-term.json"),
        serde_json::to_vec(&terminal).unwrap(),
    )
    .unwrap();
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(outcome.state, "succeeded");
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(base.join("journal/exec-term.json")).unwrap())
            .unwrap();
    assert_eq!(
        persisted["state"], "succeeded",
        "terminal journal was overwritten"
    );
}

// CR #58 r3 P2-5: a terminal state reached via reconcile (no observed
// paths) must not panic the same-outcome report branch; it is a structured
// refusal, not a crash.
#[tokio::test]
async fn same_outcome_report_with_null_observed_facts_is_safe() {
    let (_lock, _, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-null").await;
    store
        .reconcile(
            TENANT,
            PROJECT,
            RUNNER,
            &execution,
            "failed",
            json!({}),
            false,
        )
        .await
        .unwrap();
    let err = store
        .report(
            TENANT,
            PROJECT,
            RUNNER,
            "trusted_executor",
            &execution,
            "failed",
            json!({"output_digest": "d"}),
            &["src/foo".into()],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::Protocol(_)), "got {err}");
}

// CR #58 r4 P2-3: same work, different scope = different counters.
#[cfg(unix)]
#[tokio::test]
async fn runner_fence_ledger_distinguishes_scopes() {
    let base = std::env::temp_dir().join(format!("awr-p7-scopelock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let mut main_high = delivery_on_work(
        "exec-m7",
        "work-a",
        json!([{"path": "src/m.txt", "content": "m7"}]),
        &["src"],
    );
    main_high.fence = 7;
    main_high.scope_id = "main".into();
    assert_eq!(
        runner.handle_delivery(&main_high, CrashPoint::None).state,
        "succeeded"
    );
    let mut review_low = delivery_on_work(
        "exec-r1",
        "work-a",
        json!([{"path": "src/r.txt", "content": "r1"}]),
        &["src"],
    );
    review_low.fence = 1;
    review_low.scope_id = "review".into();
    let outcome = runner.handle_delivery(&review_low, CrashPoint::None);
    assert_eq!(
        outcome.state, "succeeded",
        "review scope falsely refused: {outcome:?}"
    );
}

// CR #58 r4 P2-3: raw-id/suffix collisions cannot collide ledgers and locks.
#[cfg(unix)]
#[tokio::test]
async fn fence_files_never_collide_across_identities() {
    let base = std::env::temp_dir().join(format!("awr-p7-namecollide-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let mut task_lock = delivery_on_work(
        "exec-tl",
        "task.lock",
        json!([{"path": "src/tl.txt", "content": "tl"}]),
        &["src"],
    );
    task_lock.fence = 1;
    assert_eq!(
        runner.handle_delivery(&task_lock, CrashPoint::None).state,
        "succeeded"
    );
    let mut task = delivery_on_work(
        "exec-t",
        "task",
        json!([{"path": "src/t.txt", "content": "t"}]),
        &["src"],
    );
    task.fence = 1;
    let outcome = runner.handle_delivery(&task, CrashPoint::None);
    assert_eq!(
        outcome.state, "succeeded",
        "identity collision: {outcome:?}"
    );
}

// CR #58 r4 P1: a plan with a KNOWN-illegal destination must fail before
// ANY file is written.
#[cfg(unix)]
#[tokio::test]
async fn runner_rejects_known_illegal_plan_before_any_write() {
    let base = std::env::temp_dir().join(format!("awr-p7-planreject-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let outside = base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let sentinel = outside.join("sentinel.txt");
    std::fs::write(&sentinel, "original").unwrap();
    let file_dir = base.join("worktree/src");
    std::fs::create_dir_all(&file_dir).unwrap();
    std::os::unix::fs::symlink(&sentinel, file_dir.join("bad.txt")).unwrap();
    let runner = ReferenceRunner::new(&base);
    let outcome = runner.handle_delivery(
        &delivery_named(
            "exec-planreject",
            json!([
                {"path": "src/good.txt", "content": "first"},
                {"path": "src/bad.txt", "content": "tampered"}
            ]),
            &["src"],
        ),
        CrashPoint::None,
    );
    assert_eq!(outcome.state, "failed");
    assert!(
        !base.join("worktree/src/good.txt").exists(),
        "a known-illegal plan still wrote the first file"
    );
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "original");
}

// CR #58 r5 P2: identities containing the separator character get distinct
// ledgers (structured digest keys).
#[cfg(unix)]
#[tokio::test]
async fn fence_keys_are_unambiguous_for_separator_bearing_identities() {
    let base = std::env::temp_dir().join(format!("awr-p7-sepkey-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let mut a = delivery_on_work(
        "exec-sep-a",
        "task",
        json!([{"path": "src/a.txt", "content": "a"}]),
        &["src"],
    );
    a.scope_id = "main|aux".into();
    a.fence = 7;
    assert_eq!(
        runner.handle_delivery(&a, CrashPoint::None).state,
        "succeeded"
    );
    // ("main", "aux|task") must NOT share ("main|aux", "task")'s ledger.
    let b = delivery_on_work(
        "exec-sep-b",
        "aux|task",
        json!([{"path": "src/b.txt", "content": "b"}]),
        &["src"],
    );
    let outcome = runner.handle_delivery(&b, CrashPoint::None);
    assert_eq!(
        outcome.state, "succeeded",
        "separator identity collided: {outcome:?}"
    );
}

// CR #58 r5 P2-3 + r6 P2-1: legacy outbox rows (payload without
// tenant/project/scope) recover the FULL identity from the execution row —
// including a non-main scope — landing in the same fencing namespace as new
// entries.
#[tokio::test]
async fn legacy_outbox_payload_keeps_row_identity() {
    let (_lock, admin, store, leases, _, _db) = setup().await;
    let (_session, claim) = claimed(&leases).await;
    let execution = prepare_default(&store, &claim, "prep-legacy").await;
    // Simulate a pre-upgrade row: strip the identity fields from the payload.
    admin
        .execute(
            "UPDATE awr_team.outbox SET payload_json = payload_json - 'tenant_id' - 'project_id'
             WHERE aggregate_id=$1",
            &[&execution],
        )
        .await
        .unwrap();
    let delivery = store
        .claim_dispatch(TENANT, PROJECT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(delivery.tenant_id, TENANT);
    assert_eq!(delivery.project_id, PROJECT);
    assert_eq!(delivery.scope_id, "main");
    assert_eq!(delivery.work_id, "work-a");
}

// CR #58 r6 P2-1: a legacy payload from a NON-main execution recovers the
// real scope from the execution row instead of guessing main.
#[tokio::test]
async fn legacy_outbox_payload_recovers_the_real_scope() {
    let (_lock, admin, store, leases, _, _db) = setup().await;
    admin
        .batch_execute(
            "INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
             VALUES ('tenant-a','project-a','review','review','active');",
        )
        .await
        .unwrap();
    let session = leases
        .start_session(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "conv-review",
            "review",
            "work-a",
        )
        .await
        .unwrap();
    let claim = leases
        .claim(
            TENANT,
            PROJECT,
            &session.id,
            ACTOR,
            CLIENT,
            "claim-review",
            3600,
        )
        .await
        .unwrap();
    let execution = store
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            CLIENT,
            "prep-review",
            &claim.id,
            RUNNER,
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &writes_in_scope(),
        )
        .await
        .unwrap()
        .id;
    admin
        .execute(
            "UPDATE awr_team.outbox SET payload_json = payload_json - 'tenant_id' - 'project_id' - 'scope_id'
             WHERE aggregate_id=$1",
            &[&execution],
        )
        .await
        .unwrap();
    let delivery = store
        .claim_dispatch(TENANT, PROJECT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        delivery.scope_id, "review",
        "legacy scope recovered as main"
    );
    assert_eq!(delivery.work_id, "work-a");
}

// CR #58 r5 P2: concurrent handlers creating the same fresh directory both
// succeed (EEXIST on mkdirat reopens safely).
#[cfg(unix)]
#[tokio::test]
async fn concurrent_directory_creation_both_succeed() {
    let base = std::env::temp_dir().join(format!("awr-p7-mkdirrace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let base2 = base.clone();
    let first = std::thread::spawn(move || {
        let runner = ReferenceRunner::new(base);
        runner.handle_delivery(
            &delivery_on_work(
                "exec-mkdir-a",
                "work-a",
                json!([{"path": "src/new/a.txt", "content": "a"}]),
                &["src"],
            ),
            CrashPoint::None,
        )
    });
    let second = std::thread::spawn(move || {
        let runner = ReferenceRunner::new(base2);
        runner.handle_delivery(
            &delivery_on_work(
                "exec-mkdir-b",
                "work-b",
                json!([{"path": "src/new/b.txt", "content": "b"}]),
                &["src"],
            ),
            CrashPoint::None,
        )
    });
    let (a, b) = (first.join().unwrap(), second.join().unwrap());
    assert_eq!(a.state, "succeeded", "a failed: {a:?}");
    assert_eq!(b.state, "succeeded", "b failed: {b:?}");
}

// CR #58 r5 P3: BeforeJournal means NO admission record was written; a
// normal redelivery then proceeds to execute.
#[cfg(unix)]
#[tokio::test]
async fn before_journal_crash_leaves_no_admission_record() {
    let base = std::env::temp_dir().join(format!("awr-p7-beforejournal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let delivery = delivery_named(
        "exec-bj",
        json!([{"path": "src/a.txt", "content": "x"}]),
        &["src"],
    );
    let outcome = runner.handle_delivery(&delivery, CrashPoint::BeforeJournal);
    assert_eq!(outcome.state, "prepared");
    assert!(
        !base.join("journal/exec-bj.json").exists(),
        "BeforeJournal still wrote an admission record"
    );
    let outcome = runner.handle_delivery(&delivery, CrashPoint::None);
    assert_eq!(
        outcome.state, "succeeded",
        "redelivery did not execute: {outcome:?}"
    );
}

// CR #58 r6 P2-2: pure key semantics are platform-independent and stay
// cross-platform.
#[test]
fn fence_keys_are_deterministic_and_collision_free() {
    let a = awr_team_pg::fence_key("tenant", "project", "main|aux", "task");
    let b = awr_team_pg::fence_key("tenant", "project", "main", "aux|task");
    assert_ne!(a, b, "separator-bearing identities collided");
    assert_eq!(a.len(), 64, "key is a fixed-length hex digest");
    let long = awr_team_pg::fence_key("t".repeat(64).as_str(), "p", "s", "w");
    assert_eq!(long.len(), 64, "long identities must not lengthen the key");
}

// CR #58 r6 P2-2: on non-unix the protected write is REFUSED and business
// files stay untouched. Compiles everywhere; runs when a Windows/CI lane
// enables pg-tests (not covered by the default native verification lane,
// which runs without the pg-tests feature).
#[cfg(not(unix))]
#[tokio::test]
async fn non_unix_refuses_protected_writes_without_side_effects() {
    let base = std::env::temp_dir().join(format!("awr-p7-nonunix-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let runner = ReferenceRunner::new(&base);
    let outcome = runner.handle_delivery(
        &delivery_named(
            "exec-nonunix",
            json!([{"path": "src/x.txt", "content": "x"}]),
            &["src"],
        ),
        CrashPoint::None,
    );
    assert_eq!(outcome.state, "failed");
    assert!(outcome.error.is_some());
    assert!(!base.join("worktree/src/x.txt").exists());
}
