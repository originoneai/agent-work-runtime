#![cfg(feature = "pg-tests")]
mod common;
#[path = "fixtures/workstream_access.rs"]
mod fixture;
use awr_team_pg::{AccessPlan, OperatorAccess, PgError, workstream_credential_hash};
use fixture::*;
use serde_json::{Value, json};
use tokio_postgres::Client;

const TOKEN: &str =
    "awr1.provisioned.dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
fn plan() -> AccessPlan {
    serde_json::from_value(json!({"protocol_version":1,"tenant_id":TENANT,"project_id":PROJECT,
        "actor":{"id":"provisioned","kind":"system","display_name":"Reference executor"},
        "client_id":"provisioned-client","role":"worker",
        "grants":[{"workstream_id":awr_core::Id::from(1),"authority_version":"1","read":true,"write":true,
        "manage":false,"attest_execution":true,"reconcile_execution":false}],
        "credential":{"id":"provisioned","secret_hash":workstream_credential_hash(TOKEN).unwrap(),"expires_at_unix_ms":null},
        "revoke_credentials":[]})).unwrap()
}
async fn digest(admin: &mut Client, p: &AccessPlan) -> String {
    OperatorAccess::preview(admin, p).await.unwrap()["state_digest"]
        .as_str()
        .unwrap()
        .into()
}
async fn state(admin: &Client) -> Value {
    admin.query_one("SELECT jsonb_build_object(
        'actors',(SELECT jsonb_agg(to_jsonb(a) ORDER BY tenant_id,id) FROM awr_team.actors a),
        'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY tenant_id,project_id,actor_id) FROM awr_team.project_memberships m),
        'credentials',(SELECT jsonb_agg(to_jsonb(c) ORDER BY tenant_id,id) FROM awr_team.credentials c),
        'grants',(SELECT jsonb_agg(to_jsonb(g) ORDER BY tenant_id,project_id,actor_id,client_id,workstream_id) FROM awr_team.workstream_grants g),
        'changes',(SELECT jsonb_agg(to_jsonb(c) ORDER BY tenant_id,project_id,request_id) FROM awr_team.access_changes c),
        'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY id) FROM awr_team.events e),
        'projects',(SELECT jsonb_agg(to_jsonb(p) ORDER BY tenant_id,id) FROM awr_team.projects p))",&[]).await.unwrap().get(0)
}

