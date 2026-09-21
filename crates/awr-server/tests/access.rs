use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn directory() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("awr-access-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&p).unwrap();
    p
}
fn generate(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_awr-server"))
        .env_remove("AWR_TEAM_DATABASE_URL")
        .args([
            "access",
            "token",
            "--credential-id",
            "test-client",
            "--output",
        ])
        .arg(path)
        .output()
        .unwrap()
}
#[test]
fn generated_credentials_are_private_non_overwriting_and_never_printed() {
    let dir = directory();
    let p = dir.join("credential");
    let output = generate(&p);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let meta: Value = serde_json::from_slice(&output.stdout).unwrap();
    let bearer = std::fs::read_to_string(&p).unwrap();
    let bearer = bearer.trim();
    assert!(bearer.starts_with("awr1.test-client."));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(bearer));
    assert_eq!(meta["registered"], false);
    assert_eq!(
        meta["secret_hash"],
        awr_team_pg::workstream_credential_hash(bearer).unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let existing = std::fs::read(&p).unwrap();
    let failed = generate(&p);
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
    assert!(std::fs::read(&p).unwrap() == existing);
    let q = dir.join("different");
    assert!(generate(&q).status.success());
    assert!(std::fs::read(&q).unwrap() != existing);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn token_generation_refuses_a_symlink_without_touching_its_target() {
    let dir = directory();
    let target = dir.join("target");
    std::fs::write(&target, "retained").unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(!generate(&link).status.success());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "retained");
    std::fs::remove_dir_all(dir).unwrap();
}
