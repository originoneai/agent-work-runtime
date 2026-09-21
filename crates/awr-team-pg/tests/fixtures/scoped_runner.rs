#![allow(dead_code)]
use crate::{common, fixture::*};
use awr_team_pg::{ReferenceRunRequest, ReferenceWrite, ReferenceWritePlan, WorkstreamReadStore};
use serde_json::json;
use std::path::PathBuf;
use tokio_postgres::Client;

pub struct Directory(pub PathBuf);
impl Directory {
    pub fn new() -> Self {
        Self(std::env::temp_dir().join(format!("awr-scoped-runner-{}", common::nonce(0))))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub async fn trust(admin: &Client) {
    enable_writes(admin).await;
    admin.batch_execute("UPDATE awr_team.actors SET kind='system' WHERE tenant_id='reader-tenant' AND id='agent';
        UPDATE awr_team.workstream_grants SET can_attest_execution=true,grant_version=grant_version+1 WHERE client_id='cli-a'").await.unwrap();
}
pub async fn request(
    store: &WorkstreamReadStore,
    writes: Vec<ReferenceWrite>,
) -> ReferenceRunRequest {
    let c=store.commands().execute(TENANT,PROJECT,A,command(&prepare(store,A,"a").await,"runner-claim","claim.acquire",
        json!({"session_id":"session-a","expected_session_version":"1","expected_work_version":"0","ttl_seconds":60}))).await.unwrap()["receipt"]["data"].clone();
    let plan = ReferenceWritePlan {
        protocol_version: 1,
        writes,
    };
    let hash = plan.digest().unwrap();
    let p = prepare(store, A, "a").await;
    let e=store.commands().execute(TENANT,PROJECT,A,command(&p,"runner-prepare","execution.prepare",
        json!({"session_id":"session-a","expected_session_version":"1","claim_id":c["claim_id"],"expected_fence":c["fence"],"expected_lease_version":c["lease_version"],
            "expected_work_version":p["data"]["runtime"]["work_version"],"input_digest":hash,"declared_scope":["src/api"]}))).await.unwrap()["receipt"]["data"].clone();
    let p = prepare(store, A, "a").await;
    ReferenceRunRequest {
        tenant_id: TENANT.into(),
        project_id: PROJECT.into(),
        plan,
        command: command(
            &p,
            "runner-start",
            "execution.start",
            json!({"session_id":"session-a","expected_session_version":"1",
            "execution_id":e["execution_id"],"expected_execution_version":e["execution_version"],
            "claim_id":c["claim_id"],"expected_fence":c["fence"],"expected_lease_version":c["lease_version"],
            "expected_work_version":p["data"]["runtime"]["work_version"],"execution_mode":"reference_write_v1","expected_input_digest":hash}),
        ),
    }
}
pub fn writes() -> Vec<ReferenceWrite> {
    vec![ReferenceWrite {
        path: "src/api/result.txt".into(),
        content: "verified artifact\n".into(),
    }]
}
