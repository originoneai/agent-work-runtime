#![cfg(feature = "pg-tests")]
mod common;
#[path = "fixtures/workstream_access.rs"]
mod fixture;
use awr_team_pg::{PgError, WorkstreamCommand, WorkstreamReadStore, workstream_credential_hash};
use fixture::*;
use serde_json::{Value, json};
use tokio_postgres::Client;

const OP: &str = "awr1.operator.dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
async fn operator(admin: &Client, store: &WorkstreamReadStore) -> String {
    admin
        .batch_execute(
            "INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status)
        VALUES('reader-tenant','operator','human','Recovery operator','active');
        INSERT INTO awr_team.project_memberships(tenant_id,project_id,actor_id,role)
        VALUES('reader-tenant','reader-project','operator','admin')",
        )
        .await
        .unwrap();
    admin
        .execute(
            "INSERT INTO awr_team.credentials(tenant_id,id,actor_id,client_id,secret_hash)
        VALUES($1,'operator','operator','operator-cli',$2)",
            &[&TENANT, &workstream_credential_hash(OP).unwrap()],
        )
        .await
        .unwrap();
    admin.execute("INSERT INTO awr_team.workstream_grants(tenant_id,project_id,actor_id,client_id,workstream_id,authority_version,
        can_read,can_write,can_manage,can_reconcile_execution) VALUES($1,$2,'operator','operator-cli',$3,1,true,true,true,true)",
        &[&TENANT,&PROJECT,&awr_core::Id::from(1).to_string()]).await.unwrap();
    let r = store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            OP,
            command(
                &prepare(store, OP, "a").await,
                "operator-session",
                "session.start",
                json!({"conversation_id":"recovery"}),
            ),
        )
        .await
        .unwrap();
    r["receipt"]["data"]["session_id"].as_str().unwrap().into()
}
async fn take(store: &WorkstreamReadStore) -> Value {
    store.commands().execute(TENANT,PROJECT,A,command(&prepare(store,A,"a").await,"take","claim.acquire",
        json!({"session_id":"session-a","expected_session_version":"1","expected_work_version":"0","ttl_seconds":600})))
        .await.unwrap()["receipt"]["data"].clone()
}
async fn start(store: &WorkstreamReadStore, claim: &Value, name: &str) -> Value {
    let p = prepare(store, A, "a").await;
    let r=store.commands().execute(TENANT,PROJECT,A,command(&p,&format!("prepare-{name}"),"execution.prepare",
        json!({"session_id":"session-a","expected_session_version":"1","claim_id":claim["claim_id"],
        "expected_fence":claim["fence"],"expected_lease_version":claim["lease_version"],
        "expected_work_version":p["data"]["runtime"]["work_version"],"input_digest":"a".repeat(64),"declared_scope":["src/api"]})))
        .await.unwrap();
    let p = prepare(store, A, "a").await;
    store.commands().execute(TENANT,PROJECT,A,command(&p,&format!("start-{name}"),"execution.start",
        json!({"session_id":"session-a","expected_session_version":"1","execution_id":r["receipt"]["data"]["execution_id"],
        "expected_execution_version":"1","claim_id":claim["claim_id"],"expected_fence":claim["fence"],
        "expected_lease_version":claim["lease_version"],"expected_work_version":p["data"]["runtime"]["work_version"],"execution_mode":"caller_managed"})))
        .await.unwrap()["receipt"]["data"].clone()
}
fn facts(outcome: &str) -> Value {
    json!({"outcome":outcome,"input_digest":"a".repeat(64),"output_digest":"b".repeat(64),
        "environment_digest":"c".repeat(64),"observed_paths":["src/api/result.json"],"note":"Verified the actual process result and effects."})
}
async fn inspect(store: &WorkstreamReadStore, token: &str, e: &Value) -> Value {
    let mut q = query("execution.inspect");
    q.work_id = Some("a".into());
    q.execution_id = Some(e["execution_id"].as_str().unwrap().into());
    store.query(TENANT, PROJECT, token, q).await.unwrap()["data"].clone()
}
async fn report(store: &WorkstreamReadStore, e: &Value, name: &str) -> Value {
    store.commands().execute(TENANT,PROJECT,A,command(&prepare(store,A,"a").await,name,"execution.report",
        json!({"session_id":"session-a","expected_session_version":"1","execution_id":e["execution_id"],
        "expected_execution_version":e["execution_version"],"outcome":"succeeded","output_digest":"b".repeat(64),
        "observed_paths":["src/api/result.json"],"note":"Caller observation, awaiting verification."})))
        .await.unwrap()["receipt"]["data"].clone()
}
async fn attest(
    store: &WorkstreamReadStore,
    e: &Value,
    name: &str,
    outcome: &str,
) -> WorkstreamCommand {
    command(
        &prepare(store, A, "a").await,
        name,
        "execution.attest",
        json!({"session_id":"session-a",
        "expected_session_version":"1","execution_id":e["execution_id"],"expected_execution_version":e["execution_version"],"facts":facts(outcome)}),
    )
}
async fn reconcile(
    store: &WorkstreamReadStore,
    session: &str,
    e: &Value,
    name: &str,
    outcome: &str,
) -> WorkstreamCommand {
    let now = inspect(store, OP, e).await;
    let p = prepare(store, OP, "a").await;
    command(
        &p,
        name,
        "execution.reconcile",
        json!({"session_id":session,"expected_session_version":"1",
        "execution_id":e["execution_id"],"expected_execution_version":now["execution_version"],
        "expected_work_version":p["data"]["runtime"]["work_version"],
        "reviewed_receipt_id":now["latest_receipt"]["receipt_id"],"clear_recovery_block":true,"facts":facts(outcome)}),
    )
}
async fn snapshot(admin: &Client) -> Value {
    admin.query_one("SELECT jsonb_build_object(
        'executions',(SELECT jsonb_agg(to_jsonb(e) ORDER BY id) FROM awr_team.executions e),
        'resources',(SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM awr_team.resource_reservations r),
        'receipts',(SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM awr_team.execution_receipts r),
        'runtime',(SELECT jsonb_agg(to_jsonb(w) ORDER BY work_id) FROM awr_team.work_runtime w),
        'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY id) FROM awr_team.events e),
        'operations',(SELECT jsonb_agg(to_jsonb(o) ORDER BY id) FROM awr_team.operations o),
        'revision',(SELECT project_revision FROM awr_team.projects WHERE tenant_id=$1 AND id=$2))",
        &[&TENANT,&PROJECT]).await.unwrap().get(0)
}
async fn trusted_runner(admin: &Client) {
    admin.batch_execute("UPDATE awr_team.actors SET kind='system' WHERE id='agent';
        UPDATE awr_team.workstream_grants SET can_attest_execution=true,grant_version=grant_version+1 WHERE client_id='cli-a'").await.unwrap();
}

#[tokio::test]
async fn trusted_executor_requires_both_operator_grant_and_exact_system_client() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    trusted_runner(&admin).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    assert_eq!(e["result_authority"], "trusted_executor");
    let cmd = attest(&store, &e, "attest", "succeeded").await;
    let before = snapshot(&admin).await;
    admin
        .batch_execute(
            "UPDATE awr_team.actors SET kind='agent' WHERE id='agent';
        UPDATE awr_team.workstream_grants SET can_attest_execution=false WHERE client_id='cli-a'",
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, cmd.clone())
            .await,
        Err(PgError::Forbidden)
    ));
    admin.batch_execute("UPDATE awr_team.workstream_grants SET can_attest_execution=true WHERE client_id='cli-a'").await.unwrap();
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, cmd.clone())
            .await,
        Err(PgError::Forbidden)
    ));
    admin
        .batch_execute(
            "UPDATE awr_team.actors SET kind='system' WHERE id='agent';
        UPDATE awr_team.workstream_grants SET can_attest_execution=false WHERE client_id='cli-a'",
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, A, cmd.clone())
            .await,
        Err(PgError::Forbidden)
    ));
    trusted_runner(&admin).await;
    admin.execute("INSERT INTO awr_team.workstream_grants(tenant_id,project_id,actor_id,client_id,workstream_id,authority_version,can_read,can_write,can_attest_execution)
        VALUES($1,$2,'agent','cli-b',$3,1,true,true,true)",&[&TENANT,&PROJECT,&awr_core::Id::from(1).to_string()]).await.unwrap();
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, B, cmd.clone())
            .await,
        Err(PgError::Forbidden)
    ));
    let mut forged = cmd.clone();
    forged.args["receipt_kind"] = json!("trusted_executor");
    assert!(matches!(
        store.commands().execute(TENANT, PROJECT, A, forged).await,
        Err(PgError::Protocol(_))
    ));
    assert_eq!(snapshot(&admin).await, before);
    let result = store
        .commands()
        .execute(TENANT, PROJECT, A, cmd)
        .await
        .unwrap();
    assert_eq!(
        result["receipt"]["data"]["receipt_kind"],
        "trusted_executor"
    );
    assert_eq!(result["receipt"]["data"]["state"], "succeeded");
}

