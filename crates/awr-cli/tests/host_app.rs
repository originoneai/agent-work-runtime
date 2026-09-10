//! Exercises the public host example through the actual native CLI, without a TTY.
use std::{path::Path, process::Command};

#[test]
fn public_python_host_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let python = if cfg!(windows) { "python" } else { "python3" };
    let result = Command::new(python)
        .current_dir(root)
        .env("PYTHONUTF8", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("AWR_TEST_BINARY", env!("CARGO_BIN_EXE_awr"))
        .env("AWR_TEST_VERSION", env!("CARGO_PKG_VERSION"))
        .args([
            "-m",
            "unittest",
            "discover",
            "-s",
            "examples/host-app",
            "-v",
        ])
        .output()
        .expect("Python 3.11+ is required to exercise the developer host example");
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stderr));
}
