#![cfg(feature = "pg-tests")]
mod common;
#[path = "fixtures/workstream_access.rs"]
mod fixture;
use awr_team_pg::PgError;
use fixture::*;
use serde_json::{Value, json};

#[tokio::test]
async fn navigation_and_audit_obey_identity_visibility_and_stable_cursors() {
    let (_g, owner, _db, store) = setup().await;
    enable_writes(&owner).await;
    let initial = store
        .query(TENANT, PROJECT, A, query("work.next"))
        .await
        .unwrap();
    assert_eq!(initial["data"]["items"][0]["navigation"], "prepare");
    assert_eq!(
        initial["data"]["items"][1]["navigation"],
        "waiting_dependency"
    );
    owner.batch_execute("INSERT INTO awr_team.work_runtime(tenant_id,project_id,scope_id,work_id,state) SELECT tenant_id,project_id,scope_id,work_id,'pending' FROM awr_team.work_contracts ON CONFLICT DO NOTHING").await.unwrap();
    let caps = store
        .query(TENANT, PROJECT, A, query("capabilities"))
        .await
        .unwrap();
    assert_eq!(caps["identity"]["actor_id"], "agent");
    assert_eq!(caps["identity"]["can_manage_members"], false);
    let next = store
        .query(TENANT, PROJECT, A, query("work.next"))
        .await
        .unwrap();
    assert_eq!(next["data"]["resume"][0]["session_id"], "session-a");
    assert!(!next.to_string().contains("session-b"));
    assert!(!next.to_string().contains("b-private"));
    assert_eq!(next["data"]["execution_authorized"], false);
    let items = next["data"]["items"].as_array().unwrap();
    assert_eq!(
        items.iter().find(|w| w["work_id"] == "a").unwrap()["navigation"],
        "prepare"
    );
    assert_eq!(
        items.iter().find(|w| w["work_id"] == "c").unwrap()["navigation"],
        "waiting_dependency"
    );
    let mut q = query("work.next");
    q.limit = Some(1);
    let page = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    q.cursor = page["data"]["next_cursor"].as_str().map(str::to_owned);
    let page2 = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_ne!(
        page["data"]["items"][0]["work_id"],
        page2["data"]["items"][0]["work_id"]
    );
    assert!(matches!(
        store.query(TENANT, PROJECT, B, q).await,
        Err(PgError::CursorExpired)
    ));
    owner.batch_execute("INSERT INTO awr_team.sessions(tenant_id,project_id,id,scope_id,work_id,actor_id,client_id,conversation_id,state,workstream_id,ownership_version) SELECT tenant_id,project_id,'other-session',scope_id,work_id,'reviewer','review-cli','other-conversation','active',workstream_id,ownership_version FROM awr_team.sessions WHERE id='session-a'; INSERT INTO awr_team.claims(tenant_id,project_id,id,scope_id,work_id,session_id,actor_id,fence,expires_at,state) VALUES('reader-tenant','reader-project','other-claim','main','a','other-session','reviewer',1,clock_timestamp()+interval '5 minutes','active')").await.unwrap();
    let held = store
        .query(TENANT, PROJECT, A, query("work.next"))
        .await
        .unwrap();
    assert_eq!(held["data"]["items"][0]["navigation"], "held");
    owner.batch_execute("UPDATE awr_team.claims SET expires_at=clock_timestamp()-interval '1 minute' WHERE id='other-claim'").await.unwrap();
    let expired = store
        .query(TENANT, PROJECT, A, query("work.next"))
        .await
        .unwrap();
    assert_eq!(
        expired["data"]["items"][0]["navigation"],
        "recovery_required"
    );
    owner.batch_execute("DELETE FROM awr_team.claims WHERE id='other-claim'; DELETE FROM awr_team.sessions WHERE id='other-session'").await.unwrap();
    let prepared = prepare(&store, A, "a").await;
    store
        .commands()
        .execute(
            TENANT,
            PROJECT,
            A,
            command(
                &prepared,
                "navigation-start",
                "session.start",
                json!({"conversation_id":"navigation"}),
            ),
        )
        .await
        .unwrap();
    let development = store
        .query(TENANT, PROJECT, A, query("audit.development"))
        .await
        .unwrap();
    assert!(
        development["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["action"] == "session.start"
                && r["client_id"] == "cli-a"
                && r["work_id"] == "a")
    );
    assert!(!development.to_string().contains("payload"));
    assert!(!development.to_string().contains("b-private"));
    let first = store
        .request_audit_begin(TENANT, PROJECT, A, "work.prepare", Some("a"))
        .await
        .unwrap();
    store
        .request_audit_finish(TENANT, PROJECT, &first, "succeeded")
        .await
        .unwrap();
    let second = store
        .request_audit_begin(TENANT, PROJECT, A, "access.preview", Some("b-private"))
        .await
        .unwrap();
    store
        .request_audit_finish(TENANT, PROJECT, &second, "denied")
        .await
        .unwrap();
    let mut q = query("audit.requests");
    q.limit = Some(1);
    let page = store.query(TENANT, PROJECT, A, q.clone()).await.unwrap();
    assert_eq!(page["data"]["items"][0]["result"], "denied");
    assert!(page["data"]["items"][0]["work_id"].is_null());
    assert_eq!(page["data"]["items"][0]["credential_id"], "reader-a");
    q.cursor = page["data"]["next_cursor"].as_str().map(str::to_owned);
    // A newer request must not repeat or displace already paged records.
    let unknown = store
        .request_audit_begin(TENANT, PROJECT, A, "work.next", None)
        .await
        .unwrap();
    let next = store.query(TENANT, PROJECT, A, q).await.unwrap();
    assert_eq!(next["data"]["items"][0]["id"], first);
    let all = store
        .query(TENANT, PROJECT, A, query("audit.requests"))
        .await
        .unwrap();
    assert!(
        all["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == unknown && r["result"] == "unknown")
    );
    assert!(!all.to_string().contains(A));
    assert!(!all.to_string().contains("secret_hash"));
    owner.batch_execute("UPDATE awr_team.project_memberships SET role='developer',membership_version=membership_version+1 WHERE actor_id='agent'; INSERT INTO awr_team.request_audit(tenant_id,project_id,id,actor_id,client_id,credential_id,action) VALUES('reader-tenant','reader-project','other-record','reviewer','review-cli','review-credential','work.next')").await.unwrap();
    let own = store
        .query(TENANT, PROJECT, A, query("audit.requests"))
        .await
        .unwrap();
    assert_eq!(own["data"]["scope"], "self");
    assert!(
        own["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["actor_id"] == "agent")
    );
    let mut other = query("audit.requests");
    other.member_actor_id = Some("reviewer".into());
    assert!(matches!(
        store.query(TENANT, PROJECT, A, other).await,
        Err(PgError::Forbidden)
    ));
    let mut other = query("audit.development");
    other.member_actor_id = Some("reviewer".into());
    assert!(matches!(
        store.query(TENANT, PROJECT, A, other).await,
        Err(PgError::Forbidden)
    ));
    assert!(matches!(
        store.query(TENANT, PROJECT, NONE, query("work.next")).await,
        Err(PgError::Forbidden)
    ));
    let rows: Value = owner
        .query_one(
            "SELECT coalesce(jsonb_agg(to_jsonb(a)), '[]'::jsonb) FROM awr_team.request_audit a",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!rows.to_string().contains(A));
}
