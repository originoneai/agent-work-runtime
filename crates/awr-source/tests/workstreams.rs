use awr_core::*;
use awr_source::{Manifest, index_project};
use awr_store::Store;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

const LEDGER: &str = include_str!("../../../tests/fixtures/workstreams/ledger.yaml");

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-scoped-source-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("ledger.yaml"), LEDGER).unwrap();
        Self(root)
    }
    fn store(&self) -> Store {
        Store::open(&self.0.join("state.db")).unwrap()
    }
    fn write(&self, text: &str) {
        fs::write(self.0.join("ledger.yaml"), text).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest() -> Manifest {
    Manifest::parse("[project]\nname='Scopes'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='ledger.yaml'\nadapter='yaml-workstream-ledger-v1'\n").unwrap()
}

#[test]
fn complete_source_import_is_idempotent_and_preserves_work_identity() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    let initial = index_project(&mut store, &fixture.0, &manifest(), false).unwrap();
    assert!(initial.ok, "{initial:?}");
    let catalog = store.workstream_catalog(initial.project_id).unwrap();
    assert_eq!(catalog.workstreams.len(), 2);
    assert_eq!(catalog.project_id, initial.project_id.to_string());
    let api = store.work_item(initial.project_id, "API-1").unwrap().item;
    let client = store
        .work_item(initial.project_id, "CLIENT-1")
        .unwrap()
        .item;
    let api_binding = store
        .workstream_binding(initial.project_id, api.meta.id)
        .unwrap();
    let client_binding = store
        .workstream_binding(initial.project_id, client.meta.id)
        .unwrap();
    assert_ne!(api_binding.workstream_id, client_binding.workstream_id);
    let again = index_project(&mut store, &fixture.0, &manifest(), false).unwrap();
    assert!(again.ok);
    assert_eq!(again.project_revision, initial.project_revision);
    fixture.write(&LEDGER.replace("title: API\n", "title: API contract\n"));
    let changed = index_project(&mut store, &fixture.0, &manifest(), false).unwrap();
    assert!(changed.ok, "{changed:?}");
    assert_eq!(
        store
            .work_item(initial.project_id, "API-1")
            .unwrap()
            .item
            .meta
            .id,
        api.meta.id
    );
    assert_eq!(
        store
            .workstream_binding(initial.project_id, api.meta.id)
            .unwrap(),
        api_binding
    );
    assert_eq!(
        store
            .workstream_catalog(initial.project_id)
            .unwrap()
            .workstreams[0]
            .authority_version,
        1
    );
    assert!(Store::inspect(&fixture.0.join("state.db")).unwrap().ok);
}

#[test]
fn invalid_scopes_rollback_the_entire_candidate_and_block_stale_catalog_reads() {
    let invalid = [
        LEDGER.replace("version: 1", "version: 9"),
        LEDGER.replace("workstream: api", "workstream: missing"),
        LEDGER.replace("    workstream: api\n", ""),
        LEDGER.replace("workstream: api", "workstream: [api, client]"),
        LEDGER.replace("external_key: client", "external_key: api"),
        LEDGER.replace("00000002", "00000001"),
        LEDGER.replace("goal_keys: [delivery]", "goal_keys: [missing]"),
        LEDGER.replace("  version: 1", "  version: 1\n  grant: all"),
        LEDGER.replace("title: API\n", "title: API\n      project_id: foreign"),
        LEDGER.replace("state: active", "state: paused"), // unchanged authority version
        LEDGER.replace("workstream: api", "workstream: client"), // no silent movement
        LEDGER.replace("01K00000000000000000000002", "01K00000000000000000000003"),
    ];
    for text in invalid {
        let fixture = Fixture::new();
        let mut store = fixture.store();
        let initial = index_project(&mut store, &fixture.0, &manifest(), false).unwrap();
        assert!(initial.ok);
        let source = store.sources(initial.project_id).unwrap().remove(0);
        let baseline = store
            .source_projection_payloads(&source, EntityKind::WorkItem)
            .unwrap();
        fixture.write(&text.replace("Define the interface", "Candidate title"));
        let rejected = index_project(&mut store, &fixture.0, &manifest(), false).unwrap();
        assert!(!rejected.ok, "invalid candidate was accepted: {text}");
        assert!(matches!(
            store.workstream_catalog(initial.project_id),
            Err(Error::SourceStale(_))
        ));
        assert_eq!(
            store
                .source_projection_payloads(&source, EntityKind::WorkItem)
                .unwrap(),
            baseline
        );
        fixture.write(LEDGER);
        let recovered = index_project(&mut store, &fixture.0, &manifest(), false).unwrap();
        assert!(recovered.ok, "{recovered:?}");
        assert_eq!(
            store
                .workstream_catalog(initial.project_id)
                .unwrap()
                .workstreams
                .len(),
            2
        );
    }
}

#[test]
fn source_adapter_downgrade_and_secondary_authority_are_rejected() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    let mut legacy = manifest();
    legacy.sources[0].adapter = "yaml-ledger-v1".into();
    let rejected = index_project(&mut store, &fixture.0, &legacy, false).unwrap();
    assert!(!rejected.ok);
    let accepted = index_project(&mut store, &fixture.0, &manifest(), false).unwrap();
    assert!(accepted.ok, "{accepted:?}");
    let mut extra = manifest();
    let mut spec = extra.sources[0].clone();
    spec.path = Some("other.yaml".into());
    extra.sources.push(spec);
    fs::write(
        fixture.0.join("other.yaml"),
        LEDGER
            .replace("API-1", "API-2")
            .replace("CLIENT-1", "CLIENT-2"),
    )
    .unwrap();
    assert!(
        !index_project(&mut store, &fixture.0, &extra, false)
            .unwrap()
            .ok
    );
    let mut supporting = manifest();
    supporting.sources[0].role = "supporting".into();
    assert!(supporting.validate().is_err());
}