#[tokio::test]
async fn later_authority_cannot_upgrade_an_ordinary_admission_to_trusted_execution() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "ordinary").await;
    assert_eq!(e["result_authority"], "caller_asserted");
    trusted_runner(&admin).await;
    assert_eq!(inspect(&store, A, &e).await["attestation_authority"], false);
    let before = snapshot(&admin).await;
    assert!(matches!(
        store
            .commands()
            .execute(
                TENANT,
                PROJECT,
                A,
                attest(&store, &e, "retroactive", "succeeded").await
            )
            .await,
        Err(PgError::Forbidden)
    ));
    assert_eq!(snapshot(&admin).await, before);
    let r = store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            OP,
            reconcile(&store, &os, &e, "review", "succeeded").await,
        )
        .await
        .unwrap();
    assert_eq!(r["receipt"]["data"]["receipt_kind"], "reconcile");
    assert_eq!(r["receipt"]["data"]["recovery_blocked"], false);
    assert!(inspect(&store, OP, &e).await["latest_receipt"]["payload"]["admission_attestation_grant_version"].is_null());
}

#[tokio::test]
async fn trusted_results_release_only_bound_resources_and_do_not_complete_work() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    trusted_runner(&admin).await;
    let c = take(&store).await;
    for outcome in ["succeeded", "failed", "cancelled"] {
        let e = start(&store, &c, outcome).await;
        let cmd = attest(&store, &e, outcome, outcome).await;
        let result = store
            .commands()
            .execute(TENANT, PROJECT, A, cmd.clone())
            .await
            .unwrap();
        let d = &result["receipt"]["data"];
        assert_eq!(d["state"], outcome);
        assert_eq!(d["resources_released"], 1);
        assert_eq!(d["work_completed"], false);
        assert_eq!(d["recovery_blocked"], false);
        assert_eq!(result["execution_authorized"], false);
        let before = snapshot(&admin).await;
        let replay = store
            .commands()
            .execute(TENANT, PROJECT, A, cmd)
            .await
            .unwrap();
        assert_eq!(replay["receipt"], result["receipt"]);
        assert_eq!(snapshot(&admin).await, before);
        let v = inspect(&store, A, &e).await;
        assert_eq!(
            v["latest_receipt"]["payload"]["output_digest"],
            "b".repeat(64)
        );
        assert_eq!(v["latest_receipt"]["receipt_kind"], "trusted_executor");
        assert_eq!(v["automatic_resume"], false);
    }
    let s = snapshot(&admin).await;
    assert!(s["runtime"][0]["selected_completion_id"].is_null());
    assert_eq!(s["resources"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn operator_reconciliation_after_expiry_unblocks_a_new_claim_without_promoting_trust_grade() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let e = report(&store, &e, "observation").await;
    admin
        .batch_execute(
            "UPDATE awr_team.claims SET expires_at=clock_timestamp()-interval '1 second'",
        )
        .await
        .unwrap();
    let cmd = reconcile(&store, &os, &e, "settle", "succeeded").await;
    let result = store
        .commands()
        .execute(TENANT, PROJECT, OP, cmd.clone())
        .await
        .unwrap();
    let d = &result["receipt"]["data"];
    assert_eq!(d["receipt_kind"], "reconcile");
    assert_eq!(d["resources_released"], 1);
    assert_eq!(d["recovery_blocked"], false);
    assert_eq!(d["work_completed"], false);
    let p = prepare(&store, A, "a").await;
    let new=store.commands().execute(TENANT,PROJECT,A,command(&p,"take-again","claim.acquire",
        json!({"session_id":"session-a","expected_session_version":"1","expected_work_version":p["data"]["runtime"]["work_version"],"ttl_seconds":60})))
        .await.unwrap();
    assert_ne!(new["receipt"]["data"]["fence"], c["fence"]);
    let before = snapshot(&admin).await;
    let replay = store
        .commands()
        .execute(TENANT, PROJECT, OP, cmd)
        .await
        .unwrap();
    assert_eq!(replay["receipt"], result["receipt"]);
    assert_eq!(snapshot(&admin).await, before);
    assert_eq!(admin.query_one("SELECT count(*) FROM awr_team.execution_receipts WHERE receipt_kind='trusted_executor'",&[]).await.unwrap().get::<_,i64>(0),0);
}

#[tokio::test]
async fn admin_or_manage_alone_and_agent_self_certification_cannot_reconcile() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let e = report(&store, &e, "observe").await;
    let cmd = reconcile(&store, &os, &e, "settle", "succeeded").await;
    let before = snapshot(&admin).await;
    for sql in [
        "UPDATE awr_team.workstream_grants SET can_reconcile_execution=false WHERE client_id='operator-cli'",
        "UPDATE awr_team.workstream_grants SET can_reconcile_execution=true,can_manage=false WHERE client_id='operator-cli'",
        "UPDATE awr_team.workstream_grants SET can_manage=true WHERE client_id='operator-cli'; UPDATE awr_team.project_memberships SET role='worker' WHERE actor_id='operator'",
        "UPDATE awr_team.project_memberships SET role='admin' WHERE actor_id='operator'; UPDATE awr_team.actors SET kind='agent' WHERE id='operator'",
    ] {
        admin.batch_execute(sql).await.unwrap();
        assert!(matches!(
            store
                .commands()
                .execute(TENANT, PROJECT, OP, cmd.clone())
                .await,
            Err(PgError::Forbidden)
        ));
    }
    assert_eq!(snapshot(&admin).await, before);
}

