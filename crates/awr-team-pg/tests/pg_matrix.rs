use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

const HISTORICAL_AGENT_INDEX_PATH: &str =
    "docs/reference/team-v1-historical-agent-evidence-v1.json";
const HISTORICAL_AGENT_INDEX_SHA256: &str =
    "45e97fc92ed40a79eabb263105f3bbc207c359b747156ff0ef6a8a044ddb5b78";

fn matrix() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/dev/reference/team-v1-evidence-matrix.json"
    ))
    .unwrap()
}

fn historical_agent_index_source() -> &'static str {
    include_str!("../../../docs/dev/reference/team-v1-historical-agent-evidence-v1.json")
}

fn historical_agent_index() -> Value {
    serde_json::from_str(historical_agent_index_source()).unwrap()
}

fn normalized_source_sha256(source: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(source.replace("\r\n", "\n").as_bytes())
    )
}

fn case_mut<'a>(value: &'a mut Value, id: &str) -> &'a mut Value {
    value["cases"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|case| case["id"] == id)
        .unwrap()
}

fn normalized_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    let value = value.as_str().ok_or_else(|| format!("{field} string"))?;
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(format!("{field} normalized non-empty string"));
    }
    Ok(value)
}

fn hex_string(value: &Value, length: usize, field: &str) -> Result<(), String> {
    let value = normalized_string(value, field)?;
    if value.len() != length || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{field} hex length {length}"));
    }
    Ok(())
}

fn case_definition_fingerprint(case: &Value) -> Result<String, String> {
    let id = normalized_string(&case["id"], "case id")?;
    let title = normalized_string(&case["title"], "case title")?;
    let phase = normalized_string(&case["phase"], "case phase")?;
    let required = case["required"].as_bool().ok_or("required boolean")?;
    let canonical = format!(
        "{{\"id\":{},\"title\":{},\"phase\":{},\"required\":{required}}}",
        serde_json::to_string(id).unwrap(),
        serde_json::to_string(title).unwrap(),
        serde_json::to_string(phase).unwrap(),
    );
    Ok(format!("{:x}", Sha256::digest(canonical.as_bytes())))
}

