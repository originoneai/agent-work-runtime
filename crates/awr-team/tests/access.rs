use awr_team::*;
use serde_json::json;

fn auth() -> AuthContext {
    AuthContext {
        tenant_id: "tenant-a".into(),
        project_id: "project-a".into(),
        actor_id: "actor-a".into(),
        client_id: "client-a".into(),
    }
}

fn remote() -> RemoteProfile {
    RemoteProfile {
        name: "prod".into(),
        endpoint: "https://awr.example/team/v1".into(),
        project_key: "alpha".into(),
        credential_env: "AWR_TEAM_TOKEN".into(),
        protocol_version: 1,
    }
}

#[test]
fn old_client_without_protocol_is_rejected() {
    let err = parse_envelope(&json!({"op":"work.claim","request_id":"r1"})).unwrap_err();
    assert_eq!(err, TeamError::ProtocolUnsupported);
    assert_eq!(err.code(), "PROTOCOL_UNSUPPORTED");
}

#[test]
fn body_cannot_override_authorized_project() {
    let err = authorize(
        &auth(),
        &json!({"project_id":"other","protocol_version":1,"request_id":"r","op":"work.claim"}),
    )
    .unwrap_err();
    assert_eq!(err, TeamError::AuthProjectMismatch);
}

#[test]
fn missing_project_selection_is_rejected() {
    let mut missing = auth();
    missing.project_id.clear();
    let err = authorize(&missing, &json!({})).unwrap_err();
    assert_eq!(err, TeamError::ProjectRequired);
}

#[test]
fn offline_or_missing_remote_cannot_claim() {
    let env = parse_envelope(&json!({
        "protocol_version": 1,
        "request_id": "r1",
        "op": "work.claim",
        "args": {"work_id":"work-a","scope_id":"main","session_id":"s1","expected_work_version":"1","expected_contract_hash":"h"}
    }))
    .unwrap();
    let err = execute("cli", env.clone(), &auth(), None, true).unwrap_err();
    assert_eq!(err, TeamError::OfflineWriteForbidden);
    let err = execute("cli", env, &auth(), Some(&remote()), false).unwrap_err();
    assert_eq!(err, TeamError::OfflineWriteForbidden);
}

#[test]
fn three_surfaces_share_error_and_success_semantics() {
    let env = parse_envelope(&json!({
        "protocol_version": 1,
        "request_id": "r1",
        "op": "work.claim",
        "args": {"work_id":"work-a","scope_id":"main","session_id":"s1","expected_work_version":"1","expected_contract_hash":"h"}
    }))
    .unwrap();
    same_error_on_all_surfaces(env.clone(), &auth(), None, true).unwrap_err();
    assert_eq!(
        same_error_on_all_surfaces(env, &auth(), Some(&remote()), true).unwrap_err(),
        TeamError::Unsupported
    );
}

#[test]
fn profile_does_not_store_secrets_or_postgres_dsn() {
    let mut bad = remote();
    bad.credential_env = "postgres://user:pass@localhost/db".into();
    assert_eq!(bad.validate().unwrap_err(), TeamError::SecretRefInvalid);
    let ok = remote();
    let redacted = ok.redacted().to_string();
    assert!(!redacted.contains("password"));
    assert!(redacted.contains("AWR_TEAM_TOKEN"));
}

#[test]
fn u64_versions_round_trip_without_float_rounding() {
    let value = u64::MAX;
    let encoded = encode_u64(value);
    assert_eq!(encoded, "18446744073709551615");
    assert_eq!(decode_u64(&encoded).unwrap(), value);
    assert!(decode_u64("18446744073709551616").is_err());
}

#[test]
fn unknown_claim_fields_are_rejected() {
    let err = parse_envelope(&json!({
        "protocol_version": 1,
        "request_id": "r1",
        "op": "work.claim",
        "args": {"work_id":"work-a","extra_bypass": true}
    }))
    .unwrap_err();
    assert!(matches!(err, TeamError::UnknownRequiredField(_)));
}

#[test]
fn personal_crates_do_not_depend_on_team_postgres() {
    let cli = include_str!("../../awr-cli/Cargo.toml");
    let mcp = include_str!("../../awr-mcp/Cargo.toml");
    assert!(!cli.contains("awr-team-pg"));
    assert!(!mcp.contains("awr-team-pg"));
}