#[tokio::test]
async fn reconciliation_keeps_other_execution_and_legacy_resource_barriers() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let e = report(&store, &e, "observe").await;
    admin.batch_execute("INSERT INTO awr_team.executions(tenant_id,project_id,id,work_id,fence,contract_hash,executor_actor_id,state)
        VALUES('reader-tenant','reader-project','other-unknown','a',0,'old','old-runner','unknown');
        INSERT INTO awr_team.resource_reservations(tenant_id,project_id,id,work_id,resource_kind,canonical_key,state,execution_id)
        VALUES('reader-tenant','reader-project','other-resource','a','prefix','other','unknown','other-unknown'),
        ('reader-tenant','reader-project','legacy-resource','a','named','legacy','unknown',NULL);
        UPDATE awr_team.work_runtime SET recovery_blocked=false WHERE work_id='a'").await.unwrap();
    let result = store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            OP,
            reconcile(&store, &os, &e, "settle", "succeeded").await,
        )
        .await
        .unwrap();
    let d = &result["receipt"]["data"];
    assert_eq!(d["state"], "succeeded");
    assert_eq!(d["resources_released"], 1);
    assert_eq!(d["recovery_blocked"], true);
    assert_eq!(d["unresolved_work_effects"], true);
    let r = admin
        .query_one(
            "SELECT (SELECT state FROM awr_team.executions WHERE id='other-unknown'),
        (SELECT count(*) FROM awr_team.resource_reservations WHERE state='unknown')",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(r.get::<_, String>(0), "unknown");
    assert_eq!(r.get::<_, i64>(1), 2);
}