fn validate_historical_agent_evidence(
    matrix: &Value,
    index: &Value,
    index_source_sha256: &str,
) -> Result<(), String> {
    let require = |ok: bool, message: &str| if ok { Ok(()) } else { Err(message.to_string()) };
    require(
        index_source_sha256 == HISTORICAL_AGENT_INDEX_SHA256,
        "unsealed historical agent index",
    )?;
    require(index["schema_version"] == 1, "historical index schema")?;
    require(
        index["index_id"] == "team-p13-pr50-historical-agent-evidence-v1",
        "historical index identity",
    )?;
    require(
        index["index_seal_algorithm"] == "sha256(UTF-8 bytes with CRLF normalized to LF)",
        "historical index seal algorithm",
    )?;
    require(
        index["review_state"] == "historical_pending_review",
        "historical index review state",
    )?;
    require(
        index["declaration_source"]
            == json!({
                "git_commit_sha": "a9b7123aa3f31a729dd180f03a378faabf1c0a42",
                "matrix_git_blob_sha1": "e1e25d211d331ab18263da17b576e1eab5ad51ae",
                "matrix_file_sha256": "b6761eda49cdf5e62853c6eac99df12ef2db80ccb9b8277823211fefd6dee685",
                "completion_manifest_sha256": "9cdad25c6fee024722db7d79321acf6121a6bfbb82baa1723fc89e1e8f69c27f"
            }),
        "historical declaration source",
    )?;
    require(
        index["case_definition_fingerprint"]
            == json!({
                "version": "team-v1-case-definition-v1",
                "algorithm": "sha256(compact JSON object with ordered id,title,phase,required fields)",
                "historical_run_binding": null
            }),
        "case definition fingerprint contract",
    )?;

    let retained = index["retained_material_review"]
        .as_object()
        .ok_or("retained material review object")?;
    require(
        retained.get("observed_at") == Some(&json!("2026-09-22T06:47:37.925265+00:00")),
        "retained review time",
    )?;
    require(
        retained.get("reviewed_main_sha")
            == Some(&json!("85698e4adff721659100059dfc870b58879ac294")),
        "retained review source",
    )?;
    hex_string(
        retained
            .get("retained_driver_sha256")
            .ok_or("retained driver fingerprint missing")?,
        64,
        "retained driver fingerprint",
    )?;
    require(
        retained.get("retained_driver_historical_binding") == Some(&Value::Null),
        "retained driver must remain unbound",
    )?;
    require(
        retained.get("full_chain_reverified") == Some(&json!(false)),
        "historical chain was not reverified",
    )?;
    require(
        retained.get("summary_hashes_match_completion_manifest") == Some(&json!(true)),
        "summary manifest review",
    )?;
    require(
        retained.get("existing_referenced_artifact_mismatches") == Some(&json!(0)),
        "retained artifact mismatch count",
    )?;
    require(
        retained.get("case_ids_with_missing_referenced_temporary_outputs")
            == Some(&json!([
                "TC-026", "TC-031", "TC-039", "TC-041", "TC-042", "TC-043", "TC-059", "TC-069"
            ])),
        "missing temporary artifact disclosure",
    )?;

    let cases = matrix["cases"].as_array().ok_or("cases array")?;
    let mut cases_by_id = BTreeMap::new();
    let mut historical_case_ids = BTreeSet::new();
    for case in cases {
        let id = normalized_string(&case["id"], "case id")?;
        cases_by_id.insert(id, case);
        if case["status"] == "real_agent_accepted" {
            historical_case_ids.insert(id);
        }
    }

    let entries = index["entries"]
        .as_array()
        .ok_or("historical entries array")?;
    require(
        retained.get("case_summary_count").and_then(Value::as_u64) == Some(entries.len() as u64),
        "historical summary count",
    )?;
    let mut entry_ids = BTreeSet::new();
    for entry in entries {
        let object = entry.as_object().ok_or("historical entry object")?;
        require(object.len() == 8, "historical entry fields")?;
        let id = normalized_string(&entry["case_id"], "historical case id")?;
        require(entry_ids.insert(id), "duplicate historical case entry")?;
        let case = cases_by_id.get(id).ok_or("unknown historical case entry")?;

        let definition = entry["case_definition"]
            .as_object()
            .ok_or("case definition object")?;
        require(definition.len() == 5, "case definition fields")?;
        require(
            definition.get("id") == Some(&case["id"])
                && definition.get("title") == Some(&case["title"])
                && definition.get("phase") == Some(&case["phase"])
                && definition.get("required") == Some(&case["required"]),
            "case definition mismatch",
        )?;
        require(
            definition.get("sha256") == Some(&json!(case_definition_fingerprint(case)?)),
            "case definition fingerprint mismatch",
        )?;

        let evidence = entry["evidence"]
            .as_object()
            .ok_or("historical evidence object")?;
        require(evidence.len() == 2, "historical evidence fields")?;
        require(
            evidence.get("archive_ref") == Some(&json!(format!("TEAM-P13/{id}.json"))),
            "historical evidence case locator mismatch",
        )?;
        let summary_sha = evidence
            .get("sha256")
            .ok_or("historical evidence fingerprint missing")?;
        hex_string(summary_sha, 64, "historical evidence fingerprint")?;

        let binding = entry["binding"]
            .as_object()
            .ok_or("historical binding object")?;
        require(binding.len() == 6, "historical binding fields")?;
        for field in [
            "tested_source_git_sha",
            "historical_driver_sha256",
            "run_id",
            "run_at",
            "participant_identities",
        ] {
            require(
                binding.get(field) == Some(&Value::Null),
                "unverified historical binding must remain unknown",
            )?;
        }
        let oracle = binding
            .get("oracle")
            .and_then(Value::as_object)
            .ok_or("historical oracle object")?;
        require(oracle.len() == 3, "historical oracle fields")?;
        require(
            oracle.get("expected") == Some(&Value::Null)
                && oracle.get("independent_identity") == Some(&Value::Null),
            "unverified oracle binding must remain unknown",
        )?;
        require(
            oracle.get("observed")
                == Some(&json!({
                    "claimed_result": "pass",
                    "bound_summary_sha256": summary_sha
                })),
            "historical oracle observation mismatch",
        )?;

        let (scope, unverified_claims) = if id == "TC-069" {
            (
                "shared_admission_function_comparison",
                json!([
                    "real_http_transport_unverified",
                    "real_mcp_transport_unverified",
                    "real_cli_transport_unverified"
                ]),
            )
        } else {
            (
                "retained_summary_integrity_only",
                json!(["business_outcome_not_independently_reverified"]),
            )
        };
        require(
            entry["supported_scope"] == scope,
            "historical evidence scope",
        )?;
        require(
            entry["unverified_claims"] == unverified_claims,
            "historical unverified claims",
        )?;
        require(
            entry["review_state"] == "historical_pending_review"
                && entry["current_verified"] == false,
            "historical entry must remain pending review",
        )?;
        require(
            case["agent_evidence"]
                == json!({
                    "index_entry": id,
                    "summary_sha256": summary_sha,
                    "review_state": "historical_pending_review"
                }),
            "case historical evidence binding mismatch",
        )?;
    }
    require(
        entry_ids == historical_case_ids,
        "historical claims must exactly match sealed entries",
    )?;
    require(
        matrix["historical_agent_evidence"]
            == json!({
                "index_path": HISTORICAL_AGENT_INDEX_PATH,
                "index_sha256": HISTORICAL_AGENT_INDEX_SHA256,
                "historical_claims": entries.len(),
                "historical_pending_review": entries.len(),
                "current_verified": 0
            }),
        "historical evidence summary",
    )?;
    Ok(())
}

