#![cfg(feature = "pg-tests")]
mod common;
#[path = "fixtures/workstream_access.rs"]
mod fixture;
use awr_team_pg::{PgError, WorkstreamCommand, WorkstreamReadStore};
use fixture::*;
use serde_json::{Value, json};
use tokio_postgres::Client;

async fn claim(store: &WorkstreamReadStore) -> Value {
    store.commands().execute(TENANT,PROJECT,A,command(&prepare(store,A,"a").await,"take","claim.acquire",
        json!({"session_id":"session-a","expected_session_version":"1","expected_work_version":"0","ttl_seconds":60})))
        .await.unwrap()["receipt"]["data"].clone()
}
async fn intent(store: &WorkstreamReadStore, c: &Value, id: &str) -> WorkstreamCommand {
    let p = prepare(store, A, "a").await;
    command(
        &p,
        id,
        "execution.prepare",
        json!({"session_id":"session-a","expected_session_version":"1",
        "claim_id":c["claim_id"],"expected_fence":c["fence"],"expected_lease_version":c["lease_version"],
        "expected_work_version":p["data"]["runtime"]["work_version"],"input_digest":"a".repeat(64),"declared_scope":["src/api"]}),
    )
}
async fn cancel(store: &WorkstreamReadStore, e: &Value, id: &str) -> WorkstreamCommand {
    command(
        &prepare(store, A, "a").await,
        id,
        "execution.cancel",
        json!({"session_id":"session-a",
        "expected_session_version":"1","execution_id":e["execution_id"],"expected_execution_version":e["execution_version"]}),
    )
}
async fn inspect(store: &WorkstreamReadStore, e: &Value) -> Value {
    let mut q = query("execution.inspect");
    q.work_id = Some("a".into());
    q.execution_id = Some(e["execution_id"].as_str().unwrap().into());
    store.query(TENANT, PROJECT, A, q).await.unwrap()["data"].clone()
}
async fn snapshot(admin: &Client) -> Value {
    admin.query_one("SELECT jsonb_build_object(
        'execution',(SELECT jsonb_agg(to_jsonb(e) ORDER BY id) FROM awr_team.executions e),
        'work',(SELECT jsonb_agg(to_jsonb(w) ORDER BY work_id) FROM awr_team.work_runtime w),
        'outbox',(SELECT jsonb_agg(to_jsonb(o) ORDER BY id) FROM awr_team.outbox o),
        'receipts',(SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM awr_team.execution_receipts r),
        'resources',(SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM awr_team.resource_reservations r),
        'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY id) FROM awr_team.events e),
        'operations',(SELECT jsonb_agg(to_jsonb(o) ORDER BY id) FROM awr_team.operations o),
        'revision',(SELECT project_revision FROM awr_team.projects WHERE tenant_id=$1 AND id=$2))",
        &[&TENANT,&PROJECT]).await.unwrap().get(0)
}

#[tokio::test]
async fn intents_cancel_without_dispatch_and_replay_never_resurrects_them() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    let cmd = intent(&store, &c, "prepare").await;
    let one = store
        .commands()
        .execute(TENANT, PROJECT, A, cmd.clone())
        .await
        .unwrap();
    let e = &one["receipt"]["data"];
    assert_eq!(one["receipt"]["execution_authorized"], false);
    assert_eq!(e["dispatched"], false);
    assert_eq!(e["admission"], "not_evaluated");
    assert_eq!(e["execution_state_basis"], "at_commit");
    assert_eq!(e["work_version"], "2");
    assert!(snapshot(&admin).await["outbox"].is_null());
    assert_eq!(inspect(&store, e).await["owned_by_client"], true);
    assert_eq!(inspect(&store, e).await["execution_authorized"], false);
    let mut start = cmd.clone();
    start.op = "execution.start".into();
    assert!(matches!(
        store.commands().execute(TENANT, PROJECT, A, start).await,
        Err(PgError::Unsupported(_))
    ));
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, intent(&store, &c, "second").await)
            .await,
        Err(PgError::RecoveryBlocked)
    ));
    admin
        .batch_execute(
            "UPDATE awr_team.claims SET expires_at=clock_timestamp()-interval '1 second'",
        )
        .await
        .unwrap();
    // Cancellation remains possible after the planning lease expires.
    let stopped = store
        .commands()
        .execute(TENANT, PROJECT, A, cancel(&store, e, "cancel").await)
        .await
        .unwrap();
    assert_eq!(stopped["receipt"]["data"]["stop_confirmed"], true);
    assert_eq!(stopped["receipt"]["data"]["state"], "cancelled");
    let before = snapshot(&admin).await;
    let replay = store
        .commands()
        .execute(TENANT, PROJECT, A, cmd)
        .await
        .unwrap();
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["receipt"], one["receipt"]);
    assert_eq!(inspect(&store, e).await["state"], "cancelled");
    assert_eq!(snapshot(&admin).await, before);
    store.commands().execute(TENANT,PROJECT,A,command(&prepare(&store,A,"a").await,"release","claim.release",
        json!({"session_id":"session-a","expected_session_version":"1","claim_id":c["claim_id"],
            "expected_fence":c["fence"],"expected_lease_version":c["lease_version"]}))).await.unwrap();
}

