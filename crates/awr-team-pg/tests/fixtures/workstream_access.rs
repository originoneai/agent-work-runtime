#![allow(dead_code)]
use crate::common;
use awr_core::{Id, Workstream, WorkstreamCatalog, WorkstreamState};
use awr_team::{SourceActivationPlan, WorkContract, WorkId, WorkstreamBundle, WorkstreamContract};
use awr_team_pg::{
    IngestRequest, SourceFile, SourceStore, WorkstreamQuery, WorkstreamReadStore,
    workstream_credential_hash,
};
use std::sync::MutexGuard;
use tokio_postgres::Client;

pub const TENANT: &str = "reader-tenant";
pub const PROJECT: &str = "reader-project";
pub const A: &str =
    "awr1.reader-a.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const B: &str =
    "awr1.reader-b.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub const NONE: &str =
    "awr1.no-grants.cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

pub fn query(op: &str) -> WorkstreamQuery {
    serde_json::from_value(serde_json::json!({"protocol_version":1,"op":op})).unwrap()
}

pub async fn setup() -> (MutexGuard<'static, ()>, Client, String, WorkstreamReadStore) {
    let (guard, admin, db) = common::fresh_team_schema().await;
    admin.batch_execute("INSERT INTO awr_team.tenants(id,name,status) VALUES('reader-tenant','Readers','active'),('other-tenant','Other','active');
        INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status) VALUES
          ('reader-tenant','agent','agent','Worker','active'),('reader-tenant','reviewer','human','Reviewer','active');
        INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status) VALUES
          ('reader-tenant','reader-project','p','team','epoch-a','active'),('other-tenant','reader-project','p','team','epoch-b','active');
        INSERT INTO awr_team.project_memberships(tenant_id,project_id,actor_id,role) VALUES
          ('reader-tenant','reader-project','agent','admin'),('reader-tenant','reader-project','reviewer','reviewer');
        INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
          VALUES('reader-tenant','reader-project','main','Main','active');").await.unwrap();
    for (id, client, token) in [
        ("reader-a", "cli-a", A),
        ("reader-b", "cli-b", B),
        ("no-grants", "unscoped", NONE),
    ] {
        let hash = workstream_credential_hash(token).unwrap();
        admin
            .execute(
                "INSERT INTO awr_team.credentials(tenant_id,id,actor_id,client_id,secret_hash)
            VALUES($1,$2,'agent',$3,$4)",
                &[&TENANT, &id, &client, &hash],
            )
            .await
            .unwrap();
    }
    let definitions = vec![(1, "alpha"), (2, "private-beta")]
        .into_iter()
        .map(|(i, key)| Workstream {
            id: Id::from(i),
            project_id: PROJECT.into(),
            external_key: key.into(),
            title: key.into(),
            state: WorkstreamState::Active,
            authority_version: 1,
            goal_keys: vec![key.into()],
            acceptance_contracts: vec![],
        })
        .collect();
    let contracts = vec![
        ("a", 1, vec![]),
        ("b-private", 2, vec![]),
        ("c", 1, vec!["b-private"]),
    ]
    .into_iter()
    .map(|(id, stream, deps)| WorkstreamContract {
        workstream_id: Id::from(stream),
        contract: WorkContract {
            codec: WorkContract::CODEC.into(),
            work_id: WorkId::new(id).unwrap(),
            external_key: id.into(),
            goals: vec!["ship".into()],
            hard_rules: vec!["preserve compatibility".into()],
            scope_paths: vec!["src".into()],
            acceptance: vec!["verified".into()],
            required_dependencies: deps.into_iter().map(Into::into).collect(),
            completion_policy: "review".into(),
            verification_requirements: vec!["report".into()],
        },
    })
    .collect();
    let bundle = WorkstreamBundle {
        codec: WorkstreamBundle::CODEC.into(),
        catalog: WorkstreamCatalog {
            version: 1,
            project_id: PROJECT.into(),
            legacy_default: None,
            workstreams: definitions,
        },
        contracts,
    };
    let source = SourceStore::from_config(common::with_app_role(&common::test_config(), &db));
    let c = source
        .ingest(IngestRequest {
            tenant_id: TENANT.into(),
            project_id: PROJECT.into(),
            actor_id: "agent".into(),
            parser_version: "workstreams/1".into(),
            files: vec![SourceFile {
                path: "workstreams.json".into(),
                bytes: serde_json::to_vec(&bundle).unwrap(),
            }],
        })
        .await
        .unwrap();
    source
        .approve(
            TENANT,
            PROJECT,
            &c.proposal_id,
            "reviewer",
            &c.manifest_digest,
        )
        .await
        .unwrap();
    source
        .activate_workstreams(
            TENANT,
            PROJECT,
            "agent",
            &c.proposal_id,
            &SourceActivationPlan {
                candidate_digest: c.manifest_digest.clone(),
                parser_version: c.parser_version.clone(),
                expected_authority_epoch: c.base_epoch.clone(),
                approved_candidate_digest: c.manifest_digest.clone(),
            },
        )
        .await
        .unwrap();
    for (client, stream) in [("cli-a", 1), ("cli-b", 2)] {
        admin.execute("INSERT INTO awr_team.workstream_grants(tenant_id,project_id,actor_id,client_id,workstream_id,authority_version,can_read)
            VALUES($1,$2,'agent',$3,$4,1,true)", &[&TENANT,&PROJECT,&client,&Id::from(stream).to_string()]).await.unwrap();
    }
    for (index, stream, work, session, next, client) in [
        (2, 1, "a", "session-a", "continue alpha", "cli-a"),
        (
            3,
            2,
            "b-private",
            "session-b",
            "PRIVATE NEXT ACTION",
            "cli-b",
        ),
    ] {
        let stream = Id::from(stream).to_string();
        admin.execute("INSERT INTO awr_team.sessions(tenant_id,project_id,id,scope_id,work_id,actor_id,client_id,conversation_id,state,workstream_id,ownership_version)
            VALUES($1,$2,$3,'main',$4,'agent',$6,$3,'active',$5,1)", &[&TENANT,&PROJECT,&session,&work,&stream,&client]).await.unwrap();
        let checkpoint = format!("cp-{session}");
        admin.execute("INSERT INTO awr_team.checkpoints(tenant_id,project_id,id,session_id,context_hash,contract_hash,observed_revision,next_action,open_loops_json)
            VALUES($1,$2,$3,$4,'context','contract',1,$5,'[]')", &[&TENANT,&PROJECT,&checkpoint,&session,&next]).await.unwrap();
        admin
            .execute(
                "UPDATE awr_team.sessions SET latest_checkpoint_id=$1 WHERE id=$2",
                &[&checkpoint, &session],
            )
            .await
            .unwrap();
        admin.execute("INSERT INTO awr_team.events(tenant_id,project_id,id,project_revision,event_index,event_type,actor_id,work_id,payload_json,workstream_id)
            VALUES($1,$2,$3,$4,0,'session.started','agent',$5,'{}',$6)", &[&TENANT,&PROJECT,&format!("ev-{session}"),&(index as i64),&work,&stream]).await.unwrap();
    }
    admin
        .execute(
            "UPDATE awr_team.projects SET project_revision=3 WHERE tenant_id=$1 AND id=$2",
            &[&TENANT, &PROJECT],
        )
        .await
        .unwrap();
    let store =
        WorkstreamReadStore::from_config(common::with_app_role(&common::test_config(), &db));
    (guard, admin, db, store)
}
