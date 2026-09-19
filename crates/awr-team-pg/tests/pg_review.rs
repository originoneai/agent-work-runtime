#![cfg(feature = "pg-tests")]
//! TEAM-P8 evidence/review/completion tests on real PostgreSQL.
//! Fixture isolation comes from tests/common (CR #42 P2-11).

use awr_team_pg::{PgError, ReviewStore};
use serde_json::json;
use std::sync::MutexGuard;
use tokio_postgres::Client;

mod common;
use common::{fresh_team_schema, test_config, with_app_role};

const TENANT: &str = "tenant-a";
const PROJECT: &str = "project-a";
const AUTHOR: &str = "actor-a";
const REVIEWER: &str = "actor-b";
const REVIEWER_C: &str = "actor-c";
const RUNNER: &str = "runner-a";
const DISABLED: &str = "actor-d";
const READER: &str = "actor-e";

async fn setup(policy: &str) -> (MutexGuard<'static, ()>, Client, ReviewStore) {
    let (guard, admin, db) = fresh_team_schema().await;
    admin
        .batch_execute(&format!(
            "INSERT INTO awr_team.tenants(id,name,status) VALUES ('tenant-a','A','active');
             INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status) VALUES
                ('tenant-a','actor-a','agent','A','active'),
                ('tenant-a','actor-b','human','B','active'),
                ('tenant-a','actor-c','human','C','active'),
                ('tenant-a','actor-d','human','D','disabled'),
                ('tenant-a','actor-e','human','E','active'),
                ('tenant-a','runner-a','system','Runner','active');
             INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status)
                VALUES ('tenant-a','project-a','alpha','team','epoch-1','active');
             INSERT INTO awr_team.project_memberships(tenant_id,project_id,actor_id,role) VALUES
                ('tenant-a','project-a','actor-b','reviewer'),
                ('tenant-a','project-a','actor-c','reviewer'),
                ('tenant-a','project-a','actor-d','reviewer'),
                ('tenant-a','project-a','actor-e','reader');
             INSERT INTO awr_team.source_snapshots(
                tenant_id, project_id, id, manifest_digest, source_ref_json, parser_version, created_by)
                VALUES ('tenant-a','project-a','snap-1','digest','{{}}','p1','actor-a');
             UPDATE awr_team.projects SET active_snapshot_id='snap-1' WHERE id='project-a';
             INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
                VALUES ('tenant-a','project-a','main','main','active');
             INSERT INTO awr_team.work_items(tenant_id,project_id,id,external_key) VALUES
                ('tenant-a','project-a','work-a','W'),
                ('tenant-a','project-a','work-b','X');
             INSERT INTO awr_team.work_contracts(
                tenant_id, project_id, snapshot_id, scope_id, work_id, contract_hash,
                definition_state, title, contract_json)
             VALUES
                ('tenant-a','project-a','snap-1','main','work-a','hash-a','enabled','W','{{\"completion_policy\":\"{policy}\",\"acceptance\":[\"a\"]}}'),
                ('tenant-a','project-a','snap-1','main','work-b','hash-b','enabled','X','{{\"completion_policy\":\"{policy}\",\"acceptance\":[\"b\"]}}');",
        ))
        .await
        .unwrap();
    let store = ReviewStore::from_config(with_app_role(&test_config(), &db));
    (guard, admin, store)
}

/// Trusted-grade evidence is recorded by the system runner (an agent's
/// record stays caller_asserted and can never satisfy the strict policy).
async fn evidence(
    store: &ReviewStore,
    hash: &str,
    payload: serde_json::Value,
    bytes: Option<&[u8]>,
    exec: Option<&str>,
) -> awr_team_pg::EvidenceRecord {
    store
        .record_evidence(
            TENANT,
            PROJECT,
            RUNNER,
            "work-a",
            hash,
            None,
            &payload,
            bytes,
            Some("in-1"),
            false,
            exec,
        )
        .await
        .unwrap()
}

