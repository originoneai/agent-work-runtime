#![cfg(feature = "pg-tests")]
#[path = "../../awr-team-pg/tests/common/mod.rs"]
mod common;
#[path = "../../awr-team-pg/tests/fixtures/workstream_access.rs"]
mod fixture;
#[path = "../../awr-team-pg/tests/fixtures/scoped_runner.rs"]
mod runner_fixture;
use fixture::*;
use runner_fixture::*;
use serde_json::Value;
use std::process::Command;

fn app_url(db: &str) -> String {
    let raw = common::test_database_url_raw();
    if raw.starts_with("postgres://") || raw.starts_with("postgresql://") {
        let mut u = reqwest::Url::parse(&raw).unwrap();
        u.set_path(&format!("/{db}"));
        let pairs = u
            .query_pairs()
            .filter(|(k, _)| !matches!(k.as_ref(), "dbname" | "user" | "password"))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        u.set_query(None);
        if !pairs.is_empty() {
            u.query_pairs_mut().extend_pairs(pairs);
        }
        u.set_username("awr_app").unwrap();
        u.set_password(Some("app-test")).unwrap();
        u.to_string()
    } else {
        format!("{raw} dbname={db} user=awr_app password=app-test")
    }
}
fn cli(url: &str, args: &[&str]) -> Value {
    let o = Command::new(env!("CARGO_BIN_EXE_awr-server"))
        .env("AWR_TEAM_DATABASE_URL", url)
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!String::from_utf8_lossy(&o.stdout).contains(A));
    assert!(!String::from_utf8_lossy(&o.stderr).contains(A));
    serde_json::from_slice(&o.stdout).unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_scoped_runner_uses_application_role_and_durable_reports_without_reexecution() {
    let (_g, admin, db, store) = setup().await;
    trust(&admin).await;
    let req = request(&store, writes()).await;
    let dir = Directory::new();
    std::fs::create_dir_all(&dir.0).unwrap();
    let token = dir.0.join("credential");
    std::fs::write(&token, A).unwrap();
    let input = dir.0.join("job.json");
    std::fs::write(&input, serde_json::to_vec(&req).unwrap()).unwrap();
    let plan = dir.0.join("plan.json");
    std::fs::write(&plan, serde_json::to_vec(&req.plan).unwrap()).unwrap();
    let url = app_url(&db);
    let digest = cli(
        &url,
        &["runner", "digest", "--input", plan.to_str().unwrap()],
    );
    assert_eq!(digest["input_digest"], req.plan.digest().unwrap());
    assert_eq!(digest["executed"], false);
    let args = [
        "runner",
        "run",
        "--input",
        input.to_str().unwrap(),
        "--credential-file",
        token.to_str().unwrap(),
        "--root",
        dir.0.to_str().unwrap(),
    ];
    let result = cli(&url, &args);
    assert_eq!(result["report_required"], false);
    assert_eq!(result["outcome"]["state"], "succeeded");
    let replay = cli(&url, &args);
    assert_eq!(replay["effects_attempted"], false);
    assert_eq!(replay["replayed"], true);
    let reported = cli(
        &url,
        &[
            "runner",
            "report",
            "--input",
            result["report_request_file"].as_str().unwrap(),
            "--credential-file",
            token.to_str().unwrap(),
            "--root",
            dir.0.to_str().unwrap(),
        ],
    );
    assert_eq!(reported["replayed"], true);
    assert_eq!(reported["receipt"], result["report"]["receipt"]);
    assert_eq!(
        admin
            .query_one("SELECT count(*) FROM awr_team.execution_receipts", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}