#[tokio::test]
async fn preview_is_read_only_and_provisioned_client_can_only_use_its_granted_workstream() {
    let (_g, mut admin, _, store) = setup().await;
    let p = plan();
    let before = state(&admin).await;
    let preview = OperatorAccess::preview(&mut admin, &p).await.unwrap();
    assert_eq!(state(&admin).await, before);
    assert!(
        !preview
            .to_string()
            .contains(&p.credential.as_ref().unwrap().secret_hash)
    );
    assert!(!preview.to_string().contains(TOKEN));
    let applied = OperatorAccess::apply(
        &mut admin,
        &p,
        "register",
        preview["state_digest"].as_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(applied["receipt"]["execution_authorized"], false);
    assert!(applied["receipt"]["previous_policy"]["actor"].is_null());
    assert_eq!(
        applied["receipt"]["current_policy"]["actor"]["kind"],
        "system"
    );
    assert!(
        !applied
            .to_string()
            .contains(&p.credential.as_ref().unwrap().secret_hash)
    );
    assert_eq!(prepare(&store, TOKEN, "a").await["data"]["work_id"], "a");
    let mut q = query("work.prepare");
    q.work_id = Some("b-private".into());
    assert!(matches!(
        store.query(TENANT, PROJECT, TOKEN, q).await,
        Err(PgError::Forbidden)
    ));
    assert_eq!(
        prepare(&store, B, "b-private").await["data"]["work_id"],
        "b-private"
    );
    let inspected = OperatorAccess::inspect(
        &mut admin,
        TENANT,
        PROJECT,
        "provisioned",
        "provisioned-client",
    )
    .await
    .unwrap();
    assert_eq!(inspected["state"]["grants"][0]["attest_execution"], true);
    assert_eq!(inspected["state"]["grants"][0]["version"], "1");
}

#[tokio::test]
async fn exact_replay_is_historical_and_cannot_restore_later_revoked_access() {
    let (_g, mut admin, _, store) = setup().await;
    let p = plan();
    let d = digest(&mut admin, &p).await;
    let original = OperatorAccess::apply(&mut admin, &p, "register", &d)
        .await
        .unwrap();
    let before = state(&admin).await;
    assert_eq!(
        OperatorAccess::apply(&mut admin, &p, "register", &d)
            .await
            .unwrap()["receipt"],
        original["receipt"]
    );
    assert_eq!(state(&admin).await, before);
    let mut changed = p.clone();
    changed.role = "reader".into();
    changed.grants.clear();
    changed.credential = None;
    changed.revoke_credentials = vec!["provisioned".into()];
    let d2 = digest(&mut admin, &changed).await;
    OperatorAccess::apply(&mut admin, &changed, "revoke", &d2)
        .await
        .unwrap();
    let before = state(&admin).await;
    assert_eq!(
        OperatorAccess::apply(&mut admin, &p, "register", &d)
            .await
            .unwrap()["receipt"],
        original["receipt"]
    );
    assert_eq!(state(&admin).await, before);
    assert!(matches!(
        store
            .query(TENANT, PROJECT, TOKEN, query("capabilities"))
            .await,
        Err(PgError::Forbidden)
    ));
    let current = OperatorAccess::inspect(&mut admin, TENANT, PROJECT, &p.actor.id, &p.client_id)
        .await
        .unwrap();
    assert_eq!(current["state"]["membership"]["version"], "2");
    assert_eq!(current["state"]["grants"][0]["active"], false);
    assert_eq!(current["state"]["grants"][0]["version"], "2");
    assert!(current["state"]["credentials"][0]["revoked_at_unix_ms"].is_number());
    assert_eq!(
        OperatorAccess::outcome(&mut admin, TENANT, PROJECT, "register")
            .await
            .unwrap()["receipt"],
        original["receipt"]
    );
    assert_eq!(
        OperatorAccess::outcome(&mut admin, TENANT, PROJECT, "missing")
            .await
            .unwrap()["outcome"],
        "unknown"
    );
    assert!(matches!(
        OperatorAccess::apply(&mut admin, &changed, "register", &d).await,
        Err(PgError::IdempotencyConflict)
    ));
    assert!(matches!(
        OperatorAccess::preview(&mut admin, &p).await,
        Err(PgError::PreconditionsChanged)
    ));
}

#[tokio::test]
async fn service_database_role_is_not_an_operator_and_cannot_read_or_forge_operator_receipts() {
    let (_g, mut admin, db, _) = setup().await;
    let p = plan();
    let d = digest(&mut admin, &p).await;
    OperatorAccess::apply(&mut admin, &p, "register", &d)
        .await
        .unwrap();
    let mut app = common::app_client(&db).await;
    assert!(matches!(
        OperatorAccess::inspect(&mut app, TENANT, PROJECT, &p.actor.id, &p.client_id).await,
        Err(PgError::Forbidden)
    ));
    assert!(matches!(
        OperatorAccess::preview(&mut app, &p).await,
        Err(PgError::Forbidden)
    ));
    assert!(matches!(
        OperatorAccess::apply(&mut app, &p, "register", &d).await,
        Err(PgError::Forbidden)
    ));
    assert!(matches!(
        OperatorAccess::outcome(&mut app, TENANT, PROJECT, "register").await,
        Err(PgError::Forbidden)
    ));
    for sql in [
        "SELECT * FROM awr_team.access_changes",
        "DELETE FROM awr_team.access_changes",
        "INSERT INTO awr_team.access_changes SELECT * FROM awr_team.access_changes",
    ] {
        assert_eq!(
            app.batch_execute(sql).await.unwrap_err().code(),
            Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
        );
    }
}

#[tokio::test]
async fn stale_preview_and_concurrent_replacements_do_not_lose_policy_changes() {
    let (_g, mut admin, db, _) = setup().await;
    let p = plan();
    let d = digest(&mut admin, &p).await;
    let mut second = common::connect_config(&common::with_db(&common::test_config(), &db)).await;
    let (a, b) = tokio::join!(
        OperatorAccess::apply(&mut admin, &p, "one", &d),
        OperatorAccess::apply(&mut second, &p, "two", &d)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(
        a.err().or(b.err()).unwrap(),
        PgError::PreconditionsChanged
    ));
    let before = state(&admin).await;
    assert!(matches!(
        OperatorAccess::apply(&mut admin, &p, "stale", &d).await,
        Err(PgError::PreconditionsChanged)
    ));
    assert_eq!(state(&admin).await, before);
    let fresh = digest(&mut admin, &p).await;
    let (a, b) = tokio::join!(
        OperatorAccess::apply(&mut admin, &p, "same", &fresh),
        OperatorAccess::apply(&mut second, &p, "same", &fresh)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a["replayed"], b["replayed"]);
    assert_eq!(a["receipt"], b["receipt"]);
}

#[tokio::test]
async fn grant_replacement_preserves_other_clients_and_reactivation_increments_version() {
    let (_g, mut admin, _, store) = setup().await;
    let mut p = plan();
    p.actor = awr_team_pg::AccessActor {
        id: "agent".into(),
        kind: "agent".into(),
        display_name: "Worker".into(),
    };
    p.client_id = "cli-a".into();
    p.role = "admin".into();
    p.credential = None;
    p.grants.clear();
    let d = digest(&mut admin, &p).await;
    OperatorAccess::apply(&mut admin, &p, "remove-alpha", &d)
        .await
        .unwrap();
    assert!(matches!(
        store.query(TENANT, PROJECT, A, query("capabilities")).await,
        Err(PgError::Forbidden)
    ));
    assert_eq!(
        prepare(&store, B, "b-private").await["data"]["work_id"],
        "b-private"
    );
    p.grants = plan().grants;
    p.grants[0].attest_execution = false;
    let d = digest(&mut admin, &p).await;
    OperatorAccess::apply(&mut admin, &p, "restore-alpha", &d)
        .await
        .unwrap();
    assert_eq!(prepare(&store, A, "a").await["data"]["work_id"], "a");
    let v = OperatorAccess::inspect(&mut admin, TENANT, PROJECT, "agent", "cli-a")
        .await
        .unwrap();
    assert_eq!(v["state"]["grants"][0]["version"], "3");
    let mut wrong = p.clone();
    wrong.revoke_credentials = vec!["reader-b".into()];
    assert!(matches!(
        OperatorAccess::preview(&mut admin, &wrong).await,
        Err(PgError::Forbidden)
    ));
}

#[tokio::test]
async fn invalid_trust_or_identity_or_credential_changes_are_rejected_without_mutation() {
    let (_g, mut admin, _, _) = setup().await;
    let mut cases = vec![];
    let mut p = plan();
    p.actor.kind = "agent".into();
    cases.push(p);
    let mut p = plan();
    p.role = "reader".into();
    cases.push(p);
    let mut p = plan();
    p.grants[0].reconcile_execution = true;
    cases.push(p);
    let mut p = plan();
    p.grants[0].authority_version = "2".into();
    cases.push(p);
    let mut p = plan();
    p.grants[0].workstream_id = awr_core::Id::from(99);
    cases.push(p);
    let mut p = plan();
    p.grants.push(p.grants[0].clone());
    cases.push(p);
    let mut p = plan();
    p.credential.as_mut().unwrap().expires_at_unix_ms = Some(1);
    cases.push(p);
    let mut p = plan();
    p.credential.as_mut().unwrap().id = "reader-a".into();
    cases.push(p);
    let mut p = plan();
    p.actor.id = "agent".into();
    cases.push(p);
    let before = state(&admin).await;
    for p in cases {
        assert!(OperatorAccess::preview(&mut admin, &p).await.is_err());
    }
    assert_eq!(state(&admin).await, before);
    let p = plan();
    let d = digest(&mut admin, &p).await;
    OperatorAccess::apply(&mut admin, &p, "first", &d)
        .await
        .unwrap();
    let before = state(&admin).await;
    let mut changed = p.clone();
    changed.actor.kind = "human".into();
    changed.grants[0].attest_execution = false;
    assert!(matches!(
        OperatorAccess::preview(&mut admin, &changed).await,
        Err(PgError::PreconditionsChanged)
    ));
    changed = p.clone();
    changed.credential.as_mut().unwrap().secret_hash = format!("sha256:{}", "e".repeat(64));
    assert!(matches!(
        OperatorAccess::preview(&mut admin, &changed).await,
        Err(PgError::PreconditionsChanged)
    ));
    assert_eq!(state(&admin).await, before);
}

#[tokio::test]
async fn failed_audit_rolls_back_actor_membership_credentials_grants_and_receipt() {
    let (_g, mut admin, _, _) = setup().await;
    admin.batch_execute("CREATE FUNCTION awr_team.reject_access() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
        IF NEW.event_type='access.changed' THEN RAISE EXCEPTION 'synthetic failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER reject_access BEFORE INSERT ON awr_team.events FOR EACH ROW EXECUTE FUNCTION awr_team.reject_access()").await.unwrap();
    let p = plan();
    let d = digest(&mut admin, &p).await;
    let before = state(&admin).await;
    assert!(matches!(
        OperatorAccess::apply(&mut admin, &p, "rollback", &d).await,
        Err(PgError::Db(_))
    ));
    assert_eq!(state(&admin).await, before);
    assert_eq!(
        OperatorAccess::outcome(&mut admin, TENANT, PROJECT, "rollback")
            .await
            .unwrap()["outcome"],
        "unknown"
    );
}

#[tokio::test]
async fn schema_fourteen_is_atomic_and_preserves_access_without_granting_operator_privilege() {
    let (_g, admin, db, _) = setup().await;
    admin
        .batch_execute(
            "DROP TABLE awr_team.access_changes; UPDATE awr_team.schema_state SET version=13",
        )
        .await
        .unwrap();
    let before: Value = admin
        .query_one(
            "SELECT jsonb_agg(to_jsonb(g) ORDER BY client_id) FROM awr_team.workstream_grants g",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let sql = include_str!("../migrations/20260921000014_operator_access.sql");
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
        13
    );
    awr_team_pg::migrate(&admin).await.unwrap();
    awr_team_pg::Bootstrap::grant_app(&admin, "awr_app")
        .await
        .unwrap();
    assert_eq!(admin.query_one("SELECT jsonb_agg(to_jsonb(g) ORDER BY client_id) FROM awr_team.workstream_grants g",&[]).await.unwrap().get::<_,Value>(0),before);
    let mut app = common::app_client(&db).await;
    assert!(matches!(
        OperatorAccess::preview(&mut app, &plan()).await,
        Err(PgError::Forbidden)
    ));
}