/// Insert a succeeded execution row for trusted-executor evidence.
async fn succeeded_execution(admin: &Client, id: &str, contract_hash: &str) {
    admin
        .execute(
            "INSERT INTO awr_team.executions(
                tenant_id, project_id, id, work_id, session_id, claim_id, fence,
                contract_hash, input_digest, executor_actor_id, state, effect_key,
                fencing_class, declared_scope_json, scope_id)
             VALUES ('tenant-a','project-a',$1,'work-a',NULL,NULL,1,$2,'in-1','runner-a','succeeded',$1,'hard_fence','[]','main')",
            &[&id, &contract_hash],
        )
        .await
        .unwrap();
}

async fn approve(store: &ReviewStore, evidence_id: &str) {
    let round = store
        .open_review(TENANT, PROJECT, AUTHOR, "work-a", evidence_id)
        .await
        .unwrap();
    store
        .decide_review(TENANT, PROJECT, REVIEWER, &round.id, "approve", "ok")
        .await
        .unwrap();
}

async fn complete(
    store: &ReviewStore,
    evidence_id: &str,
    request: &str,
) -> awr_team_pg::PgResult<awr_team_pg::CompletionReceipt> {
    store
        .complete(
            TENANT,
            PROJECT,
            REVIEWER,
            "client-b",
            request,
            "work-a",
            "main",
            evidence_id,
            None,
            true,
        )
        .await
}

#[tokio::test]
async fn source_declared_completed_does_not_write_a_team_receipt() {
    let (_lock, _, store) = setup("trusted_execution_and_review").await;
    let receipts = store
        .source_declared_is_not_complete(TENANT, PROJECT, "work-a", "main")
        .await
        .unwrap();
    assert!(receipts);
}

// CR #42 P2-1: a trusted grade must bind a SUCCEEDED execution of the same
// contract and input; failure-admitting or unbound material cannot complete.
#[tokio::test]
async fn trusted_grade_requires_a_successful_bound_execution() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    // payload admits failure -> rejected even with an approval
    let failed = evidence(
        &store,
        "hash-a",
        json!({"passed": false}),
        Some(b"bytes"),
        None,
    )
    .await;
    approve(&store, &failed.id).await;
    let err = complete(&store, &failed.id, "c-fail").await.unwrap_err();
    assert!(matches!(err, PgError::EvidenceInvalid), "got {err}");
    // no execution binding -> rejected
    let unbound = evidence(&store, "hash-a", json!({"log": "ok"}), Some(b"bytes"), None).await;
    approve(&store, &unbound.id).await;
    let err = complete(&store, &unbound.id, "c-unbound")
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::EvidenceInvalid), "got {err}");
    // failed execution -> rejected
    let ev = evidence(&store, "hash-a", json!({"log": "ok"}), Some(b"bytes"), None).await;
    admin
        .execute(
            "UPDATE awr_team.evidence SET execution_id='exec-failed' WHERE id=$1",
            &[&ev.id],
        )
        .await
        .unwrap();
    admin
        .execute(
            "INSERT INTO awr_team.executions(
                tenant_id, project_id, id, work_id, session_id, claim_id, fence,
                contract_hash, input_digest, executor_actor_id, state, effect_key,
                fencing_class, declared_scope_json, scope_id)
             VALUES ('tenant-a','project-a','exec-failed','work-a',NULL,NULL,1,'hash-a','in-1','runner-a','failed','fx-failed','hard_fence','[]','main')",
            &[],
        )
        .await
        .unwrap();
    approve(&store, &ev.id).await;
    let err = complete(&store, &ev.id, "c-failedexec").await.unwrap_err();
    assert!(matches!(err, PgError::EvidenceInvalid), "got {err}");
    // succeeded execution with matching contract+input -> completes
    let good = evidence(
        &store,
        "hash-a",
        json!({"log": "ok"}),
        Some(b"bytes"),
        Some("exec-good"),
    )
    .await;
    succeeded_execution(&admin, "exec-good", "hash-a").await;
    approve(&store, &good.id).await;
    complete(&store, &good.id, "c-good").await.unwrap();
}

