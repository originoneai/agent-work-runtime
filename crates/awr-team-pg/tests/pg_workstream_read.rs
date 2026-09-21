#![cfg(feature = "pg-tests")]
mod common;
#[path = "fixtures/workstream_access.rs"]
mod fixture;
use awr_core::Id;
use awr_team_pg::PgError;
use fixture::*;

#[tokio::test]
async fn credentials_and_project_membership_do_not_grant_other_clients_scopes() {
    let (_guard, _, _, store) = setup().await;
    let a = store
        .query(TENANT, PROJECT, A, query("workstreams.list"))
        .await
        .unwrap();
    assert_eq!(a["total"], 1);
    assert!(!a.to_string().contains("private-beta"));
    let b = store
        .query(TENANT, PROJECT, B, query("work.list"))
        .await
        .unwrap();
    assert_eq!(b["data"]["total"], 1);
    assert_eq!(b["data"]["items"][0]["work_id"], "b-private");
    for token in [NONE, "garbage", &A.replace('a', "d")] {
        assert!(matches!(
            store
                .query(TENANT, PROJECT, token, query("capabilities"))
                .await,
            Err(PgError::Forbidden)
        ));
    }
    assert!(matches!(
        store
            .query("other-tenant", PROJECT, A, query("capabilities"))
            .await,
        Err(PgError::Forbidden)
    ));
}

#[tokio::test]
async fn hidden_and_missing_work_or_session_have_the_same_denial() {
    let (_guard, _, _, store) = setup().await;
    for work in ["b-private", "missing"] {
        let mut q = query("work.prepare");
        q.work_id = Some(work.into());
        assert!(matches!(
            store.query(TENANT, PROJECT, A, q).await,
            Err(PgError::Forbidden)
        ));
    }
    for session in ["session-b", "absent"] {
        let mut q = query("session.inspect");
        q.session_id = Some(session.into());
        assert!(matches!(
            store.query(TENANT, PROJECT, A, q).await,
            Err(PgError::Forbidden)
        ));
    }
    let mut mismatch = query("work.prepare");
    mismatch.work_id = Some("a".into());
    mismatch.workstream_id = Some(Id::from(2));
    assert!(store.query(TENANT, PROJECT, A, mismatch).await.is_err());
}

#[tokio::test]
async fn scoped_count_search_pagination_and_cursors_never_include_hidden_works() {
    let (_guard, admin, _, store) = setup().await;
    let mut q = query("work.list");
    q.limit = Some(1);
    let first = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_eq!(first["data"]["total"], 2);
    q.cursor = Some(first["data"]["next_cursor"].as_str().unwrap().into());
    let next = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_eq!(next["data"]["items"][0]["work_id"], "c");
    assert!(matches!(
        store.query(TENANT, PROJECT, B, q.clone()).await,
        Err(PgError::CursorExpired)
    ));
    admin.batch_execute("UPDATE awr_team.workstream_grants SET grant_version=grant_version+1 WHERE client_id='cli-a'").await.unwrap();
    assert!(matches!(
        store.query(TENANT, PROJECT, A, q).await,
        Err(PgError::CursorExpired)
    ));
    let mut q = query("work.search");
    q.search = Some("private".into());
    let search = store.query(TENANT, PROJECT, A, q).await.unwrap();
    assert_eq!(search["data"]["total"], 0);
}

#[tokio::test]
async fn dependency_exports_are_opaque_and_required_context_is_never_cut_to_budget() {
    let (_guard, _, _, store) = setup().await;
    let mut q = query("work.prepare");
    q.work_id = Some("c".into());
    let result = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_eq!(result["data"]["context_complete"], false);
    assert_eq!(result["data"]["dependency_export_unavailable"], true);
    assert!(!result.to_string().contains("b-private"));
    assert!(result.to_string().contains("preserve compatibility"));
    q.max_context_bytes = Some(1);
    assert!(matches!(
        store.query(TENANT, PROJECT, A, q).await,
        Err(PgError::ContextIncomplete)
    ));
}