#[tokio::test]
async fn stale_receipt_or_work_version_and_diverging_terminal_facts_are_rejected() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let e = report(&store, &e, "observe").await;
    let cmd = reconcile(&store, &os, &e, "settle", "succeeded").await;
    let before = snapshot(&admin).await;
    for (field, value) in [
        ("reviewed_receipt_id", json!("not-reviewed")),
        ("expected_work_version", json!("999")),
        ("expected_execution_version", json!("999")),
    ] {
        let mut bad = cmd.clone();
        bad.args[field] = value;
        assert!(matches!(
            store.commands().execute(TENANT, PROJECT, OP, bad).await,
            Err(PgError::PreconditionsChanged)
        ));
    }
    let mut bad = cmd.clone();
    bad.args["facts"]["input_digest"] = json!("d".repeat(64));
    assert!(matches!(
        store.commands().execute(TENANT, PROJECT, OP, bad).await,
        Err(PgError::PreconditionsChanged)
    ));
    assert_eq!(snapshot(&admin).await, before);
    let r = store
        .commands()
        .execute(TENANT, PROJECT, OP, cmd)
        .await
        .unwrap();
    let e = &r["receipt"]["data"];
    let mut changed = reconcile(&store, &os, e, "rewrite", "succeeded").await;
    changed.args["facts"]["output_digest"] = json!("f".repeat(64));
    let before = snapshot(&admin).await;
    assert!(matches!(
        store.commands().execute(TENANT, PROJECT, OP, changed).await,
        Err(PgError::PreconditionsChanged)
    ));
    assert_eq!(snapshot(&admin).await, before);
}