#[tokio::test]
async fn agent_cannot_upgrade_self_report_or_self_review() {
    let (_lock, _, store) = setup("trusted_execution_and_review").await;
    let evidence = store
        .record_evidence(
            TENANT,
            PROJECT,
            AUTHOR,
            "work-a",
            "hash-a",
            Some("trusted_executor"),
            &json!({"passed": true, "output_digest": "deadbeef"}),
            Some(b"report-bytes"),
            Some("in-1"),
            false,
            None,
        )
        .await
        .unwrap();
    assert_eq!(evidence.trust_basis, "caller_asserted");
    let err = complete(&store, &evidence.id, "c-self").await.unwrap_err();
    assert!(matches!(err, PgError::EvidenceInvalid));
    let round = store
        .open_review(TENANT, PROJECT, AUTHOR, "work-a", &evidence.id)
        .await
        .unwrap();
    let err = store
        .decide_review(TENANT, PROJECT, AUTHOR, &round.id, "approve", "self")
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::AuthorCannotReview));
}

// CR #42 P2-5: disabled accounts, non-members and reader-role members cannot
// review; a ghost author cannot open a round.
#[tokio::test]
async fn review_authority_requires_active_capable_membership() {
    let (_lock, _, store) = setup("trusted_execution_and_review").await;
    let evidence = evidence(&store, "hash-a", json!({"log": "x"}), Some(b"b"), None).await;
    let round = store
        .open_review(TENANT, PROJECT, AUTHOR, "work-a", &evidence.id)
        .await
        .unwrap();
    for (who, why) in [
        (DISABLED, "disabled account"),
        (READER, "reader role"),
        ("ghost", "nonexistent account"),
    ] {
        let err = store
            .decide_review(TENANT, PROJECT, who, &round.id, "approve", why)
            .await
            .unwrap_err();
        assert!(matches!(err, PgError::Forbidden), "{why}: got {err}");
    }
    let err = store
        .open_review(TENANT, PROJECT, "ghost-author", "work-a", &evidence.id)
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::Forbidden), "ghost author: got {err}");
    store
        .decide_review(TENANT, PROJECT, REVIEWER, &round.id, "approve", "ok")
        .await
        .unwrap();
}

#[tokio::test]
async fn trusted_receipt_with_independent_review_completes_and_old_contract_does_not() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    let old = evidence(
        &store,
        "hash-old",
        json!({"log": "old"}),
        Some(b"old-bytes"),
        None,
    )
    .await;
    let err = complete(&store, &old.id, "c-old").await.unwrap_err();
    assert!(matches!(err, PgError::EvidenceInvalid));
    let ev = evidence(
        &store,
        "hash-a",
        json!({"log": "ok"}),
        Some(b"good-bytes"),
        Some("exec-1"),
    )
    .await;
    succeeded_execution(&admin, "exec-1", "hash-a").await;
    approve(&store, &ev.id).await;
    let receipt = complete(&store, &ev.id, "c-ok").await.unwrap();
    let count: i64 = admin
        .query_one("SELECT count(*) FROM awr_team.completion_receipts", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
    // The receipt records the actual approver, not the submitter (CR #42 audit).
    let approved_by: serde_json::Value = admin
        .query_one(
            "SELECT approved_by_json FROM awr_team.completion_receipts WHERE id=$1",
            &[&receipt.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(approved_by["approved_by"], json!(REVIEWER));
    assert_eq!(approved_by["submitted_by"], json!(REVIEWER));
    // Real mapping rows exist.
    let mapped: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.completion_evidence WHERE completion_id=$1",
            &[&receipt.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(mapped, 1);
}

// CR #42 P2-3: a new contract never reuses an old approval, even when the
// payload/bytes are re-recorded identically.
#[tokio::test]
async fn new_contract_requires_fresh_review() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    let ev1 = evidence(
        &store,
        "hash-a",
        json!({"log": "same"}),
        Some(b"bytes"),
        Some("exec-1"),
    )
    .await;
    succeeded_execution(&admin, "exec-1", "hash-a").await;
    approve(&store, &ev1.id).await;
    complete(&store, &ev1.id, "c-first").await.unwrap();
    // Publish contract H2 for the same work.
    admin
        .batch_execute(
            "INSERT INTO awr_team.source_snapshots(
                tenant_id, project_id, id, manifest_digest, source_ref_json, parser_version, created_by)
             VALUES ('tenant-a','project-a','snap-2','digest2','{}','p1','actor-a');
             INSERT INTO awr_team.work_contracts(
                tenant_id, project_id, snapshot_id, scope_id, work_id, contract_hash,
                definition_state, title, contract_json)
             VALUES ('tenant-a','project-a','snap-2','main','work-a','hash-b','enabled','W',
                '{\"completion_policy\":\"trusted_execution_and_review\",\"acceptance\":[\"a2\"]}');
             UPDATE awr_team.projects SET active_snapshot_id='snap-2' WHERE id='project-a';",
        )
        .await
        .unwrap();
    // Same payload/bytes re-recorded under H2.
    let ev2 = evidence(
        &store,
        "hash-b",
        json!({"log": "same"}),
        Some(b"bytes"),
        Some("exec-2"),
    )
    .await;
    succeeded_execution(&admin, "exec-2", "hash-b").await;
    let err = complete(&store, &ev2.id, "c-second").await.unwrap_err();
    assert!(matches!(err, PgError::ReviewRequired), "got {err}");
    // After a fresh independent review of the H2 bundle, it completes.
    approve(&store, &ev2.id).await;
    complete(&store, &ev2.id, "c-second-2").await.unwrap();
}

// CR #42 P2-4: the pinned round and its decision never come from different
// rounds (cross-timing regression).
#[tokio::test]
async fn round_and_decision_stay_pinned() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    let ev = evidence(
        &store,
        "hash-a",
        json!({"log": "ok"}),
        Some(b"bytes"),
        Some("exec-1"),
    )
    .await;
    succeeded_execution(&admin, "exec-1", "hash-a").await;
    // R1 opened by AUTHOR, R2 opened by REVIEWER (same bundle).
    let r1 = store
        .open_review(TENANT, PROJECT, AUTHOR, "work-a", &ev.id)
        .await
        .unwrap();
    let r2 = store
        .open_review(TENANT, PROJECT, REVIEWER, "work-a", &ev.id)
        .await
        .unwrap();
    // R2 decided EARLY by AUTHOR... wait: AUTHOR is an agent, cannot review.
    // Use REVIEWER_C for R2's decision and REVIEWER for R1's (later).
    store
        .decide_review(
            TENANT,
            PROJECT,
            REVIEWER_C,
            &r2.id,
            "approve",
            "round two first",
        )
        .await
        .unwrap();
    store
        .decide_review(
            TENANT,
            PROJECT,
            REVIEWER,
            &r1.id,
            "approve",
            "round one later",
        )
        .await
        .unwrap();
    // The latest round R2 (author REVIEWER, approver REVIEWER_C) is a valid
    // independent review; the older round's later decision must not poison it.
    complete(&store, &ev.id, "c-rounds").await.unwrap();
    let _ = r1;
}

