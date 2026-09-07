use awr_core::{EntityKind, Error, EventDraft, Freshness, Id, Source};
use awr_source::{Manifest, index_project};
use awr_store::Store;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-indexer-{}", Id::new()));
        fs::create_dir_all(root.join("decisions")).unwrap();
        fs::write(
            root.join("ledger.yaml"),
            include_str!("../../../tests/fixtures/yaml-ledger/ledger.yaml"),
        )
        .unwrap();
        fs::write(
            root.join("goals.md"),
            "# Delivery\n\nKeep source facts complete.\n",
        )
        .unwrap();
        fs::write(
            root.join("rules.md"),
            "# Source rule\n\nKeep files authoritative.\n",
        )
        .unwrap();
        fs::write(
            root.join("decisions/001.md"),
            include_str!("../../../tests/fixtures/decisions/001-persistence.md"),
        )
        .unwrap();
        fs::write(
            root.join("decisions/002.md"),
            include_str!("../../../tests/fixtures/decisions/002-obsolete.md"),
        )
        .unwrap();
        Self(root)
    }
    fn store(&self) -> Store {
        Store::open(&self.0.join("state.db")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn manifest() -> Manifest {
    Manifest::parse("[project]\nname='Incremental fixture'\nexternal_key='incremental'\n\
[[sources]]\ndomain='ledger'\nrole='primary'\npath='ledger.yaml'\nadapter='yaml-ledger-v1'\n\
[[sources]]\ndomain='goal'\nrole='primary'\npath='goals.md'\nadapter='markdown-heading-v1'\n\
[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n\
[[sources]]\ndomain='decisions'\nrole='supporting'\npath='decisions'\nadapter='markdown-directory-v1'\n").unwrap()
}
fn source(store: &Store, project: Id, domain: &str) -> Source {
    store
        .sources(project)
        .unwrap()
        .into_iter()
        .find(|source| source.domain == domain)
        .unwrap()
}
fn work(store: &Store, source: &Source) -> BTreeMap<String, Value> {
    store
        .source_projection_payloads(source, EntityKind::WorkItem)
        .unwrap()
        .into_iter()
        .map(|item| (item["external_key"].as_str().unwrap().into(), item))
        .collect()
}

#[test]
fn unchanged_sources_entities_and_configuration() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    let mut manifest = manifest();
    let first = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    assert!(first.ok);
    assert_eq!(first.indexed, 5);
    let ledger = source(&store, first.project_id, "ledger");
    let before = work(&store, &ledger);
    let repeat = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    assert!(repeat.ok);
    assert_eq!((repeat.indexed, repeat.unchanged), (0, 5));
    assert_eq!(repeat.project_revision, first.project_revision);
    assert_eq!(work(&store, &ledger), before);
    assert_eq!(
        repeat
            .sources
            .iter()
            .map(|s| &s.warnings)
            .collect::<Vec<_>>(),
        first
            .sources
            .iter()
            .map(|s| &s.warnings)
            .collect::<Vec<_>>()
    );
    let path = fixture.0.join("ledger.yaml");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("waiting_for_customer", "ready"),
    )
    .unwrap();
    let changed = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    assert!(changed.ok);
    assert_eq!((changed.indexed, changed.unchanged), (1, 4));
    let after = work(&store, &ledger);
    assert_eq!(before["W1"]["revision"], after["W1"]["revision"]);
    assert_eq!(before["W1"]["id"], after["W1"]["id"]);
    assert_eq!(after["W2"]["revision"], 2);
    assert_eq!(after["W2"]["status"], "ready");
    assert_eq!(after["W1"]["source_ref"]["source_revision"], 2);
    let rules = source(&store, first.project_id, "rules");
    let original_fingerprint = rules.fingerprint.clone();
    for (key, value) in [("severity", "hard"), ("scope", "project"), ("value", "*")] {
        manifest.sources[2].options.insert(key.into(), value.into());
    }
    let configured = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    assert!(configured.ok);
    assert_eq!((configured.indexed, configured.unchanged), (1, 4));
    let rules = store.source(first.project_id, rules.id).unwrap();
    assert_eq!(rules.fingerprint, original_fingerprint);
    assert_eq!(rules.revision, 2);
    let rule = &store
        .source_projection_payloads(&rules, EntityKind::Rule)
        .unwrap()[0];
    assert_eq!(rule["severity"], "hard");
    assert_eq!(rule["scope"]["type"], "project");
    assert_eq!(rule["unresolved"], serde_json::json!([]));
}

