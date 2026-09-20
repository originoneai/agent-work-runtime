#![cfg(feature = "pg-tests")]

use awr_team_pg::{ImportStore, PgError};
use serde_json::json;
use std::sync::MutexGuard;
use tokio_postgres::Client;
mod common;
use common::{fresh_team_schema, test_config, with_app_role};
const TENANT: &str = "tenant-a";
const PROJECT: &str = "project-a";
const ACTOR: &str = "actor-a";
async fn setup() -> (MutexGuard<'static, ()>, Client, ImportStore) {
    let (guard, admin, db) = fresh_team_schema().await;
    admin
        .batch_execute(
            "INSERT INTO awr_team.tenants(id,name,status) VALUES ('tenant-a','A','active');
             INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status)
                VALUES ('tenant-a','actor-a','agent','A','active');
             INSERT INTO awr_team.credentials(tenant_id,id,actor_id,client_id,secret_hash)
                VALUES ('tenant-a','cred-1','actor-a','client-a','hash');
             INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status)
                VALUES ('tenant-a','project-a','alpha','team','epoch-1','active');
             INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
                VALUES ('tenant-a','project-a','main','main','active');",
        )
        .await
        .unwrap();
    (
        guard,
        admin,
        ImportStore::from_config(with_app_role(&test_config(), &db)),
    )
}

fn contract(id: &str, key: &str) -> serde_json::Value {
    json!({"codec":"awr-team-contract-v1","work_id":id,"external_key":key,"goals":[],"hard_rules":[],"scope_paths":["src"],"acceptance":["verified output"],"required_dependencies":[],"completion_policy":"ordinary_confirm","verification_requirements":[]})
}
fn manifest() -> serde_json::Value {
    let c = contract("work-a", "W");
    let hash = serde_json::from_value::<awr_team::WorkContract>(c.clone())
        .unwrap()
        .hash()
        .unwrap();
    json!({"format":"awr-team-import-v1","scopes":["main"],
        "works":[{"id":"work-a","external_key":"W","contract":c,"contract_hash":hash}],
        "evidence":[{"id":"ev-1","work_id":"work-a","claimed_trust":"trusted_executor","trust_basis":"trusted_executor","contract_hash":hash,"evidence_kind":"report","payload_json":{"note":"historical report"},"digest":"original","bytes_present":true}],
        "local_claims":[{"work_id":"work-a","state":"active"}]})
}

#[tokio::test]
async fn duplicate_import_is_idempotent_and_does_not_copy_local_claims() {
    let (_lock, admin, store) = setup().await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    let first = store
        .load(TENANT, PROJECT, ACTOR, "imp-1", &manifest())
        .await
        .unwrap();
    let again = store
        .load(TENANT, PROJECT, ACTOR, "imp-1", &manifest())
        .await
        .unwrap();
    assert!(again.replayed);
    assert_eq!(first.id, again.id);
    let works: i64 = admin
        .query_one("SELECT count(*) FROM awr_team.work_items", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(works, 1);
    store
        .activate(TENANT, PROJECT, &first.id, false)
        .await
        .unwrap();
    assert!(
        store
            .local_claim_is_not_team_lease(TENANT, PROJECT, "work-a")
            .await
            .unwrap()
    );
    let trust: String = admin
        .query_one(
            "SELECT trust_basis FROM awr_team.evidence WHERE id='ev-1'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(trust, "caller_asserted");
}

#[test]
fn source_divergence_is_not_resolved_by_mtime() {
    let store = ImportStore::new("postgres://unused");
    let err = store
        .inspect_sources(&[("a", "fp-1"), ("b", "fp-2")])
        .unwrap_err();
    assert!(matches!(err, PgError::SourceDivergence));
    store
        .inspect_sources(&[("a", "fp-1"), ("b", "fp-1")])
        .unwrap();
}

#[tokio::test]
async fn restore_isolates_old_epoch_and_does_not_replay_outbox() {
    let (_lock, admin, store) = setup().await;
    admin
        .batch_execute(
            "INSERT INTO awr_team.work_items(tenant_id,project_id,id,external_key)
             VALUES ('tenant-a','project-a','work-a','W');
             INSERT INTO awr_team.work_runtime(tenant_id,project_id,scope_id,work_id,state,work_version,last_fence)
             VALUES ('tenant-a','project-a','main','work-a','pending',1,0);
             INSERT INTO awr_team.outbox(tenant_id,project_id,id,state,payload_json)
             VALUES ('tenant-a','project-a','ob-1','pending','{}');",
        )
        .await
        .unwrap();
    let backup = store.backup(TENANT, PROJECT, &[], &[]).await.unwrap();
    let err = store
        .restore(TENANT, PROJECT, &backup.id, true, true)
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::OutboxReplayForbidden));
    let err = store
        .restore(TENANT, PROJECT, &backup.id, false, false)
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::RestoreIncomplete));
    let run = store
        .restore(TENANT, PROJECT, &backup.id, true, false)
        .await
        .unwrap();
    assert_ne!(run.new_epoch, backup.coordinator_epoch);
    store
        .require_epoch(TENANT, PROJECT, &backup.coordinator_epoch)
        .await
        .unwrap_err();
    store
        .require_epoch(TENANT, PROJECT, &run.new_epoch)
        .await
        .unwrap();
    let pending: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.outbox WHERE state='pending'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(pending, 0);
    let revoked: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.credentials WHERE revoked_at IS NOT NULL",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(revoked, 1);
}