#[tokio::test]
async fn changed_bytes_and_material_invalidate_review_and_forbid_policy_downgrade() {
    let (_lock, _, store) = setup("trusted_execution_and_review").await;
    let first = evidence(
        &store,
        "hash-a",
        json!({"log": "a"}),
        Some(b"bytes-a"),
        None,
    )
    .await;
    let round = store
        .open_review(TENANT, PROJECT, AUTHOR, "work-a", &first.id)
        .await
        .unwrap();
    store
        .decide_review(TENANT, PROJECT, REVIEWER, &round.id, "approve", "ok")
        .await
        .unwrap();
    let second = evidence(
        &store,
        "hash-a",
        json!({"log": "b"}),
        Some(b"bytes-b"),
        None,
    )
    .await;
    store
        .open_review(TENANT, PROJECT, AUTHOR, "work-a", &second.id)
        .await
        .unwrap();
    let err = complete(&store, &first.id, "c-changed").await.unwrap_err();
    assert!(matches!(
        err,
        PgError::CompletionRejected | PgError::ReviewRequired | PgError::EvidenceInvalid
    ));
    let err = store
        .complete(
            TENANT,
            PROJECT,
            AUTHOR,
            "client-a",
            "c-down",
            "work-a",
            "main",
            &second.id,
            Some("ordinary_confirm"),
            true,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::PolicyDowngrade));
}