fn validate_live_agent_run(value: &Value) -> Result<(), String> {
    let require = |ok: bool, message: &str| if ok { Ok(()) } else { Err(message.to_string()) };
    let run = value.as_object().ok_or("live_agent_run object")?;
    require(
        run.get("schema_version") == Some(&json!(1)),
        "live schema version",
    )?;
    require(
        run.get("status") == Some(&json!("locally_verified")),
        "live verified status",
    )?;
    hex_string(
        run.get("git_head").ok_or("live git head missing")?,
        40,
        "live git head",
    )?;

    let clients = run
        .get("clients")
        .and_then(Value::as_array)
        .ok_or("live clients array")?;
    require(clients.len() == 2, "exactly two live clients")?;
    require(
        clients[0]["product"] == "Kimi Code CLI" && clients[1]["product"] == "ZCode CLI",
        "live client order",
    )?;
    normalized_string(&clients[0]["version"], "first client version")?;
    normalized_string(&clients[1]["version"], "second client version")?;
    let first_actor = normalized_string(&clients[0]["actor_id"], "first actor")?;
    let second_actor = normalized_string(&clients[1]["actor_id"], "second actor")?;
    require(first_actor != second_actor, "live actors must be distinct")?;

    normalized_string(
        run.get("project_key").ok_or("live project missing")?,
        "live project",
    )?;
    normalized_string(run.get("work_id").ok_or("live work missing")?, "live work")?;
    normalized_string(
        run.get("receipt_id").ok_or("live receipt missing")?,
        "live receipt",
    )?;
    normalized_string(
        run.get("execution_id").ok_or("live execution missing")?,
        "live execution",
    )?;
    normalized_string(run.get("flow").ok_or("live flow missing")?, "live flow")?;
    let evidence_path = normalized_string(
        run.get("evidence_path")
            .ok_or("live evidence locator missing")?,
        "live evidence locator",
    )?;
    require(
        Path::new(evidence_path)
            .components()
            .all(|part| matches!(part, Component::Normal(_))),
        "unsafe live evidence locator",
    )?;

    let oracle = run
        .get("oracle")
        .and_then(Value::as_object)
        .ok_or("live oracle object")?;
    require(
        oracle.get("receipts").and_then(Value::as_u64) == Some(1),
        "live receipt count",
    )?;
    require(
        oracle.get("executions").and_then(Value::as_u64) == Some(1),
        "live execution count",
    )?;
    require(
        oracle.get("active_holder").and_then(Value::as_str) == Some(second_actor),
        "live successor holder",
    )?;
    require(
        oracle.get("pass").and_then(Value::as_bool) == Some(true),
        "live oracle pass",
    )?;
    require(
        run.get("not_a_release_tag").and_then(Value::as_bool) == Some(true),
        "live release limitation",
    )?;

    let review = run
        .get("historical_evidence_review")
        .and_then(Value::as_object)
        .ok_or("historical evidence review object")?;
    require(
        review.get("schema_version") == Some(&json!(1)),
        "historical evidence review schema version",
    )?;
    normalized_string(
        review
            .get("observed_at")
            .ok_or("historical evidence observation missing")?,
        "historical evidence observed_at",
    )?;
    hex_string(
        review
            .get("evidence_bundle_sha256")
            .ok_or("historical evidence bundle hash missing")?,
        64,
        "historical evidence bundle hash",
    )?;
    hex_string(
        review
            .get("retained_driver_sha256")
            .ok_or("retained driver hash missing")?,
        64,
        "retained driver hash",
    )?;
    require(
        review.get("retained_driver_binding") == Some(&json!("unbound")),
        "retained driver binding",
    )?;
    require(
        review.get("run_time") == Some(&Value::Null),
        "historical run time must remain unknown",
    )?;
    require(
        review.get("full_chain_reverified").and_then(Value::as_bool) == Some(false),
        "historical full chain was not reverified",
    )?;

    let current = run
        .get("current_verification")
        .and_then(Value::as_object)
        .ok_or("current verification object")?;
    require(
        current.get("live_rerun").and_then(Value::as_bool) == Some(false),
        "current live run was not rerun",
    )?;
    Ok(())
}