#[tokio::test]
async fn concurrent_intents_and_exact_retries_create_one_execution() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    let cmd = intent(&store, &c, "same").await;
    let s = store.commands();
    let (a, b) = tokio::join!(
        s.execute(TENANT, PROJECT, A, cmd.clone()),
        s.execute(TENANT, PROJECT, A, cmd.clone())
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a["receipt"], b["receipt"]);
    assert_ne!(a["replayed"], b["replayed"]);
    let mut changed = cmd;
    changed.args["input_digest"] = json!("b".repeat(64));
    assert!(matches!(
        s.execute(TENANT, PROJECT, A, changed).await,
        Err(PgError::IdempotencyConflict)
    ));
    let e = &a["receipt"]["data"];
    s.execute(TENANT, PROJECT, A, cancel(&store, e, "cancel").await)
        .await
        .unwrap();
    let first = intent(&store, &c, "next").await;
    let mut second = first.clone();
    second.request_id = "other".into();
    let (a, b) = tokio::join!(
        s.execute(TENANT, PROJECT, A, first),
        s.execute(TENANT, PROJECT, A, second)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert!(matches!(
        a.err().or(b.err()),
        Some(PgError::PreconditionsChanged)
    ));
    assert_eq!(
        snapshot(&admin).await["execution"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn scope_and_version_guards_reject_forged_identity_without_writes() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    let cmd = intent(&store, &c, "intent").await;
    let before = snapshot(&admin).await;
    for p in [
        "/src",
        "src/../outside",
        "src//file",
        "src/./file",
        "src\\file",
        "C:/src",
        "src\nfile",
    ] {
        let mut r = cmd.clone();
        r.args["declared_scope"] = json!([p]);
        assert!(matches!(
            store.commands().execute(TENANT, PROJECT, A, r).await,
            Err(PgError::Protocol(_))
        ));
    }
    for (field, v) in [
        ("actor_id", json!("reviewer")),
        ("fencing_class", json!("hard_fence")),
        ("receipt_kind", json!("trusted_executor")),
        ("input_digest", json!("invented")),
        ("expected_work_version", json!("01")),
        ("declared_scope", json!(vec!["src"; 129])),
    ] {
        let mut r = cmd.clone();
        r.args[field] = v;
        assert!(matches!(
            store.commands().execute(TENANT, PROJECT, A, r).await,
            Err(PgError::Protocol(_))
        ));
    }
    let mut outside = cmd.clone();
    outside.args["declared_scope"] = json!(["outside"]);
    assert!(matches!(
        store.commands().execute(TENANT, PROJECT, A, outside).await,
        Err(PgError::ScopeExceeded)
    ));
    for field in [
        "expected_work_version",
        "expected_session_version",
        "expected_lease_version",
    ] {
        let mut r = cmd.clone();
        r.args[field] = json!("9");
        assert!(matches!(
            store.commands().execute(TENANT, PROJECT, A, r).await,
            Err(PgError::PreconditionsChanged)
        ));
    }
    let mut r = cmd.clone();
    r.args["expected_fence"] = json!("9");
    assert!(matches!(
        store.commands().execute(TENANT, PROJECT, A, r).await,
        Err(PgError::StaleFence)
    ));
    assert_eq!(snapshot(&admin).await, before);
    admin
        .batch_execute(
            "UPDATE awr_team.claims SET expires_at=clock_timestamp()-interval '1 second'",
        )
        .await
        .unwrap();
    assert!(matches!(
        store.commands().execute(TENANT, PROJECT, A, cmd).await,
        Err(PgError::LeaseExpired)
    ));
}

#[tokio::test]
async fn same_actor_other_client_may_read_granted_metadata_but_cannot_cancel() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    let cmd = intent(&store, &c, "intent").await;
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, B, cmd.clone())
            .await,
        Err(PgError::Forbidden)
    ));
    let e = store
        .commands()
        .execute(TENANT, PROJECT, A, cmd)
        .await
        .unwrap()["receipt"]["data"]
        .clone();
    let mut q = query("execution.inspect");
    q.work_id = Some("a".into());
    q.execution_id = Some(e["execution_id"].as_str().unwrap().into());
    assert!(matches!(
        store.query(TENANT, PROJECT, B, q.clone()).await,
        Err(PgError::Forbidden)
    ));
    admin.execute("INSERT INTO awr_team.workstream_grants(tenant_id,project_id,actor_id,client_id,workstream_id,authority_version,can_read,can_write)
        VALUES($1,$2,'agent','cli-b',$3,1,true,true)",&[&TENANT,&PROJECT,&awr_core::Id::from(1).to_string()]).await.unwrap();
    assert_eq!(
        store.query(TENANT, PROJECT, B, q.clone()).await.unwrap()["data"]["owned_by_client"],
        false
    );
    let before = snapshot(&admin).await;
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, B, cancel(&store, &e, "forged").await)
            .await,
        Err(PgError::Forbidden)
    ));
    for id in ["hidden", e["execution_id"].as_str().unwrap()] {
        q.work_id = Some("b-private".into());
        q.execution_id = Some(id.into());
        assert!(matches!(
            store.query(TENANT, PROJECT, B, q.clone()).await,
            Err(PgError::Forbidden)
        ));
    }
    assert_eq!(snapshot(&admin).await, before);
}