#[tokio::test]
async fn ordinary_confirm_works_for_humans_and_dirty_tree_needs_bytes() {
    let (_lock, _, store) = setup("ordinary_confirm").await;
    let err = store
        .record_evidence(
            TENANT,
            PROJECT,
            AUTHOR,
            "work-a",
            "hash-a",
            None,
            &json!({"passed": true}),
            None,
            None,
            true,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::EvidenceInvalid));
    let evidence = store
        .record_evidence(
            TENANT,
            PROJECT,
            REVIEWER,
            "work-a",
            "hash-a",
            None,
            &json!({"confirmed": true}),
            Some(b"checklist"),
            Some("tree-digest"),
            true,
            None,
        )
        .await
        .unwrap();
    store
        .complete(
            TENANT,
            PROJECT,
            REVIEWER,
            "client-b",
            "c-ordinary",
            "work-a",
            "main",
            &evidence.id,
            Some("ordinary_confirm"),
            true,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn completed_state_cannot_be_forged_without_a_receipt() {
    let (_lock, admin, _store) = setup("trusted_execution_and_review").await;
    let app = common::app_client(&common::gate_db_name().await).await;
    app.batch_execute("SELECT set_config('awr.tenant_id','tenant-a',false); SELECT set_config('awr.project_id','project-a',false);")
        .await
        .unwrap();
    // NULL selected id: the row-level check refuses.
    let err = app
        .execute(
            "INSERT INTO awr_team.work_runtime(
                tenant_id, project_id, scope_id, work_id, state, work_version, last_fence)
             VALUES ('tenant-a','project-a','main','work-a','completed',1,0)",
            &[],
        )
        .await;
    assert!(err.is_err());
    // CR #42 P2-7: a non-NULL but nonexistent receipt is refused by the FK.
    let err = app
        .execute(
            "INSERT INTO awr_team.work_runtime(
                tenant_id, project_id, scope_id, work_id, state, work_version, last_fence,
                selected_completion_id)
             VALUES ('tenant-a','project-a','main','work-a','completed',1,0,'no-such-receipt')",
            &[],
        )
        .await;
    assert!(err.is_err(), "fabricated receipt id accepted");
    let _ = admin;
}

// CR #42 P2-8: an explicit recovery block stops completion (and stays set).
#[tokio::test]
async fn recovery_block_stops_completion() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    let ev = evidence(
        &store,
        "hash-a",
        json!({"log": "ok"}),
        Some(b"bytes"),
        Some("exec-1"),
    )
    .await;
    succeeded_execution(&admin, "exec-1", "hash-a").await;
    approve(&store, &ev.id).await;
    admin
        .batch_execute(
            "INSERT INTO awr_team.work_runtime(
                tenant_id, project_id, scope_id, work_id, state, work_version, last_fence, recovery_blocked)
             VALUES ('tenant-a','project-a','main','work-a','active',1,1,TRUE);",
        )
        .await
        .unwrap();
    let err = complete(&store, &ev.id, "c-blocked").await.unwrap_err();
    assert!(matches!(err, PgError::RecoveryBlocked), "got {err}");
    let blocked: bool = admin
        .query_one(
            "SELECT recovery_blocked FROM awr_team.work_runtime WHERE work_id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(blocked, "completion cleared the explicit block");
}

// CR #42 P2-9: completing is idempotent per (client, request); the same
// request returns the original receipt, changed parameters conflict.
#[tokio::test]
async fn completion_is_idempotent_per_request() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    let ev = evidence(
        &store,
        "hash-a",
        json!({"log": "ok"}),
        Some(b"bytes"),
        Some("exec-1"),
    )
    .await;
    succeeded_execution(&admin, "exec-1", "hash-a").await;
    approve(&store, &ev.id).await;
    let first = complete(&store, &ev.id, "c-idem").await.unwrap();
    let replay = complete(&store, &ev.id, "c-idem").await.unwrap();
    assert_eq!(first.id, replay.id, "retry minted a new receipt");
    let selected: String = admin
        .query_one(
            "SELECT selected_completion_id FROM awr_team.work_runtime WHERE work_id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(selected, first.id, "retry replaced the selected receipt");
    let receipts: i64 = admin
        .query_one("SELECT count(*) FROM awr_team.completion_receipts", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(receipts, 1);
    let ev2 = evidence(
        &store,
        "hash-a",
        json!({"log": "other"}),
        Some(b"other"),
        Some("exec-2"),
    )
    .await;
    succeeded_execution(&admin, "exec-2", "hash-a").await;
    approve(&store, &ev2.id).await;
    let err = complete(&store, &ev2.id, "c-idem").await.unwrap_err();
    assert!(matches!(err, PgError::IdempotencyConflict), "got {err}");
}

// CR #42 P2-6: missing required-dependency coverage is not "valid".
#[tokio::test]
async fn missing_required_dependency_blocks_completion() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    admin
        .execute(
            "UPDATE awr_team.work_contracts
             SET contract_json = contract_json || '{\"required_dependencies\":[\"work-b\"]}'::jsonb
             WHERE work_id='work-a'",
            &[],
        )
        .await
        .unwrap();
    let ev = evidence(
        &store,
        "hash-a",
        json!({"log": "ok"}),
        Some(b"bytes"),
        Some("exec-1"),
    )
    .await;
    succeeded_execution(&admin, "exec-1", "hash-a").await;
    approve(&store, &ev.id).await;
    let err = complete(&store, &ev.id, "c-deps").await.unwrap_err();
    assert!(matches!(err, PgError::CompletionRejected), "got {err}");
}

