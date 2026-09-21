use std::process::Command;
#[test]
fn native_runner_validates_plans_without_a_database_or_printing_plan_bodies() {
    let dir = std::env::temp_dir().join(format!("awr-runner-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plan.json");
    let private = "PLAN_CONTENT_MUST_NOT_BE_PRINTED";
    std::fs::write(
        &path,
        format!(
            r#"{{"protocol_version":1,"writes":[{{"path":"src/result","content":"{private}"}}]}}"#
        ),
    )
    .unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_awr-server"))
            .env_remove("AWR_TEAM_DATABASE_URL")
            .args(["runner", "digest", "--input", path.to_str().unwrap()])
            .output()
            .unwrap()
    };
    let ok = run();
    assert!(ok.status.success());
    assert!(!String::from_utf8_lossy(&ok.stdout).contains(private));
    std::fs::write(&path,r#"{"protocol_version":1,"writes":[{"path":"../escape","content":"PLAN_CONTENT_MUST_NOT_BE_PRINTED"}]}"#).unwrap();
    let bad = run();
    assert!(!bad.status.success());
    assert!(!String::from_utf8_lossy(&bad.stderr).contains(private));
    assert!(!dir.join("escape").exists());
    let _ = std::fs::remove_dir_all(dir);
}