#[tokio::test]
async fn cancel_of_exposed_or_unknown_execution_is_only_a_request_and_keeps_resources() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    let mut e = store
        .commands()
        .execute(TENANT, PROJECT, A, intent(&store, &c, "prepare").await)
        .await
        .unwrap()["receipt"]["data"]
        .clone();
    admin.execute("INSERT INTO awr_team.outbox(tenant_id,project_id,id,state,payload_json,action_kind,aggregate_id)
        VALUES($1,$2,'delivery','sending','{}','execution.dispatch',$3)",&[&TENANT,&PROJECT,&e["execution_id"].as_str().unwrap()]).await.unwrap();
    admin.batch_execute("UPDATE awr_team.work_runtime SET recovery_blocked=true;
        INSERT INTO awr_team.resource_reservations(tenant_id,project_id,id,work_id,resource_kind,canonical_key,state)
        VALUES('reader-tenant','reader-project','resource','a','named','shared','unknown')").await.unwrap();
    for state in ["prepared", "queued", "accepted", "running", "unknown"] {
        admin
            .execute("UPDATE awr_team.executions SET state=$1", &[&state])
            .await
            .unwrap();
        let result = store
            .commands()
            .execute(TENANT, PROJECT, A, cancel(&store, &e, state).await)
            .await
            .unwrap();
        let data = &result["receipt"]["data"];
        assert_eq!(data["state"], state);
        assert_eq!(data["stop_confirmed"], false);
        e["execution_version"] = data["execution_version"].clone();
        let snap = snapshot(&admin).await;
        assert_eq!(snap["outbox"][0]["state"], "sending");
        assert_eq!(snap["resources"][0]["state"], "unknown");
        assert_eq!(snap["work"][0]["recovery_blocked"], true);
        assert!(matches!(
            store
                .commands()
                .execute(TENANT, PROJECT, A, intent(&store, &c, "retry").await)
                .await,
            Err(PgError::RecoveryBlocked)
        ));
    }
}

#[tokio::test]
async fn epoch_and_ownership_changes_do_not_adopt_old_execution_records() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    let e = store
        .commands()
        .execute(TENANT, PROJECT, A, intent(&store, &c, "prepare").await)
        .await
        .unwrap()["receipt"]["data"]
        .clone();
    admin
        .batch_execute(
            "UPDATE awr_team.projects SET coordinator_epoch='new' WHERE tenant_id='reader-tenant'",
        )
        .await
        .unwrap();
    assert_eq!(inspect(&store, &e).await["epoch_matches_current"], false);
    assert_eq!(inspect(&store, &e).await["lease_live"], false);
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, cancel(&store, &e, "new-epoch").await)
            .await,
        Err(PgError::EpochChanged)
    ));
    admin.batch_execute("UPDATE awr_team.projects SET coordinator_epoch='epoch-a' WHERE tenant_id='reader-tenant';
        UPDATE awr_team.executions SET ownership_version=2").await.unwrap();
    let mut q = query("execution.inspect");
    q.work_id = Some("a".into());
    q.execution_id = Some(e["execution_id"].as_str().unwrap().into());
    assert!(matches!(
        store.query(TENANT, PROJECT, A, q).await,
        Err(PgError::Forbidden)
    ));
    assert!(matches!(
        store
            .commands()
            .execute(
                TENANT,
                PROJECT,
                A,
                cancel(&store, &e, "other-generation").await
            )
            .await,
        Err(PgError::Forbidden)
    ));
}

