use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

fn matrix() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/reference/team-v1-evidence-matrix.json"
    ))
    .unwrap()
}

// This guard checks inventory, reference resolution and accounting. It never
// turns source inspection into an executed test or re-accepts historical runs.
fn validate(value: &Value) -> Result<(), String> {
    let require = |ok: bool, message: &str| if ok { Ok(()) } else { Err(message.to_string()) };
    require(value["schema_version"] == 2, "schema version")?;
    require(
        value["release_candidate"] == false && value["tag_pushed"] == false,
        "release flags",
    )?;
    let cases = value["cases"].as_array().ok_or("cases array")?;
    let expected: BTreeSet<String> = (1..=69).map(|n| format!("TC-{n:03}")).collect();
    let ids: BTreeSet<String> = cases
        .iter()
        .map(|c| c["id"].as_str().unwrap_or("").to_owned())
        .collect();
    require(
        cases.len() == ids.len() && ids == expected,
        "exact unique TC-001..TC-069 inventory",
    )?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut required = 0;
    let mut implemented = 0;
    let mut accepted = 0;
    let mut partial = 0;
    let mut pending = 0;
    for case in cases {
        required += usize::from(case["required"].as_bool().ok_or("required boolean")?);
        implemented += usize::from(
            case["protocol_implemented"]
                .as_bool()
                .ok_or("implemented boolean")?,
        );
        let replayed = case["status"] == "real_agent_accepted";
        require(
            case["real_agent_clients"] == replayed,
            "agent status mismatch",
        )?;
        if replayed {
            accepted += 1;
            require(
                case["agent_evidence"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty()),
                "agent evidence missing",
            )?;
        } else {
            require(
                case["status"] == "automated_evidence_pending",
                "unsupported acceptance status",
            )?;
            require(
                case["blocker"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty()),
                "pending blocker missing",
            )?;
            pending += 1;
        }
        require(
            case["automated_coverage"]["status"] == "partial",
            "unsupported coverage claim",
        )?;
        require(
            case["automated_coverage"]["gap"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
            "coverage gap missing",
        )?;
        partial += 1;
        let tests = case["automated_tests"]
            .as_array()
            .ok_or("test references array")?;
        require(!tests.is_empty(), "missing partial test reference")?;
        let mut has_pg = false;
        for test in tests {
            let path = test["path"].as_str().ok_or("test path")?;
            require(
                Path::new(path)
                    .components()
                    .all(|p| matches!(p, Component::Normal(_))),
                "unsafe reference path",
            )?;
            let package = test["package"].as_str().ok_or("package")?;
            let target = test["target"].as_str().ok_or("target")?;
            require(
                path == format!("crates/{package}/tests/{target}.rs"),
                "target/path mismatch",
            )?;
            let source = std::fs::read_to_string(root.join(path))
                .map_err(|_| format!("missing test {path}"))?;
            require(
                test["test_file_sha256"]
                    == format!(
                        "{:x}",
                        Sha256::digest(source.replace("\r\n", "\n").as_bytes())
                    ),
                "stale test source fingerprint",
            )?;
            let file = syn::parse_file(&source).map_err(|_| format!("invalid Rust {path}"))?;
            let function = test["function"].as_str().ok_or("function")?;
            let found = file.items.iter().any(|item| match item {
                syn::Item::Fn(f) => {
                    f.sig.ident == function
                        && f.attrs.iter().any(|a| {
                            let parts: Vec<_> = a
                                .path()
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect();
                            parts == ["test"] || parts == ["tokio", "test"]
                        })
                }
                _ => false,
            });
            require(found, &format!("not a test: {path}::{function}"))?;
            let level = test["level"].as_str().ok_or("level")?;
            require(
                matches!(
                    level,
                    "postgres_integration" | "pure_function" | "cli_process"
                ),
                "unknown level",
            )?;
            has_pg |= level == "postgres_integration";
            require(
                test["features"]
                    == if package == "awr-team-pg" {
                        json!(["pg-tests"])
                    } else {
                        json!([])
                    },
                "test features",
            )?;
            require(
                test["key_assertion"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty()),
                "key assertion missing",
            )?;
            require(
                test["reviewed_source_sha"]
                    .as_str()
                    .is_some_and(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())),
                "review source SHA",
            )?;
            // This version intentionally contains reference mappings only.
            // Executed coverage needs a separate verifiable receipt contract.
            require(
                test.get("last_run") == Some(&Value::Null),
                "unbound execution claim",
            )?;
        }
        require(
            case["real_postgresql"] == has_pg,
            "PostgreSQL level mismatch",
        )?;
    }
    require(required == 69, "all contract cases are required")?;
    require(
        value["counts"]
            == json!({
                "required": required, "protocol_implemented": implemented,
                "real_agent_accepted": accepted, "automated_partial": partial,
                "automated_evidence_pending": pending
            }),
        "derived counts mismatch",
    )?;
    Ok(())
}

#[test]
fn evidence_matrix_has_resolvable_references_and_derived_counts() {
    validate(&matrix()).unwrap();
}

#[test]
fn mutations_cannot_hide_missing_cases_or_fabricate_coverage() {
    let original = matrix();
    for (name, change) in [
        ("duplicate", 0),
        ("outside inventory", 1),
        ("missing file", 2),
        ("implementation count", 3),
        ("required count", 4),
        ("missing case", 5),
        ("release flag", 6),
        ("missing function", 7),
        ("helper is not test", 8),
        ("level mismatch", 9),
        ("fabricated run", 10),
        ("stale fingerprint", 11),
    ] {
        let mut bad = original.clone();
        match change {
            0 => bad["cases"][68]["id"] = json!("TC-001"),
            1 => bad["cases"][68]["id"] = json!("TC-999"),
            2 => {
                bad["cases"][0]["automated_tests"][0]["path"] =
                    json!("crates/awr-team-pg/tests/missing.rs");
                bad["cases"][0]["automated_tests"][0]["target"] = json!("missing")
            }
            3 => {
                for c in bad["cases"].as_array_mut().unwrap() {
                    c["protocol_implemented"] = json!(false);
                }
            }
            4 => {
                for c in bad["cases"].as_array_mut().unwrap() {
                    c["required"] = json!(false);
                }
            }
            5 => {
                bad["cases"].as_array_mut().unwrap().pop();
            }
            6 => bad["release_candidate"] = json!(true),
            7 => bad["cases"][0]["automated_tests"][0]["function"] = json!("imaginary_test"),
            8 => bad["cases"][0]["automated_tests"][0]["function"] = json!("setup"),
            9 => bad["cases"][0]["real_postgresql"] = json!(false),
            10 => bad["cases"][0]["automated_tests"][0]["last_run"] = json!({"passed": true}),
            11 => {
                bad["cases"][0]["automated_tests"][0]["test_file_sha256"] =
                    json!("0000000000000000000000000000000000000000000000000000000000000000")
            }
            _ => unreachable!(),
        }
        assert!(validate(&bad).is_err(), "mutation passed: {name}");
    }
}

#[test]
fn legitimate_acceptance_and_implementation_changes_use_derived_counts() {
    let mut value = matrix();
    value["cases"][0]["status"] = json!("automated_evidence_pending");
    value["cases"][0]["real_agent_clients"] = json!(false);
    value["cases"][0]["blocker"] = json!("Historical acceptance under re-review");
    value["cases"][0]["protocol_implemented"] = json!(false);
    value["counts"]["real_agent_accepted"] = json!(41);
    value["counts"]["automated_evidence_pending"] = json!(28);
    value["counts"]["protocol_implemented"] = json!(66);
    validate(&value).unwrap();
}