#[tokio::test]
async fn runner_cannot_clear_a_recovery_block_and_scope_violation_needs_operator_resolution() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    trusted_runner(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let mut cmd = attest(&store, &e, "escaped", "succeeded").await;
    cmd.args["facts"]["observed_paths"] = json!(["outside/output"]);
    let r = store
        .commands()
        .execute(TENANT, PROJECT, A, cmd)
        .await
        .unwrap();
    let e = &r["receipt"]["data"];
    assert_eq!(e["state"], "unknown");
    assert_eq!(e["scope_violation"], true);
    assert_eq!(e["resources_released"], 0);
    let mut cmd = reconcile(&store, &os, e, "confirm-escape", "succeeded").await;
    cmd.args["facts"]["observed_paths"] = json!(["outside/output"]);
    assert!(matches!(
        store.commands().execute(TENANT, PROJECT, OP, cmd).await,
        Err(PgError::ScopeExceeded)
    ));
    // Corrected runner facts can settle the attempt, but only an operator can
    // explicitly review and lift a previously recorded work recovery barrier.
    let r = store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            A,
            attest(&store, e, "verified-stop", "failed").await,
        )
        .await
        .unwrap();
    assert_eq!(r["receipt"]["data"]["state"], "failed");
    assert_eq!(r["receipt"]["data"]["recovery_blocked"], true);
    let r = store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            OP,
            reconcile(
                &store,
                &os,
                &r["receipt"]["data"],
                "operator-clear",
                "failed",
            )
            .await,
        )
        .await
        .unwrap();
    assert_eq!(r["receipt"]["data"]["recovery_blocked"], false);
}

#[tokio::test]
async fn receipt_details_are_visible_only_to_original_client_or_scoped_recovery_operator() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let _os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let e = report(&store, &e, "observe").await;
    admin.execute("INSERT INTO awr_team.workstream_grants(tenant_id,project_id,actor_id,client_id,workstream_id,authority_version,can_read,can_write)
        VALUES($1,$2,'agent','cli-b',$3,1,true,true)",&[&TENANT,&PROJECT,&awr_core::Id::from(1).to_string()]).await.unwrap();
    assert!(inspect(&store, A, &e).await["latest_receipt"]["payload"].is_object());
    assert!(inspect(&store, OP, &e).await["latest_receipt"]["payload"].is_object());
    let b = inspect(&store, B, &e).await;
    assert!(b["latest_receipt"].is_null());
    assert_eq!(b["receipt_details_available"], false);
    admin.batch_execute("UPDATE awr_team.workstream_grants SET can_reconcile_execution=false WHERE client_id='operator-cli'").await.unwrap();
    assert!(inspect(&store, OP, &e).await["latest_receipt"].is_null());
}