#[tokio::test]
async fn failed_events_roll_back_intent_and_cancel_with_revision_and_receipt() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    let cmd = intent(&store, &c, "prepare").await;
    admin.batch_execute("CREATE FUNCTION awr_team.reject_execution_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
        IF NEW.event_type LIKE 'execution.%' THEN RAISE EXCEPTION 'synthetic failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER reject_execution_event BEFORE INSERT ON awr_team.events FOR EACH ROW EXECUTE FUNCTION awr_team.reject_execution_event()").await.unwrap();
    let before = snapshot(&admin).await;
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, cmd.clone())
            .await,
        Err(PgError::Db(_))
    ));
    assert_eq!(snapshot(&admin).await, before);
    admin
        .batch_execute("ALTER TABLE awr_team.events DISABLE TRIGGER reject_execution_event")
        .await
        .unwrap();
    let e = store
        .commands()
        .execute(TENANT, PROJECT, A, cmd)
        .await
        .unwrap()["receipt"]["data"]
        .clone();
    let cmd = cancel(&store, &e, "cancel").await;
    admin
        .batch_execute("ALTER TABLE awr_team.events ENABLE TRIGGER reject_execution_event")
        .await
        .unwrap();
    let before = snapshot(&admin).await;
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, cmd.clone())
            .await,
        Err(PgError::Db(_))
    ));
    assert_eq!(snapshot(&admin).await, before);
    admin
        .batch_execute("DROP TRIGGER reject_execution_event ON awr_team.events")
        .await
        .unwrap();
    store
        .commands()
        .execute(TENANT, PROJECT, A, cmd)
        .await
        .unwrap();
}

