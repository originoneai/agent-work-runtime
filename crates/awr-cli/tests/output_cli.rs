use serde_json::Value;
use std::process::Command;

#[test]
fn json_usage_errors_work_before_and_after_subcommands() {
    for args in [
        vec!["--json", "work", "ready"],
        vec!["ready", "--limit", "invalid", "--json"],
        vec!["--json", "work", "show"],
        vec!["context", "compile", "--json", "--checkpoint", "invalid"],
        vec!["--json", "ready", "--unknown"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "InvalidInput");
        assert!(!error["message"].as_str().unwrap().is_empty());
    }
}

#[test]
fn literal_json_text_does_not_change_error_format() {
    for args in [
        vec!["work", "show", "--", "--json", "extra"],
        vec!["--project=--json", "--unknown"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(serde_json::from_slice::<Value>(&output.stderr).is_err());
    }
}

#[test]
fn help_version_and_unsupported_have_explicit_exit_contracts() {
    for flag in ["--help", "--version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_awr"))
            .args(["--json", flag])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_awr"))
        .args(["--json", "unsupported-command"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "Unsupported"
    );
}
