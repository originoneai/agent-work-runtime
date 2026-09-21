#![cfg(feature = "pg-tests")]

use awr_team_pg::{ImportStore, LeaseStore, PgError, check_schema};
use std::sync::MutexGuard;
use tokio_postgres::Client;
mod common;
use common::{connect_config, fresh_team_schema, test_config, with_app_role, with_db};
const TENANT: &str = "tenant-a";
const PROJECT: &str = "project-a";
async fn setup() -> (MutexGuard<'static, ()>, Client, String) {
    let (guard, admin, db) = fresh_team_schema().await;
    admin
        .batch_execute(
            "INSERT INTO awr_team.tenants(id,name,status) VALUES ('tenant-a','A','active');
             INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status) VALUES
                ('tenant-a','actor-a','agent','A','active'),
                ('tenant-a','actor-b','agent','B','active');
             INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status)
                VALUES ('tenant-a','project-a','alpha','team','epoch-1','active');
             INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
                VALUES ('tenant-a','project-a','main','main','active');
             INSERT INTO awr_team.work_items(tenant_id,project_id,id,external_key)
                VALUES ('tenant-a','project-a','work-a','W');",
        )
        .await
        .unwrap();
    (guard, admin, db)
}

#[tokio::test]
async fn two_clients_cannot_hold_the_same_claim() {
    let (_lock, admin, db) = setup().await;
    let left = LeaseStore::from_config(with_app_role(&test_config(), &db));
    let right = LeaseStore::from_config(with_app_role(&test_config(), &db));
    let s1 = left
        .start_session(TENANT, PROJECT, "actor-a", "c1", "conv-a", "main", "work-a")
        .await
        .unwrap();
    let s2 = right
        .start_session(TENANT, PROJECT, "actor-b", "c2", "conv-b", "main", "work-a")
        .await
        .unwrap();
    let a = left.claim(TENANT, PROJECT, &s1.id, "actor-a", "c1", "r1", 60);
    let b = right.claim(TENANT, PROJECT, &s2.id, "actor-b", "c2", "r2", 60);
    let (ra, rb) = tokio::join!(a, b);
    assert_eq!([&ra, &rb].iter().filter(|r| r.is_ok()).count(), 1);
    let active: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.claims WHERE state='active' AND work_id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(active, 1);
}

#[tokio::test]
async fn reconnect_after_drop_still_requires_matching_schema() {
    let (_lock, _, db) = setup().await;
    let client = connect_config(&with_db(&test_config(), &db)).await;
    check_schema(&client).await.unwrap();
    drop(client);
    let again = connect_config(&with_db(&test_config(), &db)).await;
    check_schema(&again).await.unwrap();
}

#[tokio::test]
async fn missing_backup_objects_fail_closed() {
    let (_lock, _, db) = setup().await;
    let store = ImportStore::from_config(with_app_role(&test_config(), &db));
    let err = store
        .backup(TENANT, PROJECT, &["missing-art".into()], &["src".into()])
        .await
        .unwrap_err();
    assert!(matches!(err, PgError::RestoreIncomplete));
}

#[tokio::test]
async fn reconnect_rejects_incompatible_or_missing_schema_without_changing_work() {
    let (_lock, admin, db) = setup().await;
    let before: serde_json::Value = admin
        .query_one(
            "SELECT to_jsonb(w) FROM awr_team.work_items w WHERE id='work-a'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let client = connect_config(&with_db(&test_config(), &db)).await;
    check_schema(&client).await.unwrap();
    drop(client);
    admin
        .batch_execute("UPDATE awr_team.schema_state SET version=999 WHERE component='awr_team'")
        .await
        .unwrap();
    for missing in [false, true] {
        if missing {
            admin
                .batch_execute("DELETE FROM awr_team.schema_state WHERE component='awr_team'")
                .await
                .unwrap();
        }
        let again = connect_config(&with_db(&test_config(), &db)).await;
        assert!(matches!(
            check_schema(&again).await,
            Err(PgError::SchemaIncompatible(_))
        ));
        let after: serde_json::Value = admin
            .query_one(
                "SELECT to_jsonb(w) FROM awr_team.work_items w WHERE id='work-a'",
                &[],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(before, after);
    }
}
