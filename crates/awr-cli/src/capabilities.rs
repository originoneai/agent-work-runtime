use awr_core::{Error, Result};
use clap::Args;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;

const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Args)]
pub struct CapabilitiesArgs {
    /// Version of the capability negotiation contract, not the program version.
    #[arg(long, default_value_t = PROTOCOL_VERSION)]
    protocol_version: u32,
    /// Require an available capability. Repeat for each prerequisite.
    #[arg(long = "require")]
    required: Vec<String>,
}

#[derive(Serialize)]
struct Capability {
    id: &'static str,
    available: bool,
    commands: &'static [&'static str],
    limitations: &'static [&'static str],
}

fn catalog() -> Vec<Capability> {
    [
        (
            "source.read",
            true,
            &["source list", "source show"][..],
            &["registered_sources_only", "bounded_body_reads"][..],
        ),
        (
            "source.refresh",
            true,
            &["source scan", "source reindex", "source history"],
            &["may_write_runtime", "partial_failure_requires_retry"],
        ),
        (
            "source.changes",
            true,
            &["source changes"],
            &[
                "read_only_event_window",
                "consumer_owns_success_cursor",
                "current_failures_are_not_retirements",
            ],
        ),
        (
            "intake.reviewed_draft",
            true,
            &["init", "intake inspect"],
            &[
                "explicit_accept",
                "draft_fingerprint_checked",
                "effect_binding_requires_expected_preview",
            ],
        ),
        (
            "intake.exact_preview",
            true,
            &["init"],
            &["requires_expected_preview", "individual_files_only"],
        ),
        (
            "source.configure",
            true,
            &["source configure", "source configure-status"],
            &[
                "existing_project_identity_preserved",
                "requires_expected_preview",
                "projection_can_partially_fail",
            ],
        ),
        (
            "work.read",
            true,
            &["work show", "ready", "status"],
            &["summaries_are_not_a_complete_catalog"],
        ),
        (
            "object.read",
            true,
            &["object show", "decision show"],
            &["explicit_object_reference", "bounded_body_reads"],
        ),
        (
            "project.catalog",
            true,
            &["object list"],
            &[
                "revision_bound_pages",
                "retired_is_not_archived",
                "summaries_only_bodies_are_opt_in",
            ],
        ),
        (
            "history.cursor",
            true,
            &["event history", "source history", "work history"],
            &["revision_bound_cursor"],
        ),
        (
            "context.compile",
            true,
            &["context compile"],
            &["may_write_runtime", "incomplete_or_over_budget_is_failure"],
        ),
        (
            "session.checkpoint",
            true,
            &["session checkpoint"],
            &["explicit_session", "caller_supplied_digest"],
        ),
        (
            "session.resume",
            true,
            &["session resume", "recovery inspect"],
            &[
                "inspect_is_separate_from_resume",
                "not_native_client_resume",
            ],
        ),
        (
            "client.lifecycle.generic",
            true,
            &["client bind", "client progress", "client hook"],
            &[
                "documented_lifecycle_events_only",
                "no_required_hook_installation",
            ],
        ),
        (
            "execution.external.register",
            true,
            &["execution register", "execution show"],
            &["external_reference_unverified", "not_managed_execution"],
        ),
        (
            "execution.managed",
            true,
            &["execution run", "execution inspect"],
            &["local_supervisor", "project_scoped_operation_key"],
        ),
        (
            "execution.external.report",
            true,
            &[
                "execution report",
                "execution report-status",
                "execution inspect",
            ],
            &[
                "host_supplied_provenance",
                "never_promotes_managed_or_verified",
                "request_key_idempotent",
            ],
        ),
        (
            "artifact.managed",
            true,
            &["artifact add", "artifact show", "artifact cat"],
            &["add_copies_content", "bounded_authorized_reads"],
        ),
        (
            "evidence.read_write",
            true,
            &["evidence add", "evidence show"],
            &[
                "registration_is_not_verification",
                "command_field_is_not_executed",
            ],
        ),
        (
            "mutation.yaml.record",
            true,
            &["proposal create", "proposal apply"],
            &[
                "supported_fields_only",
                "field_spans_preserve_unrelated_bytes",
                "state_requires_domain_action",
                "single_file",
            ],
        ),
        (
            "mutation.yaml.lossless_fields",
            true,
            &["proposal apply"],
            &[
                "supported_yaml_shapes_only",
                "scalar_style_when_representable",
                "domain_actions_still_required",
            ],
        ),
        (
            "mutation.work.create",
            true,
            &["work create", "work create-status", "work create-recover"],
            &[
                "registered_yaml_ledger_only",
                "exact_preview_and_revision",
                "stable_request_key",
                "draft_state_not_executable",
            ],
        ),
        ("mutation.markdown", false, &[], &["not_implemented"]),
        ("mutation.human_save", false, &[], &["not_implemented"]),
        ("mutation.multi_file", false, &[], &["not_implemented"]),
        (
            "completion.engineering",
            true,
            &["work complete"],
            &[
                "session_and_claim_required",
                "source_sha_and_acceptance_evidence_required",
            ],
        ),
        (
            "completion.user_confirmation",
            false,
            &[],
            &["not_implemented"],
        ),
    ]
    .into_iter()
    .map(|(id, available, commands, limitations)| Capability {
        id,
        available,
        commands,
        limitations,
    })
    .collect()
}

