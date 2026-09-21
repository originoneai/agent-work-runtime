#![cfg(feature = "pg-tests")]

use awr_team_pg::LeaseStore;
use std::time::Instant;
mod common;
use common::{fresh_team_schema, test_config, with_app_role};

#[tokio::test]
async fn records_combined_workflow_rate_without_declaring_an_sla() {
    let (_guard, admin, db) = fresh_team_schema().await;
    admin
        .batch_execute(
            "INSERT INTO awr_team.tenants(id,name,status) VALUES ('tenant-a','A','active');
             INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status)
                VALUES ('tenant-a','actor-a','agent','A','active');
             INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status)
                VALUES ('tenant-a','project-a','alpha','team','epoch-1','active');
             INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
                VALUES ('tenant-a','project-a','main','main','active');",
        )
        .await
        .unwrap();
    let store = LeaseStore::from_config(with_app_role(&test_config(), &db));
    let n = 20usize;
    let start = Instant::now();
    for i in 0..n {
        let work = format!("work-{i}");
        admin
            .execute(
                "INSERT INTO awr_team.work_items(tenant_id,project_id,id,external_key) VALUES ('tenant-a','project-a',$1,$1)",
                &[&work],
            )
            .await
            .unwrap();
        let session = store
            .start_session(
                "tenant-a",
                "project-a",
                "actor-a",
                "client-a",
                &format!("c{i}"),
                "main",
                &work,
            )
            .await
            .unwrap();
        store
            .claim(
                "tenant-a",
                "project-a",
                &session.id,
                "actor-a",
                "client-a",
                &format!("r{i}"),
                60,
            )
            .await
            .unwrap();
    }
    let elapsed = start.elapsed();
    let pg: String = admin
        .query_one("SHOW server_version", &[])
        .await
        .unwrap()
        .get(0);
    let rate = n as f64 / elapsed.as_secs_f64().max(0.001);
    // Recorded measurement only. Do not treat this as a production SLA.
    assert!(rate > 0.0);
    assert!(pg.starts_with('1'));
    let report = serde_json::json!({
        "metric": "combined_workflows_per_second",
        "steps": ["insert_work_item", "start_session", "claim"],
        "clients": 1, "workflows": n, "elapsed_seconds": elapsed.as_secs_f64(),
        "workflows_per_second": rate, "postgres_version": pg,
        "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "source_sha": std::env::var("AWR_TEST_SOURCE_SHA").ok(),
        "source_dirty": std::env::var("AWR_TEST_SOURCE_DIRTY").ok(),
        "measured_at_unix_ms": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis(),
        "production_sla": false
    });
    eprintln!("capacity_probe={report}");
    if let Ok(path) = std::env::var("AWR_CAPACITY_REPORT") {
        assert!(
            report["source_sha"].as_str().is_some_and(|s| s.len() == 40),
            "AWR_TEST_SOURCE_SHA is required for a persisted measurement"
        );
        assert!(
            report["source_dirty"]
                .as_str()
                .is_some_and(|s| s == "true" || s == "false"),
            "AWR_TEST_SOURCE_DIRTY must describe the measured working tree"
        );
        // Never silently replace a previous measurement.
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        writeln!(file, "{}", serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
}