#[tokio::test]
async fn failed_recovery_event_rolls_back_receipt_terminal_state_resource_release_and_clear() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    trusted_runner(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let e = report(&store, &e, "observe").await;
    admin.batch_execute("CREATE FUNCTION awr_team.reject_settlement() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
        IF NEW.event_type IN ('execution.attest','execution.reconcile') THEN RAISE EXCEPTION 'synthetic failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER reject_settlement BEFORE INSERT ON awr_team.events FOR EACH ROW EXECUTE FUNCTION awr_team.reject_settlement()").await.unwrap();
    let before = snapshot(&admin).await;
    assert!(matches!(
        store
            .commands()
            .execute(
                TENANT,
                PROJECT,
                A,
                attest(&store, &e, "attest", "succeeded").await
            )
            .await,
        Err(PgError::Db(_))
    ));
    assert_eq!(snapshot(&admin).await, before);
    assert!(matches!(
        store
            .commands()
            .execute(
                TENANT,
                PROJECT,
                OP,
                reconcile(&store, &os, &e, "reconcile", "succeeded").await
            )
            .await,
        Err(PgError::Db(_))
    ));
    assert_eq!(snapshot(&admin).await, before);
}

#[tokio::test]
async fn schema_thirteen_preserves_legacy_resources_and_grants_no_new_authority() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    trusted_runner(&admin).await;
    let claim = take(&store).await;
    let execution = start(&store, &claim, "legacy").await;
    admin.batch_execute("ALTER TABLE awr_team.resource_reservations DROP COLUMN execution_id;
        ALTER TABLE awr_team.executions DROP CONSTRAINT executions_resource_identity;
        ALTER TABLE awr_team.executions DROP COLUMN attestation_grant_version;
        ALTER TABLE awr_team.workstream_grants DROP COLUMN can_attest_execution,DROP COLUMN can_reconcile_execution;
        UPDATE awr_team.schema_state SET version=12;
        INSERT INTO awr_team.resource_reservations(tenant_id,project_id,id,work_id,resource_kind,canonical_key,state)
        VALUES('reader-tenant','reader-project','legacy','a','named','deployment','unknown')").await.unwrap();
    let sql = include_str!("../migrations/20260921000013_workstream_execution_authority.sql");
    assert!(
        admin
            .batch_execute(&sql.replace(
                "UPDATE awr_team.schema_state",
                "SELECT 1/0; UPDATE awr_team.schema_state"
            ))
            .await
            .is_err()
    );
    admin.batch_execute("ROLLBACK").await.unwrap();
    assert_eq!(
        admin
            .query_one("SELECT version FROM awr_team.schema_state", &[])
            .await
            .unwrap()
            .get::<_, i32>(0),
        12
    );
    awr_team_pg::migrate(&admin).await.unwrap();
    awr_team_pg::migrate(&admin).await.unwrap();
    let r=admin.query_one("SELECT (SELECT state FROM awr_team.resource_reservations WHERE id='legacy'),
        (SELECT execution_id FROM awr_team.resource_reservations WHERE id='legacy'),
        (SELECT count(*) FROM awr_team.workstream_grants WHERE can_attest_execution OR can_reconcile_execution)",&[]).await.unwrap();
    assert_eq!(r.get::<_, String>(0), "unknown");
    assert_eq!(r.get::<_, Option<String>>(1), None);
    assert_eq!(r.get::<_, i64>(2), 0);
    let legacy = admin
        .query_one(
            "SELECT state,attestation_grant_version FROM awr_team.executions WHERE id=$1",
            &[&execution["execution_id"].as_str().unwrap()],
        )
        .await
        .unwrap();
    assert_eq!(legacy.get::<_, String>(0), "running");
    assert_eq!(legacy.get::<_, Option<i64>>(1), None);
}

#[tokio::test]
async fn unknown_results_hold_resources_and_settlement_requires_explicit_operator_clear() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    trusted_runner(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let result = store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            A,
            attest(&store, &e, "unknown", "unknown").await,
        )
        .await
        .unwrap();
    let e = result["receipt"]["data"].clone();
    assert_eq!(e["state"], "unknown");
    assert_eq!(e["resources_released"], 0);
    assert_eq!(e["recovery_blocked"], true);
    let bad = reconcile(&store, &os, &e, "clear-unknown", "unknown").await;
    let before = snapshot(&admin).await;
    assert!(matches!(
        store
            .commands()
            .execute(TENANT, PROJECT, OP, bad.clone())
            .await,
        Err(PgError::Protocol(_))
    ));
    assert_eq!(snapshot(&admin).await, before);
    let mut still_unknown = bad;
    still_unknown.args["clear_recovery_block"] = json!(false);
    let r = store
        .commands()
        .execute(TENANT, PROJECT, OP, still_unknown)
        .await
        .unwrap();
    assert_eq!(r["receipt"]["data"]["recovery_blocked"], true);
    let mut settle = reconcile(&store, &os, &e, "settle-only", "failed").await;
    settle.args["clear_recovery_block"] = json!(false);
    let r = store
        .commands()
        .execute(TENANT, PROJECT, OP, settle)
        .await
        .unwrap();
    assert_eq!(r["receipt"]["data"]["state"], "failed");
    assert_eq!(r["receipt"]["data"]["resources_released"], 1);
    assert_eq!(r["receipt"]["data"]["recovery_blocked"], true);
    let r = store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            OP,
            reconcile(&store, &os, &e, "clear-reviewed", "failed").await,
        )
        .await
        .unwrap();
    assert_eq!(r["receipt"]["data"]["recovery_blocked"], false);
}