// This guard checks inventory, reference resolution and accounting. It never
// turns source inspection into an executed test or re-accepts historical runs.
fn validate(value: &Value) -> Result<(), String> {
    let require = |ok: bool, message: &str| if ok { Ok(()) } else { Err(message.to_string()) };
    require(value["schema_version"] == 3, "schema version")?;
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
            require(
                case.get("agent_evidence").is_none(),
                "pending case cannot claim agent evidence",
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
    let historical_source = historical_agent_index_source();
    let historical_source_sha = normalized_source_sha256(historical_source);
    validate_historical_agent_evidence(value, &historical_agent_index(), &historical_source_sha)?;
    validate_live_agent_run(&value["live_agent_run"])?;
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
        ("unsupported matrix schema", 12),
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
            12 => bad["schema_version"] = json!(2),
            _ => unreachable!(),
        }
        assert!(validate(&bad).is_err(), "mutation passed: {name}");
    }
}

#[test]
fn sealed_historical_agent_evidence_accepts_the_versioned_positive_control() {
    let source = historical_agent_index_source();
    let lf_source = source.replace("\r\n", "\n");
    validate_historical_agent_evidence(
        &matrix(),
        &serde_json::from_str(&lf_source).unwrap(),
        &normalized_source_sha256(&lf_source),
    )
    .unwrap();
    let crlf_source = lf_source.replace('\n', "\r\n");
    validate_historical_agent_evidence(
        &matrix(),
        &serde_json::from_str(&crlf_source).unwrap(),
        &normalized_source_sha256(&crlf_source),
    )
    .unwrap();
}

#[test]
fn case_binding_mutations_cannot_reuse_or_promote_historical_evidence() {
    let original = matrix();
    for name in [
        "copy TC-001 binding to TC-002",
        "copy TC-001 binding to every historical case",
        "arbitrary non-empty evidence text",
        "P11 evidence path",
        "promote TC-005 with TC-001 binding",
        "change summary fingerprint",
        "change case definition",
        "change index path",
        "change index fingerprint",
        "claim current verification",
        "pending case smuggles evidence",
    ] {
        let mut bad = original.clone();
        let first_binding = case_mut(&mut bad, "TC-001")["agent_evidence"].clone();
        match name {
            "copy TC-001 binding to TC-002" => {
                case_mut(&mut bad, "TC-002")["agent_evidence"] = first_binding
            }
            "copy TC-001 binding to every historical case" => {
                for case in bad["cases"].as_array_mut().unwrap() {
                    if case["status"] == "real_agent_accepted" {
                        case["agent_evidence"] = first_binding.clone();
                    }
                }
            }
            "arbitrary non-empty evidence text" => {
                case_mut(&mut bad, "TC-002")["agent_evidence"] = json!("unverified")
            }
            "P11 evidence path" => {
                case_mut(&mut bad, "TC-002")["agent_evidence"] =
                    json!("ledger/evidence/TEAM-P11/live-dual-cli.json")
            }
            "promote TC-005 with TC-001 binding" => {
                let case = case_mut(&mut bad, "TC-005");
                case["status"] = json!("real_agent_accepted");
                case["real_agent_clients"] = json!(true);
                case["agent_evidence"] = first_binding;
                bad["counts"]["real_agent_accepted"] = json!(43);
                bad["counts"]["automated_evidence_pending"] = json!(26);
            }
            "change summary fingerprint" => {
                case_mut(&mut bad, "TC-002")["agent_evidence"]["summary_sha256"] =
                    json!("a".repeat(64))
            }
            "change case definition" => {
                case_mut(&mut bad, "TC-002")["title"] = json!("Changed acceptance definition")
            }
            "change index path" => {
                bad["historical_agent_evidence"]["index_path"] =
                    json!("ledger/evidence/TEAM-P11/live-dual-cli.json")
            }
            "change index fingerprint" => {
                bad["historical_agent_evidence"]["index_sha256"] = json!("b".repeat(64))
            }
            "claim current verification" => {
                bad["historical_agent_evidence"]["current_verified"] = json!(42)
            }
            "pending case smuggles evidence" => {
                case_mut(&mut bad, "TC-005")["agent_evidence"] = first_binding
            }
            _ => unreachable!(),
        }
        assert!(validate(&bad).is_err(), "mutation passed: {name}");
    }
}