fn claim() -> serde_json::Value {
    json!({"protocol_version":1,"request_id":"r","op":"work.claim","args":{"work_id":"w","scope_id":"main","session_id":"s","expected_work_version":"1","expected_contract_hash":"h"}})
}
#[test]
fn versions_are_checked_before_narrowing() {
    for v in [
        json!(4294967297u64),
        json!("4294967297"),
        json!(u64::MAX),
        json!("18446744073709551616"),
        json!(-1),
        json!(1.0),
        json!(true),
        json!(null),
        json!(2),
    ] {
        let mut body = claim();
        body["protocol_version"] = v;
        assert_eq!(
            parse_envelope(&body).unwrap_err(),
            TeamError::ProtocolUnsupported
        );
    }
    for v in [json!(1), json!("1")] {
        let mut body = claim();
        body["protocol_version"] = v;
        parse_envelope(&body).unwrap();
    }
}
#[test]
fn claim_arguments_require_shape_fields_and_types() {
    for args in [
        json!(null),
        json!([]),
        json!("text"),
        json!({}),
        json!({"work_id":false,"scope_id":null,"session_id":[],"expected_work_version":{},"expected_contract_hash":42}),
    ] {
        let mut body = claim();
        body["args"] = args;
        assert!(parse_envelope(&body).is_err());
    }
    for key in [
        "work_id",
        "scope_id",
        "session_id",
        "expected_work_version",
        "expected_contract_hash",
    ] {
        let mut body = claim();
        body["args"].as_object_mut().unwrap().remove(key);
        assert!(parse_envelope(&body).is_err());
        let mut body = claim();
        body["args"][key] = json!(false);
        assert!(parse_envelope(&body).is_err());
    }
}
#[test]
fn execute_enforces_independent_identity_and_never_fabricates_a_receipt() {
    for key in ["tenant_id", "project_id", "actor_id", "client_id"] {
        for v in [json!("other"), json!(false), json!(null)] {
            let mut body = claim();
            body[key] = v;
            assert_eq!(
                execute(
                    "http",
                    parse_envelope(&body).unwrap(),
                    &auth(),
                    Some(&remote()),
                    true
                )
                .unwrap_err(),
                TeamError::AuthProjectMismatch
            );
        }
    }
    let env = parse_envelope(&claim()).unwrap();
    let result = validate_only("http", &env, &auth()).unwrap();
    assert_eq!(result["submitted"], false);
    assert_eq!(result["authenticated"], false);
    assert_eq!(result["context"]["actor_id"], "actor-a");
    assert!(result.get("accepted").is_none());
    assert_eq!(
        execute("http", env, &auth(), Some(&remote()), true).unwrap_err(),
        TeamError::Unsupported
    );
}
#[test]
fn unregistered_operations_never_become_cached_reads() {
    for op in [
        "definitely.not.an.operation",
        "execution.cancel",
        "source.activate",
        "work.prepare",
    ] {
        let mut body = claim();
        body["op"] = json!(op);
        assert_eq!(parse_envelope(&body).unwrap_err(), TeamError::Unsupported);
    }
}
#[test]
fn url_structure_and_output_guard_reject_credentials() {
    for endpoint in [
        "https://:review-only-dummy@example.invalid/team/v1",
        "https://user:review-only-dummy@example.invalid",
        "https://user@example.invalid",
        "postgresql://127.0.0.1/db",
        "postgres://127.0.0.1/db",
        "file:///tmp/team",
        "https://example.invalid/?password=review-only-dummy",
        "https://example.invalid/#review-only-dummy",
        "not a url",
        "http://example.invalid/team",
    ] {
        let mut p = remote();
        p.endpoint = endpoint.into();
        assert!(p.validate().is_err(), "{endpoint}");
        assert!(!p.redacted().to_string().contains("review-only-dummy"));
        assert_eq!(p.redacted()["endpoint"], "[invalid endpoint]");
    }
    for endpoint in [
        "https://example.invalid/team/v1",
        "http://localhost:8080/team/v1",
        "http://127.0.0.1:8080/team/v1",
        "http://[::1]:8080/team/v1",
        "https://[2001:db8::1]/team/v1",
    ] {
        let mut p = remote();
        p.endpoint = endpoint.into();
        p.validate().unwrap();
        assert_eq!(p.redacted()["endpoint"], endpoint);
    }
}