#[tokio::test]
async fn new_team_writes_block_sqlite_rollback_and_unknown_blocks_activate() {
    let (_lock, admin, store) = setup().await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    let job = store
        .load(TENANT, PROJECT, ACTOR, "imp-2", &manifest())
        .await
        .unwrap();
    let err = store
        .activate(TENANT, PROJECT, &job.id, true)
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::RecoveryBlocked));
    store
        .activate(TENANT, PROJECT, &job.id, false)
        .await
        .unwrap();
    admin
        .batch_execute("UPDATE awr_team.projects SET project_revision=3 WHERE id='project-a'")
        .await
        .unwrap();
    let err = store
        .refuse_sqlite_rollback(TENANT, PROJECT)
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::RollbackForbidden));
}

#[tokio::test]
async fn dry_run_reports_missing_evidence_and_rejects_non_main_scope() {
    let store = ImportStore::new("postgres://unused");
    let mut m = manifest();
    m["evidence"][0]["bytes_present"] = json!(false);
    let report = store.dry_run(&m).unwrap();
    assert_eq!(report["can_activate"], json!(false));
    let err = store.dry_run(&json!({"scopes":["legacy"]})).unwrap_err();
    assert!(matches!(err, PgError::ScopeUnsupported));
}

async fn config(admin: &Client) -> tokio_postgres::Config {
    let db: String = admin
        .query_one("SELECT current_database()", &[])
        .await
        .unwrap()
        .get(0);
    with_app_role(&test_config(), &db)
}
async fn project_b(admin: &Client) {
    admin.batch_execute("INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status) VALUES('tenant-a','project-b','beta','team','epoch-b','active'); INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status) VALUES('tenant-a','project-b','main','main','active')").await.unwrap();
}
async fn activate_manifest(store: &ImportStore, project: &str, m: &serde_json::Value) -> String {
    store.freeze(TENANT, project).await.unwrap();
    let j = store
        .load(TENANT, project, ACTOR, "import", m)
        .await
        .unwrap();
    store.activate(TENANT, project, &j.id, false).await.unwrap();
    j.id
}
#[tokio::test]
async fn jobs_are_project_scoped_in_queries_constraints_and_rls() {
    let (_lock, admin, store) = setup().await;
    project_b(&admin).await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    store.freeze(TENANT, "project-b").await.unwrap();
    let a = store
        .load(TENANT, PROJECT, ACTOR, "same", &manifest())
        .await
        .unwrap();
    let b = store
        .load(TENANT, "project-b", ACTOR, "same", &manifest())
        .await
        .unwrap();
    assert_ne!(a.id, b.id);
    assert!(!b.replayed);
    assert!(
        store
            .activate(TENANT, "project-b", &a.id, false)
            .await
            .is_err()
    );
    let mut app = common::connect_config(&config(&admin).await).await;
    let tx = app.transaction().await.unwrap();
    tx.execute(
        "SELECT set_config('awr.tenant_id',$1,true),set_config('awr.project_id',$2,true)",
        &[&TENANT, &"project-b"],
    )
    .await
    .unwrap();
    let n: i64 = tx
        .query_one(
            "SELECT count(*) FROM awr_team.import_jobs WHERE id=$1",
            &[&a.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 0);
    assert!(
        tx.execute(
            "UPDATE awr_team.import_jobs SET state='activated' WHERE id=$1",
            &[&a.id]
        )
        .await
        .unwrap()
            == 0
    );
    tx.commit().await.unwrap();
    store.activate(TENANT, PROJECT, &a.id, false).await.unwrap();
    store
        .activate(TENANT, "project-b", &b.id, false)
        .await
        .unwrap();
}
#[tokio::test]
async fn direct_load_cannot_bypass_validation_or_missing_material_gate() {
    let (_lock, admin, store) = setup().await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    let mut m = manifest();
    m["scopes"] = json!(["other"]);
    assert!(matches!(
        store.load(TENANT, PROJECT, ACTOR, "bad-scope", &m).await,
        Err(PgError::ScopeUnsupported)
    ));
    m = manifest();
    m["evidence"][0].as_object_mut().unwrap().remove("work_id");
    assert!(
        store
            .load(TENANT, PROJECT, ACTOR, "no-work", &m)
            .await
            .is_err()
    );
    m = manifest();
    m["evidence"][0]["bytes_present"] = json!(false);
    let j = store
        .load(TENANT, PROJECT, ACTOR, "missing", &m)
        .await
        .unwrap();
    assert!(matches!(
        store.activate(TENANT, PROJECT, &j.id, false).await,
        Err(PgError::RestoreIncomplete)
    ));
    let row = admin
        .query_one(
            "SELECT state,report_json,manifest_json FROM awr_team.import_jobs WHERE id=$1",
            &[&j.id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "loaded");
    assert_eq!(row.get::<_, serde_json::Value>(1)["can_activate"], false);
    assert_eq!(row.get::<_, serde_json::Value>(2), m);
}
#[tokio::test]
async fn metadata_only_import_cannot_activate() {
    let (_lock, _, store) = setup().await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    let mut m = manifest();
    m["works"][0].as_object_mut().unwrap().remove("contract");
    let j = store
        .load(TENANT, PROJECT, ACTOR, "metadata", &m)
        .await
        .unwrap();
    assert!(matches!(
        store.activate(TENANT, PROJECT, &j.id, false).await,
        Err(PgError::RestoreIncomplete)
    ));
}
#[tokio::test]
async fn actual_unknown_execution_blocks_activation_despite_false_hint() {
    let (_lock, admin, store) = setup().await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    let j = store
        .load(TENANT, PROJECT, ACTOR, "unknown", &manifest())
        .await
        .unwrap();
    admin.batch_execute("INSERT INTO awr_team.executions(tenant_id,project_id,id,work_id,fence,contract_hash,executor_actor_id,state) VALUES('tenant-a','project-a','unknown','work-a',1,'hash','actor-a','unknown')").await.unwrap();
    assert!(matches!(
        store.activate(TENANT, PROJECT, &j.id, false).await,
        Err(PgError::RecoveryBlocked)
    ));
}
#[tokio::test]
async fn imported_contracts_prepare_and_evidence_roundtrips_without_trust_upgrade() {
    use sha2::{Digest, Sha256};
    let (_lock, admin, store) = setup().await;
    project_b(&admin).await;
    let mut m = manifest();
    m["works"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"work-b","external_key":"X","contract":contract("work-b","X")}));
    m["evidence"][0]["work_id"] = json!("work-b");
    m["evidence"][0]["payload_json"] =
        json!({"passed":true,"output_digest":"result-path-and-bytes","text":"报告正文"});
    m["evidence"][0]["artifact_bytes"] = json!(b"report-bytes".to_vec());
    m["evidence"][0]["output_digest"] = json!(format!("{:x}", Sha256::digest(b"report-bytes")));
    m["evidence"][0]["execution_result_digest"] = json!("result-path-and-bytes");
    activate_manifest(&store, PROJECT, &m).await;
    let read = awr_team_pg::ReadStore::from_config(config(&admin).await);
    read.prepare(TENANT, PROJECT, "work-b", None).await.unwrap();
    store.freeze(TENANT, PROJECT).await.unwrap();
    let exported = store.export(TENANT, PROJECT).await.unwrap();
    assert_eq!(exported["evidence"][0]["work_id"], "work-b");
    assert_eq!(
        exported["evidence"][0]["payload_json"],
        m["evidence"][0]["payload_json"]
    );
    activate_manifest(&store, "project-b", &exported).await;
    read.prepare(TENANT, "project-b", "work-b", None)
        .await
        .unwrap();
    let row=admin.query_one("SELECT e.work_id,e.trust_basis,e.payload_json,e.execution_id,e.execution_result_digest,a.content FROM awr_team.evidence e JOIN awr_team.artifacts a ON a.tenant_id=e.tenant_id AND a.project_id=e.project_id AND a.id=e.artifact_id WHERE e.project_id='project-b'",&[]).await.unwrap();
    assert_eq!(row.get::<_, String>(0), "work-b");
    assert_eq!(row.get::<_, String>(1), "caller_asserted");
    assert_eq!(
        row.get::<_, serde_json::Value>(2),
        m["evidence"][0]["payload_json"]
    );
    assert_eq!(row.get::<_, Option<String>>(3), None);
    assert_eq!(row.get::<_, String>(4), "result-path-and-bytes");
    assert_eq!(row.get::<_, Vec<u8>>(5), b"report-bytes");
}
#[tokio::test]
async fn freeze_blocks_real_writes_and_all_store_admission() {
    let (_lock, admin, store) = setup().await;
    activate_manifest(&store, PROJECT, &manifest()).await;
    let cfg = config(&admin).await;
    let team = awr_team_pg::TeamStore::from_config(cfg.clone());
    let req = awr_team_pg::CommandRequest {
        tenant_id: TENANT.into(),
        project_id: PROJECT.into(),
        actor_id: ACTOR.into(),
        client_id: "c".into(),
        request_id: "touch".into(),
        op: "work.touch".into(),
        args: json!({"work_id":"work-a","scope_id":"main"}),
    };
    team.execute(req.clone()).await.unwrap();
    store.freeze(TENANT, PROJECT).await.unwrap();
    let version: i64 = admin
        .query_one(
            "SELECT work_version FROM awr_team.work_runtime WHERE work_id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(matches!(
        team.execute(awr_team_pg::CommandRequest {
            request_id: "after-freeze".into(),
            ..req
        })
        .await,
        Err(PgError::ProjectNotAvailable)
    ));
    let leases = awr_team_pg::LeaseStore::from_config(cfg.clone());
    assert!(matches!(
        leases
            .start_session(TENANT, PROJECT, ACTOR, "c", "conv", "main", "work-a")
            .await,
        Err(PgError::ProjectNotAvailable)
    ));
    let source = awr_team_pg::SourceStore::from_config(cfg.clone());
    assert!(matches!(
        source
            .ingest(awr_team_pg::IngestRequest {
                tenant_id: TENANT.into(),
                project_id: PROJECT.into(),
                actor_id: ACTOR.into(),
                parser_version: "p1".into(),
                files: vec![awr_team_pg::SourceFile {
                    path: "contract.json".into(),
                    bytes: contract("work-a", "W").to_string().into_bytes()
                }]
            })
            .await,
        Err(PgError::ProjectNotAvailable)
    ));
    let review = awr_team_pg::ReviewStore::from_config(cfg);
    assert!(matches!(
        review
            .open_review(TENANT, PROJECT, ACTOR, "work-a", "ev-1")
            .await,
        Err(PgError::ProjectNotAvailable)
    ));
    assert_eq!(
        version,
        admin
            .query_one(
                "SELECT work_version FROM awr_team.work_runtime WHERE work_id='work-a'",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i64>(0)
    );
    let ex = store.export(TENANT, PROJECT).await.unwrap();
    assert!(ex["origin"]["project_revision"].is_string());
}
#[tokio::test]
async fn restored_accepted_execution_rejects_original_fence_and_revokes_session() {
    let (_lock, admin, store) = setup().await;
    activate_manifest(&store, PROJECT, &manifest()).await;
    let cfg = config(&admin).await;
    let leases = awr_team_pg::LeaseStore::from_config(cfg.clone());
    let exec = awr_team_pg::ExecutionStore::from_config(cfg);
    let session = leases
        .start_session(TENANT, PROJECT, ACTOR, "c", "conv", "main", "work-a")
        .await
        .unwrap();
    let claim = leases
        .claim(TENANT, PROJECT, &session.id, ACTOR, "c", "claim", 3600)
        .await
        .unwrap();
    let e = exec
        .prepare(
            TENANT,
            PROJECT,
            ACTOR,
            "c",
            "prep",
            &claim.id,
            ACTOR,
            "hash",
            "in",
            "hard_fence",
            &["src".into()],
            &json!([{ "path":"src/out","content":"old" }]),
        )
        .await
        .unwrap();
    exec.accept(TENANT, PROJECT, &e.id, e.fence).await.unwrap();
    let backup = store.backup(TENANT, PROJECT, &[], &[]).await.unwrap();
    let run = store
        .restore(TENANT, PROJECT, &backup.id, true, false)
        .await
        .unwrap();
    assert!(matches!(
        exec.start(TENANT, PROJECT, &e.id, e.fence).await,
        Err(PgError::RecoveryBlocked) | Err(PgError::StaleFence) | Err(PgError::EpochChanged)
    ));
    let row=admin.query_one("SELECT c.state,s.state,w.last_fence,e.state FROM awr_team.claims c JOIN awr_team.sessions s ON s.id=c.session_id JOIN awr_team.work_runtime w ON w.work_id=c.work_id JOIN awr_team.executions e ON e.claim_id=c.id WHERE c.id=$1",&[&claim.id]).await.unwrap();
    assert_eq!(row.get::<_, String>(0), "revoked");
    assert_eq!(row.get::<_, String>(1), "interrupted");
    assert!(row.get::<_, i64>(2) > e.fence);
    assert_eq!(row.get::<_, String>(3), "unknown");
    assert_eq!(run.fencing_barriers.len(), 1);
    assert!(run.fencing_barriers[0].fence > e.fence);
    // Even if a recovery administrator clears the work block, old epoch is inadmissible.
    admin
        .batch_execute("UPDATE awr_team.work_runtime SET recovery_blocked=FALSE")
        .await
        .unwrap();
    assert!(matches!(
        exec.start(TENANT, PROJECT, &e.id, e.fence).await,
        Err(PgError::EpochChanged)
    ));
}
#[tokio::test]
async fn backup_verifies_real_bytes_and_restore_detects_missing_or_drifted_objects() {
    use sha2::{Digest, Sha256};
    let (_lock, admin, store) = setup().await;
    let mut m = manifest();
    let bytes = b"report".to_vec();
    m["evidence"][0]["artifact_bytes"] = json!(bytes);
    m["evidence"][0]["output_digest"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    activate_manifest(&store, PROJECT, &m).await;
    assert!(matches!(
        store
            .backup(TENANT, PROJECT, &["nonexistent".into()], &[])
            .await,
        Err(PgError::RestoreIncomplete)
    ));
    let b = store.backup(TENANT, PROJECT, &[], &[]).await.unwrap();
    admin
        .batch_execute("UPDATE awr_team.artifacts SET content=NULL")
        .await
        .unwrap();
    assert!(matches!(
        store.restore(TENANT, PROJECT, &b.id, true, false).await,
        Err(PgError::RestoreIncomplete)
    ));
    assert_eq!(
        admin
            .query_one("SELECT status FROM awr_team.projects", &[])
            .await
            .unwrap()
            .get::<_, String>(0),
        "degraded"
    );
    assert_eq!(
        admin
            .query_one(
                "SELECT count(*) FROM awr_team.restore_runs WHERE state='blocked'",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    admin
        .execute("UPDATE awr_team.artifacts SET content=$1", &[&bytes])
        .await
        .unwrap();
    admin
        .batch_execute("UPDATE awr_team.source_snapshots SET source_ref_json='{}'")
        .await
        .unwrap();
    assert!(matches!(
        store.restore(TENANT, PROJECT, &b.id, true, false).await,
        Err(PgError::RestoreIncomplete)
    ));
    admin
        .execute(
            "UPDATE awr_team.source_snapshots SET source_ref_json=$1",
            &[&m],
        )
        .await
        .unwrap();
    admin
        .batch_execute("UPDATE awr_team.backups SET schema_version=1")
        .await
        .unwrap();
    assert!(matches!(
        store.restore(TENANT, PROJECT, &b.id, true, false).await,
        Err(PgError::RestoreIncomplete)
    ));
    admin
        .execute(
            "UPDATE awr_team.backups SET schema_version=$1",
            &[&awr_team_pg::EXPECTED_SCHEMA_VERSION],
        )
        .await
        .unwrap();
    store
        .restore(TENANT, PROJECT, &b.id, true, false)
        .await
        .unwrap();
    assert_eq!(
        admin
            .query_one(
                "SELECT count(*) FROM awr_team.restore_runs WHERE state='completed'",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}
#[tokio::test]
async fn lifecycle_events_are_atomic_and_import_replay_does_not_duplicate() {
    let (_lock, admin, store) = setup().await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    let j = store
        .load(TENANT, PROJECT, ACTOR, "events", &manifest())
        .await
        .unwrap();
    store
        .load(TENANT, PROJECT, ACTOR, "events", &manifest())
        .await
        .unwrap();
    store.activate(TENANT, PROJECT, &j.id, false).await.unwrap();
    let b = store.backup(TENANT, PROJECT, &[], &[]).await.unwrap();
    store
        .restore(TENANT, PROJECT, &b.id, true, false)
        .await
        .unwrap();
    let rows = admin
        .query(
            "SELECT event_type,project_revision FROM awr_team.events ORDER BY project_revision",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r.get::<_, String>(0))
            .collect::<Vec<_>>(),
        vec![
            "project.frozen",
            "import.loaded",
            "import.activated",
            "backup.recorded",
            "restore.completed"
        ]
    );
    assert_eq!(
        rows.iter().map(|r| r.get::<_, i64>(1)).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    admin.batch_execute("CREATE FUNCTION awr_team.reject_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'event failure'; END $$; CREATE TRIGGER reject_event BEFORE INSERT ON awr_team.events FOR EACH ROW EXECUTE FUNCTION awr_team.reject_event()").await.unwrap();
    assert!(store.freeze(TENANT, PROJECT).await.is_err());
    let row = admin
        .query_one(
            "SELECT status,project_revision FROM awr_team.projects WHERE id='project-a'",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "active");
    assert_eq!(row.get::<_, i64>(1), 5);
    assert!(
        store
            .restore(TENANT, PROJECT, &b.id, true, false)
            .await
            .is_err()
    );
    assert_eq!(
        admin
            .query_one("SELECT count(*) FROM awr_team.restore_runs", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}

#[cfg(unix)]
#[test]
fn resource_barrier_rejects_already_delivered_old_command() {
    let root = std::env::temp_dir().join(format!("awr-restore-barrier-{}", common::nonce(0)));
    let runner = awr_team_pg::ReferenceRunner::new(&root);
    let barrier = awr_team_pg::FencingBarrier {
        tenant_id: TENANT.into(),
        project_id: PROJECT.into(),
        scope_id: "main".into(),
        work_id: "work-a".into(),
        fence: 8,
    };
    runner.install_recovery_barrier(&barrier).unwrap();
    let delivery = awr_team_pg::OutboxDelivery {
        tenant_id: TENANT.into(),
        project_id: PROJECT.into(),
        scope_id: "main".into(),
        work_id: "work-a".into(),
        fence: 7,
        outbox_id: "ob".into(),
        execution_id: "old-exec".into(),
        effect_key: "old-effect".into(),
        fencing_class: "hard_fence".into(),
        declared_scope: json!(["src"]),
        payload: json!({"writes":[{"path":"src/out.txt","content":"must not land"}]}),
        delivery_attempts: 1,
    };
    let out = runner.handle_delivery(&delivery, awr_team_pg::CrashPoint::None);
    assert_ne!(out.state, "succeeded");
    assert!(!root.join("worktree/src/out.txt").exists());
    assert!(out.error.unwrap().contains("stale fencing"));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn export_reads_one_snapshot_despite_concurrent_committed_evidence_change() {
    let (_lock, admin, store) = setup().await;
    activate_manifest(&store, PROJECT, &manifest()).await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    let sync = tokio::sync::Barrier::new(2);
    let export = store.export_with_sync_point(TENANT, PROJECT, &sync);
    let mutate = async {
        sync.wait().await;
        admin
            .batch_execute(
                "UPDATE awr_team.evidence SET payload_json='{\"note\":\"changed concurrently\"}'",
            )
            .await
            .unwrap();
        sync.wait().await;
    };
    let (result, ()) = tokio::join!(export, mutate);
    assert_eq!(
        result.unwrap()["evidence"][0]["payload_json"],
        manifest()["evidence"][0]["payload_json"]
    );
}
#[tokio::test]
async fn tampered_projection_or_validation_report_cannot_activate() {
    let (_lock, admin, store) = setup().await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    let m = manifest();
    let j = store
        .load(TENANT, PROJECT, ACTOR, "tampered", &m)
        .await
        .unwrap();
    admin
        .batch_execute("UPDATE awr_team.import_jobs SET report_json='{}'")
        .await
        .unwrap();
    assert!(matches!(
        store.activate(TENANT, PROJECT, &j.id, false).await,
        Err(PgError::RestoreIncomplete)
    ));
    admin
        .execute(
            "UPDATE awr_team.import_jobs SET report_json=$1",
            &[&store.dry_run(&m).unwrap()],
        )
        .await
        .unwrap();
    admin
        .batch_execute("UPDATE awr_team.work_contracts SET contract_hash='corrupt'")
        .await
        .unwrap();
    assert!(matches!(
        store.activate(TENANT, PROJECT, &j.id, false).await,
        Err(PgError::InactiveCandidate)
    ));
    let hash = m["works"][0]["contract_hash"].as_str().unwrap();
    admin
        .execute(
            "UPDATE awr_team.work_contracts SET contract_hash=$1",
            &[&hash],
        )
        .await
        .unwrap();
    admin
        .batch_execute("DELETE FROM awr_team.evidence")
        .await
        .unwrap();
    assert!(matches!(
        store.activate(TENANT, PROJECT, &j.id, false).await,
        Err(PgError::EvidenceInvalid)
    ));
    assert_eq!(
        admin
            .query_one("SELECT active_snapshot_id FROM awr_team.projects", &[])
            .await
            .unwrap()
            .get::<_, Option<String>>(0),
        None
    );
}
#[tokio::test]
async fn event_failure_rolls_back_load_and_activate() {
    let (_lock, admin, store) = setup().await;
    store.freeze(TENANT, PROJECT).await.unwrap();
    admin.batch_execute("CREATE FUNCTION awr_team.reject_import_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type LIKE 'import.%' THEN RAISE EXCEPTION 'event failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_import_event BEFORE INSERT ON awr_team.events FOR EACH ROW EXECUTE FUNCTION awr_team.reject_import_event()").await.unwrap();
    assert!(
        store
            .load(TENANT, PROJECT, ACTOR, "events", &manifest())
            .await
            .is_err()
    );
    for table in ["import_jobs", "work_items", "evidence", "source_snapshots"] {
        assert_eq!(
            admin
                .query_one(&format!("SELECT count(*) FROM awr_team.{table}"), &[])
                .await
                .unwrap()
                .get::<_, i64>(0),
            0
        );
    }
    admin
        .batch_execute("ALTER TABLE awr_team.events DISABLE TRIGGER reject_import_event")
        .await
        .unwrap();
    let j = store
        .load(TENANT, PROJECT, ACTOR, "events", &manifest())
        .await
        .unwrap();
    admin
        .batch_execute("ALTER TABLE awr_team.events ENABLE TRIGGER reject_import_event")
        .await
        .unwrap();
    assert!(store.activate(TENANT, PROJECT, &j.id, false).await.is_err());
    assert_eq!(
        admin
            .query_one(
                "SELECT status,active_snapshot_id FROM awr_team.projects",
                &[]
            )
            .await
            .unwrap()
            .get::<_, String>(0),
        "importing"
    );
    assert_eq!(
        admin
            .query_one("SELECT state FROM awr_team.import_jobs", &[])
            .await
            .unwrap()
            .get::<_, String>(0),
        "loaded"
    );
}
#[tokio::test]
async fn source_ingest_bytes_are_available_to_backup_integrity_checks() {
    let (_lock, admin, store) = setup().await;
    let source = awr_team_pg::SourceStore::from_config(config(&admin).await);
    source
        .ingest(awr_team_pg::IngestRequest {
            tenant_id: TENANT.into(),
            project_id: PROJECT.into(),
            actor_id: ACTOR.into(),
            parser_version: "p1".into(),
            files: vec![awr_team_pg::SourceFile {
                path: "contract.json".into(),
                bytes: contract("work-a", "W").to_string().into_bytes(),
            }],
        })
        .await
        .unwrap();
    let backup = store.backup(TENANT, PROJECT, &[], &[]).await.unwrap();
    store
        .restore(TENANT, PROJECT, &backup.id, true, false)
        .await
        .unwrap();
    // Individually self-consistent file metadata must still match the manifest.
    let row = admin
        .query_one("SELECT source_ref_json FROM awr_team.source_snapshots", &[])
        .await
        .unwrap();
    let mut altered: serde_json::Value = row.get(0);
    use sha2::{Digest, Sha256};
    altered["files"][0]["text"] = json!("different");
    altered["files"][0]["bytes"] = json!(9);
    altered["files"][0]["sha256"] = json!(format!("{:x}", Sha256::digest(b"different")));
    admin
        .execute(
            "UPDATE awr_team.source_snapshots SET source_ref_json=$1",
            &[&altered],
        )
        .await
        .unwrap();
    assert!(matches!(
        store.backup(TENANT, PROJECT, &[], &[]).await,
        Err(PgError::RestoreIncomplete)
    ));
}
#[tokio::test]
async fn migration_quarantines_unbound_legacy_jobs_and_refuses_new_null_jobs() {
    let (_lock, admin, _) = setup().await;
    // This schema belongs exclusively to fresh_team_schema's process database.
    admin
        .batch_execute("DROP SCHEMA awr_team CASCADE")
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/20260917000001_init.sql"),
        include_str!("../migrations/20260918000002_session_wait.sql"),
        include_str!("../migrations/20260918000003_graph_resources.sql"),
        include_str!("../migrations/20260918000004_execution_protocol.sql"),
        include_str!("../migrations/20260918000005_review_completion.sql"),
        include_str!("../migrations/20260918000006_import_restore.sql"),
        include_str!("../migrations/20260919000007_completion_integrity.sql"),
        include_str!("../migrations/20260920000008_execution_result_binding.sql"),
    ] {
        admin.batch_execute(sql).await.unwrap();
    }
    admin.batch_execute("INSERT INTO awr_team.import_jobs(tenant_id,id,import_key,manifest_hash,state,report_json) VALUES('tenant-a','legacy','k','h','loaded','{}')").await.unwrap();
    awr_team_pg::migrate(&admin).await.unwrap();
    awr_team_pg::Bootstrap::grant_app(&admin, "awr_app")
        .await
        .unwrap();
    assert_eq!(
        admin
            .query_one(
                "SELECT count(*) FROM awr_team.import_jobs WHERE project_id IS NULL",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    assert!(admin.batch_execute("INSERT INTO awr_team.import_jobs(tenant_id,id,import_key,manifest_hash,state,report_json) VALUES('tenant-a','new','new','h','loaded','{}')").await.is_err());
    let app = common::connect_config(&config(&admin).await).await;
    app.batch_execute("SELECT set_config('awr.tenant_id','tenant-a',false),set_config('awr.project_id','project-a',false)").await.unwrap();
    assert_eq!(
        app.query_one("SELECT count(*) FROM awr_team.import_jobs", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
}

#[tokio::test]
async fn evidence_binding_drift_and_cross_project_backup_fail_closed() {
    let (_lock, admin, store) = setup().await;
    project_b(&admin).await;
    assert!(matches!(
        store.export(TENANT, PROJECT).await,
        Err(PgError::ProjectNotAvailable)
    ));
    store.freeze(TENANT, PROJECT).await.unwrap();
    let j = store
        .load(TENANT, PROJECT, ACTOR, "drift", &manifest())
        .await
        .unwrap();
    admin
        .batch_execute("UPDATE awr_team.evidence SET input_digest='changed'")
        .await
        .unwrap();
    assert!(matches!(
        store.activate(TENANT, PROJECT, &j.id, false).await,
        Err(PgError::EvidenceInvalid)
    ));
    admin
        .batch_execute("UPDATE awr_team.evidence SET input_digest=NULL")
        .await
        .unwrap();
    store.activate(TENANT, PROJECT, &j.id, false).await.unwrap();
    let b = store.backup(TENANT, PROJECT, &[], &[]).await.unwrap();
    assert!(matches!(
        store.restore(TENANT, "project-b", &b.id, true, false).await,
        Err(PgError::RestoreIncomplete)
    ));
    assert_eq!(
        admin
            .query_one(
                "SELECT coordinator_epoch FROM awr_team.projects WHERE id='project-b'",
                &[]
            )
            .await
            .unwrap()
            .get::<_, String>(0),
        "epoch-b"
    );
}