#[test]
fn sealed_index_mutations_cannot_fill_unknown_historical_bindings() {
    let matrix = matrix();
    let original = historical_agent_index();
    for name in [
        "case definition copied",
        "unsupported index schema",
        "tested source invented",
        "driver binding invented",
        "run identity invented",
        "participant identity invented",
        "oracle expectation invented",
        "independent oracle invented",
        "evidence locator crossed",
        "current verification invented",
        "TC-069 transport scope inflated",
        "duplicate entry",
    ] {
        let mut bad = original.clone();
        match name {
            "case definition copied" => {
                bad["entries"][1]["case_definition"] = bad["entries"][0]["case_definition"].clone()
            }
            "unsupported index schema" => bad["schema_version"] = json!(2),
            "tested source invented" => {
                bad["entries"][0]["binding"]["tested_source_git_sha"] = json!("a".repeat(40))
            }
            "driver binding invented" => {
                bad["entries"][0]["binding"]["historical_driver_sha256"] = json!("a".repeat(64))
            }
            "run identity invented" => {
                bad["entries"][0]["binding"]["run_id"] = json!("run-claimed")
            }
            "participant identity invented" => {
                bad["entries"][0]["binding"]["participant_identities"] =
                    json!(["kimi-cli", "zcode-cli"])
            }
            "oracle expectation invented" => {
                bad["entries"][0]["binding"]["oracle"]["expected"] = json!("pass")
            }
            "independent oracle invented" => {
                bad["entries"][0]["binding"]["oracle"]["independent_identity"] = json!("reviewer")
            }
            "evidence locator crossed" => {
                bad["entries"][1]["evidence"]["archive_ref"] = json!("TEAM-P13/TC-001.json")
            }
            "current verification invented" => bad["entries"][0]["current_verified"] = json!(true),
            "TC-069 transport scope inflated" => {
                let entry = bad["entries"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|entry| entry["case_id"] == "TC-069")
                    .unwrap();
                entry["supported_scope"] = json!("real_http_mcp_cli_transports")
            }
            "duplicate entry" => bad["entries"][1] = bad["entries"][0].clone(),
            _ => unreachable!(),
        }
        assert!(
            validate_historical_agent_evidence(&matrix, &bad, HISTORICAL_AGENT_INDEX_SHA256)
                .is_err(),
            "mutation passed: {name}"
        );
    }
    assert!(
        validate_historical_agent_evidence(&matrix, &original, &"0".repeat(64)).is_err(),
        "an unsealed index source passed"
    );
    let mutated_source = historical_agent_index_source().replacen(
        "Hash agreement locates retained summaries",
        "Hash agreement locates changed summaries",
        1,
    );
    assert!(
        validate_historical_agent_evidence(
            &matrix,
            &serde_json::from_str(&mutated_source).unwrap(),
            &normalized_source_sha256(&mutated_source),
        )
        .is_err(),
        "modified index bytes passed the source seal"
    );
}

#[test]
fn legitimate_implementation_changes_still_use_derived_counts() {
    let mut value = matrix();
    value["cases"][0]["protocol_implemented"] = json!(false);
    value["counts"]["protocol_implemented"] = json!(66);
    validate(&value).unwrap();
}

#[test]
fn live_agent_run_accepts_new_well_formed_observation_identifiers() {
    let mut value = matrix();
    let run = &mut value["live_agent_run"];
    run["git_head"] = json!("0123456789abcdef0123456789abcdef01234567");
    run["clients"][0]["version"] = json!("2.1.0");
    run["clients"][0]["actor_id"] = json!("kimi-successor-flow-a");
    run["clients"][1]["version"] = json!("0.17.0");
    run["clients"][1]["actor_id"] = json!("zcode-successor-flow-b");
    run["project_key"] = json!("p11-live-next");
    run["work_id"] = json!("work-p11-next");
    run["receipt_id"] = json!("receipt-next");
    run["execution_id"] = json!("execution-next");
    run["evidence_path"] = json!("ledger/evidence/TEAM-P11/live-dual-cli-next.json");
    run["oracle"]["active_holder"] = json!("zcode-successor-flow-b");
    run["historical_evidence_review"]["evidence_bundle_sha256"] = json!("a".repeat(64));
    run["historical_evidence_review"]["retained_driver_sha256"] = json!("b".repeat(64));
    validate(&value).unwrap();
}