#[tokio::test]
async fn events_and_recovery_respect_attribution_and_ownership_generation() {
    let (_guard, admin, _, store) = setup().await;
    let events = store
        .query(TENANT, PROJECT, A, query("events.list"))
        .await
        .unwrap();
    assert_eq!(events["data"]["items"].as_array().unwrap().len(), 1);
    assert!(!events.to_string().contains("session-b"));
    let mut q = query("work.recovery");
    q.work_id = Some("a".into());
    let recovery = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_eq!(
        recovery["data"]["items"][0]["next_action"],
        "continue alpha"
    );
    assert!(!recovery.to_string().contains("PRIVATE"));
    assert_eq!(
        recovery["data"]["items"][0]["contract_matches_current"],
        false
    );
    let current = recovery["data"]["current_contract_hash"].as_str().unwrap();
    admin
        .execute(
            "UPDATE awr_team.checkpoints SET contract_hash=$1 WHERE session_id='session-a'",
            &[&current],
        )
        .await
        .unwrap();
    let matched = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_eq!(
        matched["data"]["items"][0]["contract_matches_current"],
        true
    );
    // A matching contract is only one recovery fact, never permission to resume.
    assert_eq!(matched["data"]["automatic_resume"], false);
    admin
        .batch_execute("UPDATE awr_team.sessions SET ownership_version=2 WHERE id='session-a'")
        .await
        .unwrap();
    assert!(
        store.query(TENANT, PROJECT, A, q).await.unwrap()["data"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut q = query("session.inspect");
    q.session_id = Some("session-a".into());
    assert!(matches!(
        store.query(TENANT, PROJECT, A, q).await,
        Err(PgError::Forbidden)
    ));
}

#[tokio::test]
async fn semantic_context_tracks_selected_runtime_without_global_audit_churn() {
    let (_guard, admin, _, store) = setup().await;
    let mut q = query("work.prepare");
    q.work_id = Some("a".into());
    let before = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_eq!(before["data"]["context_complete"], true);
    assert_eq!(
        before["data"]["context_hash_protocol"],
        "awr-team-workstream-context-v1"
    );
    assert!(before["data"]["runtime"].is_null());
    admin.batch_execute("INSERT INTO awr_team.work_runtime(tenant_id,project_id,scope_id,work_id,state,work_version)
        VALUES('reader-tenant','reader-project','main','b-private','pending',1);
        UPDATE awr_team.projects SET project_revision=project_revision+1 WHERE tenant_id='reader-tenant';").await.unwrap();
    let unrelated = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_ne!(before["project_revision"], unrelated["project_revision"]);
    assert_eq!(before["data"], unrelated["data"]);
    admin.batch_execute("INSERT INTO awr_team.work_runtime(tenant_id,project_id,scope_id,work_id,state,work_version,recovery_blocked)
        VALUES('reader-tenant','reader-project','main','a','blocked',2,true)").await.unwrap();
    let changed = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_ne!(
        changed["data"]["context_hash"],
        before["data"]["context_hash"]
    );
    assert_eq!(changed["data"]["runtime"]["work_version"], "2");
    assert_eq!(changed["data"]["runtime"]["recovery_blocked"], true);
    assert_eq!(changed["data"]["execution_admission"], "not_evaluated");
    admin
        .batch_execute(
            "UPDATE awr_team.projects SET status='frozen' WHERE tenant_id='reader-tenant'",
        )
        .await
        .unwrap();
    let frozen = store.query(TENANT, PROJECT, A, q).await.unwrap();
    assert_eq!(frozen["project_status"], "frozen");
    assert_ne!(
        frozen["data"]["context_hash"],
        changed["data"]["context_hash"]
    );
}

#[tokio::test]
async fn mismatched_event_attribution_stale_grants_and_oversized_recovery_fail_closed() {
    let (_guard, admin, _, store) = setup().await;
    admin.execute("INSERT INTO awr_team.events(tenant_id,project_id,id,project_revision,event_index,event_type,actor_id,work_id,payload_json,workstream_id)
        VALUES($1,$2,'wrong-stream',4,0,'session.started','agent','b-private','{}',$3),
              ($1,$2,'unattributed',4,1,'session.started','agent','a','{}',NULL)",
        &[&TENANT,&PROJECT,&Id::from(1).to_string()]).await.unwrap();
    admin
        .batch_execute(
            "UPDATE awr_team.projects SET project_revision=4 WHERE tenant_id='reader-tenant'",
        )
        .await
        .unwrap();
    let events = store
        .query(TENANT, PROJECT, A, query("events.list"))
        .await
        .unwrap();
    assert_eq!(events["data"]["items"].as_array().unwrap().len(), 1);
    admin
        .batch_execute(
            "UPDATE awr_team.workstream_grants SET authority_version=2 WHERE client_id='cli-a'",
        )
        .await
        .unwrap();
    assert!(matches!(
        store.query(TENANT, PROJECT, A, query("work.list")).await,
        Err(PgError::Forbidden)
    ));
    admin.batch_execute("UPDATE awr_team.workstream_grants SET authority_version=1 WHERE client_id='cli-a';
        UPDATE awr_team.checkpoints SET next_action=repeat('x',1048576) WHERE session_id='session-a'").await.unwrap();
    let mut q = query("work.recovery");
    q.work_id = Some("a".into());
    assert!(matches!(
        store.query(TENANT, PROJECT, A, q).await,
        Err(PgError::ResponseTooLarge)
    ));
}

async fn wait_for_lock(admin: &tokio_postgres::Client, query_fragment: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = admin
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity
                WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE $1)",
                    &[&format!("%{query_fragment}%")],
                )
                .await
                .unwrap()
                .get(0);
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("expected real PostgreSQL row-lock wait");
}

#[tokio::test]
async fn revocation_and_reads_serialize_in_both_orders_without_stale_authority() {
    let (_guard, mut admin, db, store) = setup().await;
    let store = std::sync::Arc::new(store);
    let observer = common::connect_config(&common::with_db(&common::test_config(), &db)).await;
    // Revoke first. The reader began with an older repeatable-read snapshot
    // but must not return it after waiting on the credential row.
    let revoke = admin.transaction().await.unwrap();
    revoke
        .batch_execute(
            "UPDATE awr_team.credentials SET revoked_at=clock_timestamp() WHERE id='reader-a'",
        )
        .await
        .unwrap();
    let reader = store.clone();
    let read =
        tokio::spawn(async move { reader.query(TENANT, PROJECT, A, query("work.list")).await });
    wait_for_lock(&observer, "FOR SHARE OF t,a,c,m").await;
    revoke.commit().await.unwrap();
    match read.await.unwrap() {
        Err(PgError::Forbidden) => {}
        Err(PgError::Db(e)) => assert_eq!(
            e.code(),
            Some(&tokio_postgres::error::SqlState::T_R_SERIALIZATION_FAILURE)
        ),
        other => panic!("revocation must prevent stale data: {other:?}"),
    }
    assert!(matches!(
        store.query(TENANT, PROJECT, A, query("work.list")).await,
        Err(PgError::Forbidden)
    ));
    admin
        .batch_execute("UPDATE awr_team.credentials SET revoked_at=NULL WHERE id='reader-a'")
        .await
        .unwrap();
    // Read first. Hold a later grant row to observe the authenticated reader
    // retaining its credential lock until the whole query finishes.
    let hold = admin.transaction().await.unwrap();
    hold.query(
        "SELECT workstream_id FROM awr_team.workstream_grants WHERE client_id='cli-a' FOR UPDATE",
        &[],
    )
    .await
    .unwrap();
    let reader = store.clone();
    let read =
        tokio::spawn(async move { reader.query(TENANT, PROJECT, A, query("work.list")).await });
    wait_for_lock(&observer, "ORDER BY workstream_id FOR SHARE").await;
    let revoker = common::connect_config(&common::with_db(&common::test_config(), &db)).await;
    let revoke = tokio::spawn(async move {
        revoker
            .batch_execute(
                "UPDATE awr_team.credentials SET revoked_at=clock_timestamp() WHERE id='reader-a'",
            )
            .await
    });
    wait_for_lock(&observer, "UPDATE awr_team.credentials SET revoked_at").await;
    hold.rollback().await.unwrap();
    assert_eq!(read.await.unwrap().unwrap()["data"]["total"], 2);
    revoke.await.unwrap().unwrap();
    assert!(matches!(
        store.query(TENANT, PROJECT, A, query("work.list")).await,
        Err(PgError::Forbidden)
    ));
}

#[tokio::test]
async fn dynamic_revocation_expiry_actor_and_membership_changes_take_effect_on_next_read() {
    let (_guard, admin, _, store) = setup().await;
    assert!(
        store
            .query(TENANT, PROJECT, A, query("capabilities"))
            .await
            .is_ok()
    );
    for (disable, restore) in [
        (
            "UPDATE awr_team.credentials SET revoked_at=clock_timestamp() WHERE id='reader-a'",
            "UPDATE awr_team.credentials SET revoked_at=NULL WHERE id='reader-a'",
        ),
        (
            "UPDATE awr_team.credentials SET expires_at=clock_timestamp()-interval '1 second' WHERE id='reader-a'",
            "UPDATE awr_team.credentials SET expires_at=NULL WHERE id='reader-a'",
        ),
        (
            "UPDATE awr_team.actors SET status='disabled' WHERE id='agent'",
            "UPDATE awr_team.actors SET status='active' WHERE id='agent'",
        ),
        (
            "UPDATE awr_team.tenants SET status='disabled' WHERE id='reader-tenant'",
            "UPDATE awr_team.tenants SET status='active' WHERE id='reader-tenant'",
        ),
        (
            "UPDATE awr_team.workstream_grants SET active=false WHERE client_id='cli-a'",
            "UPDATE awr_team.workstream_grants SET active=true WHERE client_id='cli-a'",
        ),
    ] {
        admin.batch_execute(disable).await.unwrap();
        assert!(matches!(
            store.query(TENANT, PROJECT, A, query("capabilities")).await,
            Err(PgError::Forbidden)
        ));
        admin.batch_execute(restore).await.unwrap();
        assert!(
            store
                .query(TENANT, PROJECT, A, query("capabilities"))
                .await
                .is_ok()
        );
    }
    admin.batch_execute("DELETE FROM awr_team.workstream_grants WHERE actor_id='agent'; DELETE FROM awr_team.project_memberships WHERE actor_id='agent'").await.unwrap();
    assert!(matches!(
        store.query(TENANT, PROJECT, A, query("capabilities")).await,
        Err(PgError::Forbidden)
    ));
}

#[tokio::test]
async fn multi_scope_choice_is_explicit_and_protocol_rejects_forged_authority_fields() {
    let (_guard, admin, _, store) = setup().await;
    admin.execute("INSERT INTO awr_team.workstream_grants(tenant_id,project_id,actor_id,client_id,workstream_id,authority_version,can_read)
        VALUES($1,$2,'agent','cli-a',$3,1,true)",&[&TENANT,&PROJECT,&Id::from(2).to_string()]).await.unwrap();
    assert!(matches!(
        store.query(TENANT, PROJECT, A, query("work.list")).await,
        Err(PgError::Workstream(
            awr_core::WorkstreamError::ScopeRequired
        ))
    ));
    for field in ["actor_id", "tenant_id", "project_id", "client_id", "grants"] {
        let mut value = serde_json::json!({"protocol_version":1,"op":"work.list"});
        value[field] = serde_json::json!("forged");
        assert!(serde_json::from_value::<awr_team_pg::WorkstreamQuery>(value).is_err());
    }
    let mut unsupported = query("claim.acquire");
    unsupported.protocol_version = 1;
    assert!(matches!(
        store.query(TENANT, PROJECT, A, unsupported).await,
        Err(PgError::Unsupported(_))
    ));
}
