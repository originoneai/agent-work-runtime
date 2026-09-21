#![cfg(feature = "pg-tests")]
//! Explicit opt-in: owns a disposable Docker cluster, takes a real base backup,
//! then starts that historical data directory on a second port. No caller DB URL.
use awr_team_pg::*;
use serde_json::json;
use std::process::Command;
mod common;
fn docker(args: &[&str]) -> String {
    let out = Command::new("docker").args(args).output().unwrap();
    assert!(
        out.status.success(),
        "docker {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
struct Cluster(String);
impl Drop for Cluster {
    fn drop(&mut self) {
        let _ = Command::new("docker").args(["rm", "-fv", &self.0]).output();
    }
}
fn config(name: &str, port: &str) -> tokio_postgres::Config {
    let addr = docker(&["port", name, port]);
    format!(
        "host=127.0.0.1 port={} user=postgres dbname=postgres",
        addr.rsplit(':').next().unwrap()
    )
    .parse()
    .unwrap()
}
#[tokio::test]
#[ignore = "requires Docker postgres:17-alpine; creates and removes only a unique test cluster"]
async fn physical_backup_delayed_delivery_restore_generation() {
    let name = format!("awr-physical-{}", common::nonce(0));
    let image = docker(&[
        "image",
        "inspect",
        "postgres:17-alpine",
        "--format",
        "{{.Id}}",
    ]);
    docker(&[
        "run",
        "-d",
        "--name",
        &name,
        "-e",
        "POSTGRES_HOST_AUTH_METHOD=trust",
        "-p",
        "127.0.0.1::5432",
        "-p",
        "127.0.0.1::5433",
        &image,
    ]);
    let _cluster = Cluster(name.clone());
    let cfg = config(&name, "5432/tcp");
    let mut ready = false;
    for _ in 0..100 {
        if let Ok((c, conn)) = cfg.connect(tokio_postgres::NoTls).await {
            tokio::spawn(async move {
                let _ = conn.await;
            });
            drop(c);
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(ready);
    let admin = common::connect_config(&cfg).await;
    migrate(&admin).await.unwrap();
    admin.batch_execute("CREATE ROLE awr_app LOGIN; INSERT INTO awr_team.tenants(id,name,status) VALUES('t','Test','active'); INSERT INTO awr_team.actors(tenant_id,id,kind,display_name,status) VALUES('t','a','agent','Test','active'); INSERT INTO awr_team.projects(tenant_id,id,key,mode,coordinator_epoch,status) VALUES('t','p','test','team','initial','active');").await.unwrap();
    Bootstrap::grant_app(&admin, "awr_app").await.unwrap();
    let mut app = cfg.clone();
    app.user("awr_app");
    let store = ImportStore::from_config(app.clone());
    let contract = json!({"codec":"awr-team-contract-v1","work_id":"w","external_key":"W","goals":[],"hard_rules":[],"scope_paths":["src"],"acceptance":["output"],"required_dependencies":[],"completion_policy":"ordinary_confirm","verification_requirements":[]});
    store.freeze("t", "p").await.unwrap();
    let job=store.load("t","p","a","seed",&json!({"scopes":["main"],"works":[{"id":"w","external_key":"W","contract":contract}],"evidence":[]})).await.unwrap();
    store.activate("t", "p", &job.id, false).await.unwrap();
    admin
        .batch_execute("UPDATE awr_team.work_runtime SET last_fence=5")
        .await
        .unwrap();
    let backup = store.backup("t", "p", &[], &[]).await.unwrap();
    docker(&[
        "exec",
        "--user",
        "postgres",
        &name,
        "pg_basebackup",
        "-U",
        "postgres",
        "-D",
        "/tmp/awr-recovery",
        "-X",
        "stream",
        "-c",
        "fast",
    ]);
    // Issue and ACK two commands AFTER the physical backup; resource has seen neither.
    let leases = LeaseStore::from_config(app.clone());
    let exec = ExecutionStore::from_config(app);
    let session = leases
        .start_session("t", "p", "a", "c", "conv", "main", "w")
        .await
        .unwrap();
    let mut delayed = vec![];
    for n in [6, 7] {
        let claim = leases
            .claim("t", "p", &session.id, "a", "c", &format!("claim-{n}"), 3600)
            .await
            .unwrap();
        let e = exec
            .prepare(
                "t",
                "p",
                "a",
                "c",
                &format!("prepare-{n}"),
                &claim.id,
                "a",
                "hash",
                "in",
                "hard_fence",
                &["src".into()],
                &json!([{"path":format!("src/old-{n}"),"content":"old"}]),
            )
            .await
            .unwrap();
        assert_eq!(e.fence, n);
        let d = exec.claim_dispatch("t", "p").await.unwrap().unwrap();
        exec.ack_dispatch("t", "p", &d.outbox_id).await.unwrap();
        leases.release("t", "p", &claim.id, "a").await.unwrap();
        delayed.push(d);
    }
    store.freeze("t", "p").await.unwrap();
    docker(&[
        "exec",
        "--user",
        "postgres",
        &name,
        "pg_ctl",
        "-D",
        "/tmp/awr-recovery",
        "-o",
        "-p 5433",
        "-l",
        "/tmp/awr-recovery.log",
        "-w",
        "start",
    ]);
    let restored_cfg = config(&name, "5433/tcp");
    let restored_admin = common::connect_config(&restored_cfg).await;
    assert_eq!(
        restored_admin
            .query_one("SELECT last_fence FROM awr_team.work_runtime", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        5
    );
    assert_eq!(
        restored_admin
            .query_one("SELECT count(*) FROM awr_team.executions", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let mut restored_app = restored_cfg;
    restored_app.user("awr_app");
    let restored = ImportStore::from_config(restored_app.clone());
    let run = restored
        .restore("t", "p", &backup.id, true, false)
        .await
        .unwrap();
    assert_eq!(run.fencing_barriers[0].fence, 6);
    let root = std::env::temp_dir().join(&name);
    let runner = ReferenceRunner::new(&root);
    for barrier in &run.fencing_barriers {
        runner.install_recovery_barrier(barrier).unwrap();
    }
    // Restart resource process state, then deliver equal AND larger old tokens.
    let runner = ReferenceRunner::new(&root);
    for d in &delayed {
        let out = runner.handle_delivery(d, CrashPoint::None);
        assert!(!out.started);
        assert_ne!(out.state, "succeeded");
        assert!(out.error.unwrap().contains("generation"));
    }
    // A post-backup work identity is covered by the same PROJECT ledger.
    let mut unseen = delayed[1].clone();
    unseen.work_id = "created-after-backup".into();
    unseen.execution_id = "unseen".into();
    unseen.effect_key = "unseen".into();
    assert!(
        runner
            .handle_delivery(&unseen, CrashPoint::None)
            .error
            .unwrap()
            .contains("generation")
    );
    assert!(!root.join("worktree/src/old-6").exists());
    assert!(!root.join("worktree/src/old-7").exists());
    // Explicit recovery reconciliation for this test has no uncertain executions.
    restored_admin
        .batch_execute("UPDATE awr_team.work_runtime SET recovery_blocked=FALSE")
        .await
        .unwrap();
    let leases = LeaseStore::from_config(restored_app.clone());
    let exec = ExecutionStore::from_config(restored_app);
    let s = leases
        .start_session("t", "p", "a", "c", "new", "main", "w")
        .await
        .unwrap();
    let c = leases
        .claim("t", "p", &s.id, "a", "c", "new", 3600)
        .await
        .unwrap();
    exec.prepare(
        "t",
        "p",
        "a",
        "c",
        "new-prepare",
        &c.id,
        "a",
        "hash",
        "in",
        "hard_fence",
        &["src".into()],
        &json!([{"path":"src/new","content":"new"}]),
    )
    .await
    .unwrap();
    let new = exec.claim_dispatch("t", "p").await.unwrap().unwrap();
    assert_eq!(new.coordinator_epoch, run.new_epoch);
    assert_eq!(
        runner.handle_delivery(&new, CrashPoint::None).state,
        "succeeded"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("worktree/src/new")).unwrap(),
        "new"
    );
    std::fs::remove_dir_all(root).unwrap();
}
