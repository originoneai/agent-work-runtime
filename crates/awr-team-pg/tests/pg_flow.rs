#![cfg(feature = "pg-tests")]

use awr_team_pg::{Bootstrap, ExecutionStore, LeaseStore, PgError, ReviewStore, migrate};
use serde_json::json;
use std::sync::{Mutex, MutexGuard};
use tokio_postgres::{Client, NoTls};

static DB: Mutex<()> = Mutex::new(());
const TENANT: &str = "tenant-a";
const PROJECT: &str = "project-a";

fn admin_url() -> String {
    std::env::var("AWR_TEAM_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:awr-test@127.0.0.1:55432/awr_team_test".into())
}
fn app_url() -> String {
    admin_url().replacen("postgres:awr-test", "awr_app:app-test", 1)
}
async fn connect(url: &str) -> Client {
    let (client, connection) = tokio_postgres::connect(url, NoTls)
        .await
        .expect("postgres 17 must be running for TEAM-P11");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

async fn setup() -> (
    MutexGuard<'static, ()>,
    Client,
    LeaseStore,
    LeaseStore,
    ExecutionStore,
    ReviewStore,
) {
    let guard = DB.lock().unwrap_or_else(|e| e.into_inner());
    let admin = connect(&admin_url()).await;
    admin
        .batch_execute("DROP SCHEMA IF EXISTS awr_team CASCADE")
        .await
        .unwrap();
    migrate(&admin).await.unwrap();
    admin
        .batch_execute(
            "DO $$ BEGIN CREATE ROLE awr_app LOGIN PASSWORD 'app-test' NOSUPERUSER NOBYPASSRLS; EXCEPTION WHEN duplicate_object THEN NULL; END $$",
        )
        .await
        .unwrap();
    Bootstrap::grant_app(&admin, "awr_app").await.unwrap();
    admin
        .batch_execute(
            r#"
INSERT INTO awr_team.tenants(id,name,status) VALUES ('tenant-a','A','active');
INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status) VALUES
   ('tenant-a','actor-a','agent','A','active'),
   ('tenant-a','actor-b','agent','B','active'),
   ('tenant-a','reviewer-a','human','R','active'),
   ('tenant-a','runner-a','system','Runner','active');
INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status)
   VALUES ('tenant-a','project-a','alpha','team','epoch-1','active');
INSERT INTO awr_team.project_memberships(tenant_id,project_id,actor_id,role)
   VALUES ('tenant-a','project-a','reviewer-a','reviewer');
INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
   VALUES ('tenant-a','project-a','main','main','active');
INSERT INTO awr_team.work_items(tenant_id,project_id,id,external_key)
   VALUES ('tenant-a','project-a','work-a','W');
INSERT INTO awr_team.source_snapshots(
   tenant_id, project_id, id, manifest_digest, source_ref_json, parser_version, created_by)
   VALUES ('tenant-a','project-a','snap-1','digest','{}','p1','actor-a');
UPDATE awr_team.projects SET active_snapshot_id='snap-1' WHERE id='project-a';
INSERT INTO awr_team.work_contracts(
   tenant_id, project_id, snapshot_id, scope_id, work_id, contract_hash,
   definition_state, title, contract_json)
   VALUES ('tenant-a','project-a','snap-1','main','work-a','hash-a','enabled','W',
           '{"completion_policy":"trusted_execution_and_review","acceptance":["done"]}');
"#,
        )
        .await
        .unwrap();
    (
        guard,
        admin,
        LeaseStore::new(app_url()),
        LeaseStore::new(app_url()),
        ExecutionStore::new(app_url()),
        ReviewStore::new(app_url()),
    )
}

#[tokio::test]
async fn two_actors_handoff_review_and_complete_with_independent_oracle() {
    let (_lock, admin, left, right, exec, review) = setup().await;
    let session = left
        .start_session(TENANT, PROJECT, "actor-a", "c1", "conv-a", "main", "work-a")
        .await
        .unwrap();
    let claim = left
        .claim(TENANT, PROJECT, &session.id, "actor-a", "c1", "claim-a", 60)
        .await
        .unwrap();
    let prepared = exec
        .prepare(
            TENANT,
            PROJECT,
            "actor-a",
            "c1",
            "prep-1",
            &claim.id,
            "runner-a",
            "hash-a",
            "in-1",
            "hard_fence",
            &["src/foo".into()],
            &json!([{"path":"src/foo/a.rs","content":"ok"}]),
        )
        .await
        .unwrap();
    exec.accept(TENANT, PROJECT, &prepared.id, claim.fence)
        .await
        .unwrap();
    exec.start(TENANT, PROJECT, &prepared.id, claim.fence)
        .await
        .unwrap();
    exec.report(
        TENANT,
        PROJECT,
        "runner-a",
        "trusted_executor",
        &prepared.id,
        "succeeded",
        json!({"output_digest": "deadbeef", "environment_digest": "env"}),
        &["src/foo/a.rs".into()],
    )
    .await
    .unwrap();
    let handed = left
        .handoff(
            TENANT, PROJECT, &claim.id, "actor-a", "actor-b", "c2", "conv-b",
        )
        .await
        .unwrap();
    let err = left
        .require_fence(TENANT, PROJECT, "main", "work-a", "actor-a", claim.fence)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        PgError::StaleFence | PgError::LeaseExpired | PgError::Forbidden
    ));
    right
        .require_fence(TENANT, PROJECT, "main", "work-a", "actor-b", handed.fence)
        .await
        .unwrap();
    let evidence = review
        .record_evidence(
            TENANT,
            PROJECT,
            "runner-a",
            "work-a",
            "hash-a",
            None,
            &json!({"log": "tested"}),
            Some(b"oracle-bytes"),
            Some("in-1"),
            false,
            Some(&prepared.id),
        )
        .await
        .unwrap();
    let round = review
        .open_review(TENANT, PROJECT, "actor-b", "work-a", &evidence.id)
        .await
        .unwrap();
    let err = review
        .decide_review(TENANT, PROJECT, "actor-b", &round.id, "approve", "self")
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::AuthorCannotReview));
    review
        .decide_review(TENANT, PROJECT, "reviewer-a", &round.id, "approve", "ok")
        .await
        .unwrap();
    review
        .complete(
            TENANT,
            PROJECT,
            "reviewer-a",
            "client-reviewer",
            "flow-complete-1",
            "work-a",
            "main",
            &evidence.id,
            None,
            true,
        )
        .await
        .unwrap();

    let receipts: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.completion_receipts WHERE work_id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let active: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.claims WHERE work_id='work-a' AND state='active'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let executions: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.executions WHERE work_id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let holder: String = admin
        .query_one(
            "SELECT actor_id FROM awr_team.claims WHERE work_id='work-a' AND state='active'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(receipts, 1);
    assert_eq!(active, 1);
    assert_eq!(executions, 1);
    assert_eq!(holder, "actor-b");
}