#[tokio::test]
async fn revoked_grant_blocks_waiting_preparation_before_any_intent_is_written() {
    let (_g, mut admin, db, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    let cmd = intent(&store, &c, "prepare").await;
    let before = snapshot(&admin).await;
    let observer = common::connect_config(&common::with_db(&common::test_config(), &db)).await;
    let revoke = admin.transaction().await.unwrap();
    revoke.batch_execute("UPDATE awr_team.workstream_grants SET can_write=false,grant_version=grant_version+1 WHERE client_id='cli-a'").await.unwrap();
    let commands = store.commands();
    let write = tokio::spawn(async move { commands.execute(TENANT, PROJECT, A, cmd).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = observer
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()
                AND wait_event_type='Lock' AND query LIKE '%ORDER BY workstream_id FOR SHARE%')",
                    &[],
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
    .expect("writer must wait on the live grant row");
    revoke.commit().await.unwrap();
    assert!(matches!(write.await.unwrap(), Err(PgError::Forbidden)));
    assert_eq!(snapshot(&admin).await, before);
}

#[tokio::test]
async fn barriers_and_active_contract_checks_preserve_recovery_only_cancellation() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = claim(&store).await;
    admin
        .batch_execute(
            "UPDATE awr_team.work_contracts SET definition_state='archived' WHERE work_id='a'",
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, intent(&store, &c, "archived").await)
            .await,
        Err(PgError::PreconditionsChanged)
    ));
    admin
        .batch_execute(
            "UPDATE awr_team.work_contracts SET definition_state='enabled' WHERE work_id='a';
        INSERT INTO awr_team.wait_items(tenant_id,project_id,id,session_id,work_id,question,state)
        VALUES('reader-tenant','reader-project','wait','session-a','a','Confirm the plan?','open')",
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, intent(&store, &c, "waiting").await)
            .await,
        Err(PgError::WaitOpen)
    ));
    admin
        .batch_execute("UPDATE awr_team.wait_items SET state='replied'")
        .await
        .unwrap();
    let e = store
        .commands()
        .execute(TENANT, PROJECT, A, intent(&store, &c, "prepared").await)
        .await
        .unwrap()["receipt"]["data"]
        .clone();
    admin
        .batch_execute(
            "UPDATE awr_team.projects SET status='frozen' WHERE tenant_id='reader-tenant'",
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, cancel(&store, &e, "frozen").await)
            .await,
        Err(PgError::ProjectNotAvailable)
    ));
    assert_eq!(inspect(&store, &e).await["state"], "prepared");
    admin
        .batch_execute(
            "UPDATE awr_team.projects SET status='active' WHERE tenant_id='reader-tenant'",
        )
        .await
        .unwrap();
    let mut catalog: awr_core::WorkstreamCatalog = serde_json::from_value(
        admin
            .query_one(
                "SELECT catalog_json FROM awr_team.workstream_catalogs WHERE tenant_id=$1",
                &[&TENANT],
            )
            .await
            .unwrap()
            .get(0),
    )
    .unwrap();
    catalog
        .workstreams
        .iter_mut()
        .find(|s| s.id == awr_core::Id::from(1))
        .unwrap()
        .state = awr_core::WorkstreamState::Paused;
    admin
        .execute(
            "UPDATE awr_team.workstream_catalogs SET catalog_json=$1 WHERE tenant_id=$2",
            &[&json!(catalog), &TENANT],
        )
        .await
        .unwrap();
    store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            A,
            cancel(&store, &e, "paused-cancel").await,
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .commands()
            .execute(
                TENANT,
                PROJECT,
                A,
                intent(&store, &c, "paused-prepare").await
            )
            .await,
        Err(PgError::Workstream(awr_core::WorkstreamError::Inactive))
    ));
}

#[tokio::test]
async fn schema_twelve_preserves_legacy_executions_and_failed_migration_is_atomic() {
    let (_g, admin, _, _store) = setup().await;
    admin.batch_execute("ALTER TABLE awr_team.executions DROP CONSTRAINT executions_workstream_binding;
        ALTER TABLE awr_team.executions DROP COLUMN workstream_id,DROP COLUMN ownership_version,DROP COLUMN executor_client_id,DROP COLUMN execution_version;
        UPDATE awr_team.schema_state SET version=11;
        INSERT INTO awr_team.executions(tenant_id,project_id,id,work_id,fence,contract_hash,executor_actor_id,state)
        VALUES('reader-tenant','reader-project','legacy','a',4,'old-contract','old-runner','unknown')").await.unwrap();
    let ddl = include_str!("../migrations/20260921000012_workstream_executions.sql");
    assert!(
        admin
            .batch_execute(&ddl.replace(
                "UPDATE awr_team.schema_state",
                "SELECT 1/0; UPDATE awr_team.schema_state"
            ))
            .await
            .is_err()
    );
    admin.batch_execute("ROLLBACK").await.unwrap();
    let row=admin.query_one("SELECT (SELECT version FROM awr_team.schema_state),(SELECT count(*) FROM information_schema.columns
        WHERE table_schema='awr_team' AND table_name='executions' AND column_name='workstream_id')",&[]).await.unwrap();
    assert_eq!(row.get::<_, i32>(0), 11);
    assert_eq!(row.get::<_, i64>(1), 0);
    awr_team_pg::migrate(&admin).await.unwrap();
    awr_team_pg::migrate(&admin).await.unwrap();
    let r=admin.query_one("SELECT state,fence,scope_id,workstream_id,ownership_version,executor_client_id,execution_version FROM awr_team.executions",&[]).await.unwrap();
    assert_eq!(r.get::<_, String>(0), "unknown");
    assert_eq!(r.get::<_, i64>(1), 4);
    assert_eq!(r.get::<_, String>(2), "main");
    assert_eq!(r.get::<_, Option<String>>(3), None);
    assert_eq!(r.get::<_, Option<i64>>(4), None);
    assert_eq!(r.get::<_, Option<String>>(5), None);
    assert_eq!(r.get::<_, i64>(6), 1);
    assert!(
        admin
            .batch_execute("UPDATE awr_team.executions SET ownership_version=1")
            .await
            .is_err()
    );
}