pub fn run(args: &CapabilitiesArgs, json_output: bool) -> Result<()> {
    if args.protocol_version != PROTOCOL_VERSION {
        return Err(Error::ProtocolUnsupported {
            requested: args.protocol_version,
            supported: vec![PROTOCOL_VERSION],
        });
    }
    if args.required.len() > 100
        || args.required.iter().any(|id| {
            id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
    {
        return Err(Error::InvalidInput(
            "require accepts at most 100 capability IDs of 1..128 ASCII identifier characters"
                .into(),
        ));
    }
    let capabilities = catalog();
    let mut unknown = Vec::new();
    let mut unsupported = Vec::new();
    for id in args.required.iter().collect::<BTreeSet<_>>() {
        match capabilities.iter().find(|c| c.id == id) {
            None => unknown.push(id.clone()),
            Some(c) if !c.available => unsupported.push(id.clone()),
            Some(_) => (),
        }
    }
    if !unknown.is_empty() || !unsupported.is_empty() {
        return Err(Error::CapabilityUnavailable {
            unknown,
            unsupported,
        });
    }
    // Deliberately no project path, environment lookup, database open or source scan.
    let value = json!({
        "ok": true,
        "protocol": {"name": "awr.host", "version": PROTOCOL_VERSION},
        "program": {"name": "awr", "version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "database": {"schema_version": awr_store::SCHEMA_VERSION,
            "read_only_schema_versions": [awr_store::SCHEMA_VERSION],
            "migrate_from_schema_versions": [1, 2, 3],
            "newer_schema_policy": "reject_without_migration",
            "schema_validation_required": true},
        "source_adapters": awr_source::SOURCE_ADAPTERS.iter().map(|id| json!({
            "id": id, "read": true,
            "write_mode": if *id == "yaml-ledger-v1" { "lossless_supported_fields" } else { "read_only" },
            "lossless_field_write": *id == "yaml-ledger-v1"
        })).collect::<Vec<_>>(),
        "capabilities": capabilities,
        "source_write_performed": false,
        "runtime_write_performed": false,
        "scope": "build_capabilities_not_project_authorization",
        "transport": {"invocation": "argv", "json_flag": "--json",
            "success_stream": "stdout", "error_stream": "stderr",
            "exit_codes": {"success": 0, "runtime_error": 1, "usage_error": 2},
            "partial_result_policy": "stdout_may_accompany_nonzero_exit",
            "timeout_policy": "outcome_unknown_inspect_before_retry"}
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "awr {} — host protocol {}",
            env!("CARGO_PKG_VERSION"),
            PROTOCOL_VERSION
        );
        for capability in capabilities {
            println!(
                "{}: {}",
                capability.id,
                if capability.available {
                    "available"
                } else {
                    "unavailable"
                }
            );
        }
    }
    Ok(())
}