// CR #42 P2-2: persisted artifact bytes round-trip and are re-verified at
// completion time.
#[tokio::test]
async fn artifact_bytes_round_trip_and_are_reverified() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    let content: &[u8] = "验收报告：全部通过".as_bytes();
    let ev = evidence(
        &store,
        "hash-a",
        json!({"log": "ok"}),
        Some(content),
        Some("exec-1"),
    )
    .await;
    succeeded_execution(&admin, "exec-1", "hash-a").await;
    // Read the bytes back from a FRESH connection (restart-style check).
    let fresh = common::app_client(&common::gate_db_name().await).await;
    fresh
        .batch_execute("SELECT set_config('awr.tenant_id','tenant-a',false); SELECT set_config('awr.project_id','project-a',false);")
        .await
        .unwrap();
    let stored: Vec<u8> = fresh
        .query_one(
            "SELECT content FROM awr_team.artifacts a
             JOIN awr_team.evidence e ON e.artifact_id=a.id WHERE e.id=$1",
            &[&ev.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(stored, content, "persisted bytes differ from the original");
    approve(&store, &ev.id).await;
    complete(&store, &ev.id, "c-bytes").await.unwrap();
    // Tamper with the persisted bytes: completion must now refuse.
    admin
        .execute(
            "UPDATE awr_team.artifacts SET content='tampered' WHERE id=(SELECT artifact_id FROM awr_team.evidence WHERE id=$1)",
            &[&ev.id],
        )
        .await
        .unwrap();
    admin
        .batch_execute("UPDATE awr_team.work_runtime SET state='active', selected_completion_id=NULL WHERE work_id='work-a'; DELETE FROM awr_team.completion_evidence; DELETE FROM awr_team.completion_dependencies; DELETE FROM awr_team.completion_receipts; DELETE FROM awr_team.operations;")
        .await
        .unwrap();
    let err = complete(&store, &ev.id, "c-tampered").await.unwrap_err();
    assert!(matches!(err, PgError::EvidenceInvalid), "got {err}");
}

// CR #42 P2-10: evidence/review/completion emit ordered events in the same
// transactions; replays do not duplicate them.
#[tokio::test]
async fn review_lifecycle_emits_events_atomically() {
    let (_lock, admin, store) = setup("trusted_execution_and_review").await;
    let ev = evidence(
        &store,
        "hash-a",
        json!({"log": "ok"}),
        Some(b"bytes"),
        Some("exec-1"),
    )
    .await;
    succeeded_execution(&admin, "exec-1", "hash-a").await;
    let round = store
        .open_review(TENANT, PROJECT, AUTHOR, "work-a", &ev.id)
        .await
        .unwrap();
    store
        .decide_review(TENANT, PROJECT, REVIEWER, &round.id, "approve", "ok")
        .await
        .unwrap();
    complete(&store, &ev.id, "c-events").await.unwrap();
    complete(&store, &ev.id, "c-events").await.unwrap();
    let events: Vec<String> = admin
        .query(
            "SELECT event_type FROM awr_team.events WHERE project_id='project-a' ORDER BY project_revision",
            &[],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(
        events,
        vec![
            "evidence.recorded".to_string(),
            "review.opened".to_string(),
            "review.decided".to_string(),
            "work.completed".to_string(),
        ],
        "unexpected event stream: {events:?}"
    );
}
