use serde_json::{Value, json};
use std::process::{Command, Output};
fn claim() -> Value {
    json!({"protocol_version":1,"request_id":"r","op":"work.claim","args":{"work_id":"w","scope_id":"main","session_id":"s","expected_work_version":"1","expected_contract_hash":"h"}})
}
fn run(body: &Value, validation: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_awr-server"));
    c.env_remove("AWR_TEAM_DATABASE_URL");
    if validation {
        c.args([
            "validate-command",
            "--tenant-id",
            "tenant-a",
            "--project-id",
            "project-a",
            "--actor-id",
            "actor-a",
            "--client-id",
            "client-a",
        ]);
    } else {
        c.arg("command");
    }
    c.args(["--op", "work.claim", "--body", &body.to_string()])
        .output()
        .unwrap()
}
fn error(out: Output) -> Value {
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty());
    serde_json::from_slice(&out.stderr).expect("structured error, not panic")
}
#[test]
fn received_envelopes_are_never_filled_or_panicked() {
    for body in [json!([]), json!(true), json!(7), json!("text"), json!(null)] {
        assert_eq!(error(run(&body, false))["code"], "InvalidInput");
    }
    for key in ["protocol_version", "request_id", "op"] {
        let mut body = claim();
        body.as_object_mut().unwrap().remove(key);
        let e = error(run(&body, false));
        assert_eq!(e["code"], "PROTOCOL_UNSUPPORTED");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_awr-server"))
        .args(["command", "--op", "work.claim"])
        .output()
        .unwrap();
    assert_eq!(error(out)["code"], "InvalidInput");
    let mut body = claim();
    body["op"] = json!("capabilities");
    body["args"] = json!({});
    assert_eq!(error(run(&body, false))["code"], "InvalidInput");
}
#[test]
fn service_does_not_authorize_a_body_against_itself() {
    assert_eq!(error(run(&claim(), false))["code"], "Unsupported");
    for key in ["tenant_id", "project_id", "actor_id", "client_id"] {
        let mut body = claim();
        body[key] = json!("other");
        assert_eq!(error(run(&body, true))["code"], "FORBIDDEN");
    }
    let out = run(&claim(), true);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["validation_only"], true);
    assert_eq!(v["submitted"], false);
    assert_eq!(v["authenticated"], false);
    assert_eq!(v["context"]["tenant_id"], "tenant-a");
    assert!(v.get("accepted").is_none());
}
#[test]
fn actual_receiver_rejects_oversized_versions_and_bad_claims() {
    for version in [
        json!(4294967297u64),
        json!("4294967297"),
        json!(-1),
        json!(true),
    ] {
        let mut b = claim();
        b["protocol_version"] = version;
        assert_eq!(error(run(&b, false))["code"], "PROTOCOL_UNSUPPORTED");
    }
    for args in [json!({}), json!([]), json!("text")] {
        let mut b = claim();
        b["args"] = args;
        assert_eq!(error(run(&b, true))["code"], "InvalidInput");
    }
    let mut b = claim();
    b["op"] = json!("unknown.operation");
    assert_eq!(error(run(&b, false))["code"], "Unsupported");
}
