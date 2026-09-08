use awr_core::*;
use awr_store::{ReconcileAction, SourceRegistration, Store};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    store: Store,
    project: Project,
    source: Source,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-event-boundary-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        let mut store = Store::open(&root.join("state.db")).unwrap();
        let project = store
            .register_project(&root, "event-boundary", "Event boundary fixture")
            .unwrap();
        let source = store
            .register_source(
                project.id,
                &SourceRegistration {
                    domain: "ledger",
                    role: "primary",
                    locator: "file:///fixture/work.yaml",
                    format: "yaml",
                    adapter: "yaml-ledger-v1",
                },
            )
            .unwrap();
        Self {
            root,
            store,
            project,
            source,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const PAYLOAD_CAP: usize = 1024 * 1024;
const SUMMARY_CAP: usize = 8192;

fn append(f: &mut Fixture, summary: &str, payload: Value) -> Result<Event> {
    let mut draft = EventDraft::new("work.observed", summary);
    draft.payload = payload;
    let revision = f.store.project(f.project.id)?.project_revision;
    f.store.append_event(f.project.id, revision, draft)
}
fn reject_without_commit(f: &mut Fixture, summary: &str, payload: Value) -> Error {
    let before = f.store.project(f.project.id).unwrap().project_revision;
    let result = append(f, summary, payload);
    assert!(result.is_err(), "invalid event must be rejected");
    let error = result.unwrap_err();
    assert_eq!(
        f.store.project(f.project.id).unwrap().project_revision,
        before
    );
    assert!(
        f.store
            .events_since(f.project.id, before, 100)
            .unwrap()
            .is_empty()
    );
    error
}

#[test]
fn generic_payload_limit_counts_encoded_bytes_and_rejects_growth_atomically() {
    let mut f = Fixture::new();
    let overhead = serde_json::to_vec(&json!({"body":""})).unwrap().len();
    let value = json!({"body":"x".repeat(PAYLOAD_CAP - overhead)});
    assert_eq!(serde_json::to_vec(&value).unwrap().len(), PAYLOAD_CAP);
    append(&mut f, "Retain a bounded observation", value).unwrap();
    assert!(matches!(
        reject_without_commit(
            &mut f,
            "Too much data",
            json!({"body":"x".repeat(PAYLOAD_CAP - overhead + 1)})
        ),
        Error::InvalidInput(_)
    ));
    // Escaping expands serialized JSON even though the raw string is below the limit.
    assert!(matches!(
        reject_without_commit(
            &mut f,
            "Escaped data",
            json!({"body":"\"".repeat(PAYLOAD_CAP / 2)})
        ),
        Error::InvalidInput(_)
    ));
    println!("AWR_PAYLOAD_CASE event_payload_limit");
}

#[test]
fn event_summary_and_type_have_explicit_byte_limits() {
    let mut f = Fixture::new();
    append(&mut f, &"s".repeat(SUMMARY_CAP), json!({})).unwrap();
    for value in [
        "s".repeat(SUMMARY_CAP + 1),
        "界".repeat(SUMMARY_CAP / 3 + 1),
        "  ".into(),
    ] {
        assert!(matches!(
            reject_without_commit(&mut f, &value, json!({})),
            Error::InvalidInput(_)
        ));
    }
    for kind in ["invalid type".to_owned(), "a".repeat(129)] {
        let revision = f.store.project(f.project.id).unwrap().project_revision;
        assert!(matches!(
            f.store.append_event(
                f.project.id,
                revision,
                EventDraft::new(kind, "Reviewed observation")
            ),
            Err(Error::InvalidInput(_))
        ));
        assert_eq!(
            f.store.project(f.project.id).unwrap().project_revision,
            revision
        );
    }
    println!("AWR_PAYLOAD_CASE event_summary_limit");
}

#[test]
fn generic_payload_has_an_allowlist_and_never_echoes_untrusted_fields() {
    let mut f = Fixture::new();
    for value in [
        json!({"UNTRUSTED_NAME_SENTINEL":"UNTRUSTED_VALUE_SENTINEL"}),
        json!({"private_prompt":"UNTRUSTED_VALUE_SENTINEL"}),
        json!(["UNTRUSTED_VALUE_SENTINEL"]),
    ] {
        let error = reject_without_commit(&mut f, "Observation", value);
        assert!(matches!(error, Error::InvalidInput(_)));
        let text = error.to_string();
        assert!(
            !text.contains("UNTRUSTED_NAME_SENTINEL") && !text.contains("UNTRUSTED_VALUE_SENTINEL")
        );
    }
    println!("AWR_PAYLOAD_CASE event_field_allowlist");
}

#[test]
fn field_types_and_project_bound_references_are_checked() {
    let mut f = Fixture::new();
    for value in [
        json!({"status":17}),
        json!({"body":{"nested":"data"}}),
        json!({"exit_code":"0"}),
        json!({"duration_ms":-1}),
        json!({"tags":"tag"}),
        json!({"metrics":{"duration_ms":{"nested":1}}}),
        json!({"source_id":17}),
    ] {
        assert!(matches!(
            reject_without_commit(&mut f, "Observation", value),
            Error::InvalidInput(_)
        ));
    }
    assert!(matches!(
        reject_without_commit(&mut f, "Observation", json!({"source_id":Id::new()})),
        Error::NotFound(_)
    ));
    let other_root = f.root.join("other-project");
    fs::create_dir(&other_root).unwrap();
    let other = f
        .store
        .register_project(&other_root, "other", "Independent project")
        .unwrap();
    let foreign_source = f
        .store
        .register_source(
            other.id,
            &SourceRegistration {
                domain: "ledger",
                role: "primary",
                locator: "file:///fixture/other.yaml",
                format: "yaml",
                adapter: "yaml-ledger-v1",
            },
        )
        .unwrap();
    assert!(matches!(
        reject_without_commit(
            &mut f,
            "Observation",
            json!({"source_id":foreign_source.id})
        ),
        Error::NotFound(_)
    ));
    let source = f.source.id;
    let value = json!({"source_id":source,"body":"Literal details","stdout":"Bounded output", "stderr":"", "status":"failed","tool":"compiler","operation":"check","error_code":"BuildFailed","command":"a command recorded as data","report":"reports/build.json","exit_code":1,"duration_ms":17,"count":2,"attempt":1,"tags":["build"],"changed_entities":["work:W"],"metrics":{"elapsed_ms":17.5}});
    let event = append(&mut f, "Build observation", value.clone()).unwrap();
    assert_eq!(event.payload, value);
    println!("AWR_PAYLOAD_CASE event_field_types");
}

#[test]
fn domain_contracts_reject_unknown_operations_and_missing_receipt_fields() {
    for (kind, payload) in [
        ("source.unrecognized", json!({})),
        ("proposal.unrecognized", json!({})),
        ("source.projected", json!({"source_id":Id::new()})),
        ("checkpoint.created", json!({"checkpoint_id":null})),
        ("branch.closed", json!({"closure":[]})),
        ("claim.expired", json!({"claim_id":17})),
    ] {
        assert!(matches!(
            checked_event_payload(kind, "normal", "Incomplete receipt", &payload),
            Err(Error::InvalidInput(_))
        ));
    }
}

#[test]
fn final_event_rejection_rolls_back_a_domain_operation_and_its_revision() {
    let mut f = Fixture::new();
    let before = f.store.project(f.project.id).unwrap().project_revision;
    let result = f.store.record_evidence(
        f.project.id,
        before,
        EvidenceDraft {
            external_key: "e".repeat(SUMMARY_CAP),
            work_item_key: None,
            evidence_type: "report".into(),
            level: EvidenceLevel::Implemented,
            summary: "Reviewed report".into(),
            locator: "report.txt".into(),
            sha256: None,
            source_sha: None,
            command: None,
            scope: vec!["*".into()],
            branch_id: None,
            verified_at: None,
        },
    );
    assert!(
        matches!(result, Err(Error::InvalidInput(_))),
        "oversized generated event must reject the whole transaction"
    );
    assert_eq!(
        f.store.project(f.project.id).unwrap().project_revision,
        before
    );
    assert!(f.store.evidence_records(f.project.id).unwrap().is_empty());
    assert!(
        f.store
            .events_since(f.project.id, before, 100)
            .unwrap()
            .is_empty()
    );
    println!("AWR_PAYLOAD_CASE event_atomic_rejection");
}

#[test]
fn domain_payloads_are_bounded_after_their_actual_contents_are_constructed() {
    let mut f = Fixture::new();
    let rev = f.store.project(f.project.id).unwrap().project_revision;
    let (started, event) = f
        .store
        .start_session(
            f.project.id,
            rev,
            SessionDraft {
                work_item_key: None,
                agent_id: "fixture-writer".into(),
                provider: "local".into(),
                model: "fixture".into(),
                branch_id: None,
                claim: false,
                claim_ttl_ms: None,
            },
        )
        .unwrap();
    let attempt = f
        .store
        .begin_checkpoint_save(
            f.project.id,
            event.project_revision,
            started.session.id,
            CheckpointDraft {
                context_hash: "a".repeat(64),
                digest: "Preserved draft".into(),
                next_action: "Review draft".into(),
                open_loops: vec![],
                changed_entities: vec![],
            },
        )
        .unwrap();
    let action = ReconcileAction::AbandonCheckpoint {
        attempt_id: attempt.id,
    };
    let result = f.store.reconcile(
        f.project.id,
        attempt.project_revision,
        action.clone(),
        &"r".repeat(16 * 1024 * 1024),
    );
    assert!(
        matches!(result, Err(Error::InvalidInput(_))),
        "constructed domain payload must remain bounded"
    );
    assert_eq!(
        f.store.project(f.project.id).unwrap().project_revision,
        attempt.project_revision
    );
    assert!(
        f.store
            .events_since(f.project.id, attempt.project_revision, 100)
            .unwrap()
            .is_empty()
    );
    f.store
        .reconcile(
            f.project.id,
            attempt.project_revision,
            action,
            "Retained the original draft; a new save will replace this attempt",
        )
        .unwrap();
}