#[test]
fn live_agent_run_mutations_cannot_fabricate_a_successful_handoff() {
    let original = matrix();
    for name in [
        "receipts zero",
        "receipts two",
        "executions zero",
        "executions two",
        "holder reverted to first actor",
        "actors equal",
        "actor whitespace pseudo difference",
        "actor control character",
        "receipt empty",
        "execution empty",
        "evidence empty",
        "evidence traversal",
        "oracle failed",
        "wrong product",
        "release limitation missing",
        "run structure type",
        "clients structure type",
        "oracle structure type",
        "required field missing",
        "receipt count structure type",
        "git head malformed",
        "provenance missing",
        "provenance hash malformed",
        "retained driver falsely bound",
        "historical run time fabricated",
        "historical chain falsely reverified",
        "current run falsely claimed",
    ] {
        let mut bad = original.clone();
        match name {
            "receipts zero" => bad["live_agent_run"]["oracle"]["receipts"] = json!(0),
            "receipts two" => bad["live_agent_run"]["oracle"]["receipts"] = json!(2),
            "executions zero" => bad["live_agent_run"]["oracle"]["executions"] = json!(0),
            "executions two" => bad["live_agent_run"]["oracle"]["executions"] = json!(2),
            "holder reverted to first actor" => {
                bad["live_agent_run"]["oracle"]["active_holder"] = json!("kimi-cli")
            }
            "actors equal" => bad["live_agent_run"]["clients"][1]["actor_id"] = json!("kimi-cli"),
            "actor whitespace pseudo difference" => {
                bad["live_agent_run"]["clients"][1]["actor_id"] = json!("kimi-cli ")
            }
            "actor control character" => {
                bad["live_agent_run"]["clients"][1]["actor_id"] = json!("zcode\ncli")
            }
            "receipt empty" => bad["live_agent_run"]["receipt_id"] = json!(""),
            "execution empty" => bad["live_agent_run"]["execution_id"] = json!(""),
            "evidence empty" => bad["live_agent_run"]["evidence_path"] = json!(""),
            "evidence traversal" => {
                bad["live_agent_run"]["evidence_path"] = json!("../private/live.json")
            }
            "oracle failed" => bad["live_agent_run"]["oracle"]["pass"] = json!(false),
            "wrong product" => bad["live_agent_run"]["clients"][0]["product"] = json!("Other CLI"),
            "release limitation missing" => {
                bad["live_agent_run"]["not_a_release_tag"] = json!(false)
            }
            "run structure type" => bad["live_agent_run"] = json!([]),
            "clients structure type" => bad["live_agent_run"]["clients"] = json!({}),
            "oracle structure type" => bad["live_agent_run"]["oracle"] = json!([]),
            "required field missing" => {
                bad["live_agent_run"]
                    .as_object_mut()
                    .unwrap()
                    .remove("work_id");
            }
            "receipt count structure type" => {
                bad["live_agent_run"]["oracle"]["receipts"] = json!("1")
            }
            "git head malformed" => bad["live_agent_run"]["git_head"] = json!("main"),
            "provenance missing" => {
                bad["live_agent_run"]
                    .as_object_mut()
                    .unwrap()
                    .remove("historical_evidence_review");
            }
            "provenance hash malformed" => {
                bad["live_agent_run"]["historical_evidence_review"]["evidence_bundle_sha256"] =
                    json!("unknown")
            }
            "retained driver falsely bound" => {
                bad["live_agent_run"]["historical_evidence_review"]["retained_driver_binding"] =
                    json!("bound")
            }
            "historical run time fabricated" => {
                bad["live_agent_run"]["historical_evidence_review"]["run_time"] =
                    json!("2026-09-17T00:00:00Z")
            }
            "historical chain falsely reverified" => {
                bad["live_agent_run"]["historical_evidence_review"]["full_chain_reverified"] =
                    json!(true)
            }
            "current run falsely claimed" => {
                bad["live_agent_run"]["current_verification"]["live_rerun"] = json!(true)
            }
            _ => unreachable!(),
        }
        assert!(validate(&bad).is_err(), "mutation passed: {name}");
    }
}