#[tokio::test]
async fn recovery_cannot_adopt_executions_from_another_epoch_or_ownership_generation() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    trusted_runner(&admin).await;
    let os = operator(&admin, &store).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let attest_cmd = attest(&store, &e, "attest", "succeeded").await;
    let reconcile_cmd = reconcile(&store, &os, &e, "reconcile", "succeeded").await;
    for field in ["coordinator_epoch", "ownership_version"] {
        let sql = if field == "coordinator_epoch" {
            "UPDATE awr_team.executions SET coordinator_epoch='prior';
             UPDATE awr_team.claims SET coordinator_epoch='prior'"
        } else {
            "UPDATE awr_team.executions SET coordinator_epoch='epoch-a',ownership_version=2;
             UPDATE awr_team.claims SET coordinator_epoch='epoch-a'"
        };
        admin.batch_execute(sql).await.unwrap();
        let before = snapshot(&admin).await;
        for (token, cmd) in [(A, attest_cmd.clone()), (OP, reconcile_cmd.clone())] {
            let err = store
                .commands()
                .execute(TENANT, PROJECT, token, cmd)
                .await
                .unwrap_err();
            if field == "coordinator_epoch" {
                assert!(matches!(err, PgError::EpochChanged), "{err:?}");
            } else {
                assert!(matches!(err, PgError::Forbidden), "{err:?}");
            }
        }
        assert_eq!(snapshot(&admin).await, before);
    }
}

#[tokio::test]
async fn a_resource_cannot_be_bound_to_an_execution_of_another_work() {
    let (_g, admin, _, store) = setup().await;
    enable_writes(&admin).await;
    let c = take(&store).await;
    let e = start(&store, &c, "one").await;
    let before = snapshot(&admin).await;
    let err = admin.execute("INSERT INTO awr_team.resource_reservations(tenant_id,project_id,id,work_id,resource_kind,canonical_key,state,execution_id)
        VALUES($1,$2,'wrong-work','b','prefix','other','reserved',$3)",
        &[&TENANT,&PROJECT,&e["execution_id"].as_str().unwrap()]).await.unwrap_err();
    assert_eq!(
        err.code(),
        Some(&tokio_postgres::error::SqlState::FOREIGN_KEY_VIOLATION)
    );
    assert_eq!(snapshot(&admin).await, before);
}

#[tokio::test]
async fn revocation_while_recovery_waits_prevents_receipts_and_resource_release() {
    for recovering in [false, true] {
        let (_g, mut admin, db, store) = setup().await;
        enable_writes(&admin).await;
        trusted_runner(&admin).await;
        let os = operator(&admin, &store).await;
        let c = take(&store).await;
        let e = start(&store, &c, "one").await;
        let (token, cmd, sql) = if recovering {
            (
                OP,
                reconcile(&store, &os, &e, "reconcile", "succeeded").await,
                "UPDATE awr_team.workstream_grants SET can_reconcile_execution=false,grant_version=grant_version+1 WHERE client_id='operator-cli'",
            )
        } else {
            (
                A,
                attest(&store, &e, "attest", "succeeded").await,
                "UPDATE awr_team.workstream_grants SET can_attest_execution=false,grant_version=grant_version+1 WHERE client_id='cli-a'",
            )
        };
        let before = snapshot(&admin).await;
        let observer = common::connect_config(&common::with_db(&common::test_config(), &db)).await;
        let revoke = admin.transaction().await.unwrap();
        revoke.batch_execute(sql).await.unwrap();
        let commands = store.commands();
        let pending =
            tokio::spawn(async move { commands.execute(TENANT, PROJECT, token, cmd).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let blocked: bool = observer.query_one("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()
                    AND wait_event_type='Lock' AND query LIKE '%ORDER BY workstream_id FOR SHARE%')", &[]).await.unwrap().get(0);
                if blocked { break; }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("recovery must wait on current grant");
        revoke.commit().await.unwrap();
        assert!(matches!(pending.await.unwrap(), Err(PgError::Forbidden)));
        assert_eq!(snapshot(&admin).await, before);
    }
}
