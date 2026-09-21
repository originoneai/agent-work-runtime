//! Compile this example in baseline commit 41cde746 to exercise REAL schema-8 ingest.
//! Receives only a uniquely owned database name from pg_import's hardened fixture.
use awr_team_pg::{Bootstrap, IngestRequest, SourceFile, SourceStore, migrate};
#[tokio::main]
async fn main() {
    let db = std::env::var("AWR_LEGACY_TEST_DB").unwrap();
    assert!(db.starts_with("awr_team_gate_"));
    let mut cfg: tokio_postgres::Config = std::env::var("AWR_TEAM_TEST_DATABASE_URL")
        .unwrap()
        .parse()
        .unwrap();
    cfg.dbname(&db);
    let (admin, connection) = cfg.connect(tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    // Schema was dropped only by the owning parent fixture, never by this binary.
    migrate(&admin).await.unwrap();
    Bootstrap::grant_app(&admin, "awr_app").await.unwrap();
    admin.batch_execute("INSERT INTO awr_team.tenants(id,name,status) VALUES('tenant-a','Test','active'); INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status) VALUES('tenant-a','actor-a','agent','Test','active'); INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status) VALUES('tenant-a','project-a','test','team','old','active');").await.unwrap();
    cfg.user("awr_app").password("app-test");
    let source = SourceStore::from_config(cfg);
    let contract = serde_json::json!({"codec":"awr-team-contract-v1","work_id":"work-a","external_key":"W","goals":[],"hard_rules":[],"scope_paths":["src"],"acceptance":["output"],"required_dependencies":[],"completion_policy":"ordinary_confirm","verification_requirements":[]});
    source
        .ingest(IngestRequest {
            tenant_id: "tenant-a".into(),
            project_id: "project-a".into(),
            actor_id: "actor-a".into(),
            parser_version: "p1".into(),
            files: vec![SourceFile {
                path: "contract.json".into(),
                bytes: contract.to_string().into_bytes(),
            }],
        })
        .await
        .unwrap();
}