#[test]
fn established_legacy_history_cannot_be_reassigned_by_reindex() {
    let fixture = Fixture::new();
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(LEDGER).unwrap();
    let mut document = serde_json::to_value(value).unwrap();
    document.as_object_mut().unwrap().remove("workstreams");
    let text = serde_yaml_ng::to_string(&document).unwrap();
    fixture.write(&text);
    let mut legacy = manifest();
    legacy.sources[0].adapter = "yaml-ledger-v1".into();
    let mut store = fixture.store();
    let report = index_project(&mut store, &fixture.0, &legacy, false).unwrap();
    assert!(report.ok);
    let work = store.work_item(report.project_id, "API-1").unwrap().item;
    let baseline = store
        .workstream_binding(report.project_id, work.meta.id)
        .unwrap();
    let (started, _) = store
        .start_session(
            report.project_id,
            report.project_revision,
            SessionDraft {
                work_item_key: Some("API-1".into()),
                agent_id: "fixture-agent".into(),
                provider: "fixture".into(),
                model: "fixture".into(),
                branch_id: None,
                claim: true,
                claim_ttl_ms: Some(60_000),
            },
        )
        .unwrap();
    let history = json!(store.sessions(report.project_id, false, 20).unwrap());
    fixture.write(LEDGER);
    let rejected = index_project(&mut store, &fixture.0, &manifest(), false).unwrap();
    assert!(!rejected.ok, "{rejected:?}");
    assert_eq!(
        store
            .workstream_binding(report.project_id, work.meta.id)
            .unwrap(),
        baseline
    );
    assert_eq!(
        json!(store.sessions(report.project_id, false, 20).unwrap()),
        history
    );
    assert_eq!(started.session.work_item_id, Some(work.meta.id));
}

#[test]
fn import_preview_is_private_and_retired_authority_cannot_become_an_implicit_default() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    let project = store
        .register_project(&fixture.0, "Scopes", "Scopes")
        .unwrap();
    let before = store.project(project.id).unwrap().project_revision;
    let db_bytes = fs::read(fixture.0.join("state.db")).unwrap();
    let mut preview =
        Store::preview_snapshot(&fixture.0.join("state.db"), 32 * 1024 * 1024).unwrap();
    let proposed = index_project(&mut preview, &fixture.0, &manifest(), false).unwrap();
    assert!(proposed.ok, "{proposed:?}");
    assert_eq!(
        preview
            .workstream_catalog(project.id)
            .unwrap()
            .workstreams
            .len(),
        2
    );
    assert_eq!(
        store
            .workstream_catalog(project.id)
            .unwrap()
            .workstreams
            .len(),
        1
    );
    assert_eq!(store.project(project.id).unwrap().project_revision, before);
    assert_eq!(fs::read(fixture.0.join("state.db")).unwrap(), db_bytes);
    assert!(
        index_project(&mut store, &fixture.0, &manifest(), false)
            .unwrap()
            .ok
    );
    let original = store.work_item(project.id, "API-1").unwrap().item.meta.id;
    let mut removed = manifest();
    removed.sources[0].adapter = "yaml-ledger-v1".into();
    removed.sources[0].path = Some("other.yaml".into());
    fs::write(fixture.0.join("other.yaml"), "goals: []\n").unwrap();
    assert!(
        !index_project(&mut store, &fixture.0, &removed, false)
            .unwrap()
            .ok
    );
    assert!(matches!(
        store.workstream_catalog(project.id),
        Err(Error::SourceStale(_))
    ));
    assert!(matches!(
        store.workstream_binding(project.id, original),
        Err(Error::SourceStale(_))
    ));
    assert!(
        index_project(&mut store, &fixture.0, &manifest(), false)
            .unwrap()
            .ok
    );
    assert_eq!(
        store.work_item(project.id, "API-1").unwrap().item.meta.id,
        original
    );
}

#[test]
fn separately_owned_goal_changes_invalidate_unchanged_scope_declarations() {
    let fixture = Fixture::new();
    let mut document: Value =
        serde_json::to_value(serde_yaml_ng::from_str::<serde_yaml_ng::Value>(LEDGER).unwrap())
            .unwrap();
    document.as_object_mut().unwrap().remove("goals");
    fixture.write(&serde_yaml_ng::to_string(&document).unwrap());
    let mut mapping = manifest();
    let mut goals = mapping.sources[0].clone();
    goals.adapter = "yaml-ledger-v1".into();
    goals.path = Some("goals.yaml".into());
    mapping.sources.push(goals);
    fs::write(
        fixture.0.join("goals.yaml"),
        "goals: [{id: delivery, title: Shared goal, status: active}]\n",
    )
    .unwrap();
    let mut store = fixture.store();
    let initial = index_project(&mut store, &fixture.0, &mapping, false).unwrap();
    assert!(initial.ok, "{initial:?}");
    fs::write(fixture.0.join("goals.yaml"), "goals: []\n").unwrap();
    let changed = index_project(&mut store, &fixture.0, &mapping, false).unwrap();
    assert!(!changed.ok);
    assert!(matches!(
        store.workstream_catalog(initial.project_id),
        Err(Error::SourceStale(_))
    ));
    fs::write(
        fixture.0.join("goals.yaml"),
        "goals: [{id: delivery, title: Shared goal, status: active}]\n",
    )
    .unwrap();
    assert!(
        index_project(&mut store, &fixture.0, &mapping, false)
            .unwrap()
            .ok
    );
}