#[test]
fn removals_invalidate_facts_but_keep_history_and_unavailable_sources() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    let manifest = manifest();
    let first = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    assert!(first.ok);
    let ledger = source(&store, first.project_id, "ledger");
    let before = work(&store, &ledger);
    let work_id: Id = before["W2"]["id"].as_str().unwrap().parse().unwrap();
    let mut marker = EventDraft::new(
        "user.marker",
        "Runtime history is independent of source projection",
    );
    marker.work_item_id = Some(work_id);
    store
        .append_event(first.project_id, first.project_revision, marker)
        .unwrap();
    let removed = store
        .sources(first.project_id)
        .unwrap()
        .into_iter()
        .find(|s| s.locator.ends_with("/002.md"))
        .unwrap();
    fs::remove_file(fixture.0.join("decisions/002.md")).unwrap();
    let path = fixture.0.join("ledger.yaml");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, original.split("  - id: W2").next().unwrap()).unwrap();
    let changed = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    assert!(changed.ok);
    assert_eq!(changed.retired, 1);
    assert!(matches!(
        store.source(first.project_id, removed.id),
        Err(Error::NotFound(_))
    ));
    assert!(
        store
            .source_projection_payloads(&removed, EntityKind::Decision)
            .unwrap()
            .is_empty()
    );
    assert!(!store.projection_ids(&removed).unwrap().is_empty());
    assert_eq!(work(&store, &ledger).len(), 1);
    assert_eq!(
        store.projection_ids(&ledger).unwrap()[&(EntityKind::WorkItem, "W2".into())],
        work_id
    );
    assert_eq!(
        store
            .source_projection_payloads(&ledger, EntityKind::Evidence)
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .events_since(first.project_id, 0, 100)
            .unwrap()
            .iter()
            .any(|event| event.event_type == "user.marker" && event.work_item_id == Some(work_id))
    );
    let remaining = source(&store, first.project_id, "decisions");
    fs::rename(fixture.0.join("decisions"), fixture.0.join("offline")).unwrap();
    let unavailable = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    assert!(!unavailable.ok);
    assert_eq!(unavailable.retired, 0);
    assert_eq!(
        store
            .source(first.project_id, remaining.id)
            .unwrap()
            .freshness,
        Freshness::Unavailable
    );
    assert_eq!(
        store
            .source_projection_payloads(&remaining, EntityKind::Decision)
            .unwrap()
            .len(),
        1
    );
    fs::rename(fixture.0.join("offline"), fixture.0.join("decisions")).unwrap();
    let recovered = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    assert!(recovered.ok);
    assert_eq!(
        store
            .source(first.project_id, remaining.id)
            .unwrap()
            .freshness,
        Freshness::Fresh
    );
    assert!(store.doctor().unwrap().ok);
}

#[test]
fn rebuild_restores_source_facts_without_inventing_runtime_history() {
    let fixture = Fixture::new();
    let mut store = fixture.store();
    let manifest = manifest();
    let first = index_project(&mut store, &fixture.0, &manifest, false).unwrap();
    let ledger = source(&store, first.project_id, "ledger");
    let before = work(&store, &ledger);
    store
        .append_event(
            first.project_id,
            first.project_revision,
            EventDraft::new("user.marker", "Only in the original runtime database"),
        )
        .unwrap();
    drop(store);
    fs::remove_file(fixture.0.join("state.db")).unwrap();
    let mut rebuilt = fixture.store();
    let report = index_project(&mut rebuilt, &fixture.0, &manifest, false).unwrap();
    assert!(report.ok);
    assert_eq!(report.indexed, 5);
    let after = work(&rebuilt, &source(&rebuilt, report.project_id, "ledger"));
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    for key in before.keys() {
        for field in ["title", "raw_status", "status", "acceptance", "next_action"] {
            assert_eq!(before[key][field], after[key][field]);
        }
    }
    assert!(
        !rebuilt
            .events_since(report.project_id, 0, 100)
            .unwrap()
            .iter()
            .any(|event| event.event_type == "user.marker")
    );
    assert!(rebuilt.doctor().unwrap().ok);
    let mut overlapping = manifest.clone();
    let mut alias = overlapping.sources[1].clone();
    alias.path = Some(fixture.0.join("goals.md"));
    overlapping.sources.push(alias);
    assert!(matches!(
        index_project(&mut rebuilt, &fixture.0, &overlapping, false),
        Err(Error::SourceConflict(_))
    ));
}
