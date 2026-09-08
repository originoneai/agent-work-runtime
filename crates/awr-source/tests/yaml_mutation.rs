use awr_core::*;
use awr_source::{Manifest, index_project, prepare_yaml_mutation};
use awr_store::Store;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    store: Store,
    project: Id,
}
impl Fixture {
    fn new(text: &str) -> Self {
        let root = std::env::temp_dir().join(format!("awr-yaml-mutation-{}", Id::new()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("ledger.yaml"), text).unwrap();
        let manifest:Manifest=toml::from_str("[project]\nname='Mutation source'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='ledger.yaml'\nadapter='yaml-ledger-v1'\n").unwrap();
        fs::create_dir(root.join(".awr")).unwrap();
        fs::write(
            root.join(".awr/project.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let mut store = Store::open(&root.join("state.db")).unwrap();
        let report = index_project(&mut store, &root, &manifest, false).unwrap();
        assert!(report.ok, "{:?}", report.issues);
        Self {
            root,
            store,
            project: report.project_id,
        }
    }
    fn plan(
        &self,
        kind: EntityKind,
        key: &str,
        changes: Value,
    ) -> Result<awr_source::PreparedYamlMutation> {
        let target = self.store.mutation_target(self.project, kind, key)?;
        let patch = MutationPatch {
            version: 1,
            target: MutationTarget {
                kind,
                meta: serde_json::from_value(target.item)?,
            },
            source_config: target.source.config.clone(),
            intent: "Update exactly this source record".into(),
            changes,
        };
        let proposal = MutationProposal {
            id: Id::new(),
            project_id: self.project,
            work_item_id: None,
            source_id: target.source.id,
            base_fingerprint: target.source.fingerprint.clone(),
            expected_revision: target.project_revision,
            mutation_type: "update_fields".into(),
            patch: serde_json::to_value(patch)?,
            status: ProposalStatus::Approved,
            created_by_session: None,
            revision: 1,
        };
        prepare_yaml_mutation(
            &self.root,
            &target.source,
            &proposal,
            self.store.projection_ids(&target.source)?,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn parsed(text: &str) -> Value {
    serde_json::to_value(serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text).unwrap()).unwrap()
}

#[test]
fn exact_record_spans_preserve_other_bytes_across_block_flow_unicode_and_crlf() {
    for text in [
        "# 前面的注释\nwork_items:\n- id: BEFORE\n  title: 中文和 emoji 🐾\n  status: ready\n- id: W\n  title: Report\n  status: in_progress\n  next_action: Before\n# Keep this trailing comment\n- id: AFTER\n  status: ready\n",
        "work_items: [{id: W, status: ready, next_action: 'Before, again'}, {id: AFTER, status: ready}] # 外面\n",
        "work_items:\r\n  W:\r\n    title: Report\r\n    status: ready\r\n    next_action: >\r\n      First line\r\n      second line\r\n  AFTER:\r\n    status: ready\r\n",
    ] {
        let f = Fixture::new(text);
        let plan=f.plan(EntityKind::WorkItem,"W",json!({"next_action":"完成后继续 🐾\nsecond line","summary":"Keep\u{85}special\u{2028}breaks"})).unwrap();
        let before = parsed(text);
        let after = parsed(plan.after.text().unwrap());
        let pointer = if before["work_items"].is_array() {
            if before["work_items"][0]["id"] == "W" {
                "/work_items/0"
            } else {
                "/work_items/1"
            }
        } else {
            "/work_items/W"
        };
        let mut expected = before.clone();
        let object = expected.pointer_mut(pointer).unwrap();
        object["next_action"] = json!("完成后继续 🐾\nsecond line");
        object["summary"] = json!("Keep\u{85}special\u{2028}breaks");
        assert_eq!(after, expected);
        assert_eq!(
            fs::read_to_string(f.root.join("ledger.yaml")).unwrap(),
            text
        );
        if text.contains("# Keep") {
            assert!(
                plan.after
                    .text()
                    .unwrap()
                    .contains("\n# Keep this trailing comment\n- id: AFTER\n  status: ready\n")
            );
        }
        if text.contains("\r\n") {
            assert!(
                plan.after
                    .text()
                    .unwrap()
                    .ends_with("\r\n  AFTER:\r\n    status: ready\r\n")
            );
        }
        assert_ne!(plan.plan.before_fingerprint, plan.plan.after_fingerprint);
        plan.plan.validate().unwrap();
    }
}

#[test]
fn goals_milestones_and_structured_evidence_keep_identity_and_pointer() {
    let f = Fixture::new(
        "goals:\n  G/~1:\n    title: Goal\n    status: active\nmilestones:\n- id: M\n  name: Delivery\n  status: planned\nwork_items:\n- id: W\n  status: ready\n  evidence:\n  - locator: reports/a.json\n    summary: Before\n",
    );
    for (kind, key, changes, pointer) in [
        (
            EntityKind::Goal,
            "G/~1",
            json!({"summary":"A clearer goal"}),
            "/goals/G~1~01/summary",
        ),
        (
            EntityKind::Plan,
            "M",
            json!({"status":"active"}),
            "/milestones/0/status",
        ),
        (
            EntityKind::Evidence,
            "W/evidence/reports/a.json",
            json!({"summary":"A retained source reference"}),
            "/work_items/0/evidence/0/summary",
        ),
    ] {
        let plan = f.plan(kind, key, changes.clone()).unwrap();
        assert_eq!(
            parsed(plan.after.text().unwrap()).pointer(pointer).unwrap(),
            changes.as_object().unwrap().values().next().unwrap()
        );
    }
}

#[test]
fn unsafe_shapes_guarded_fields_and_ignored_fields_remain_manual() {
    let f = Fixture::new("work_items:\n- id: W\n  status: ready\n  evidence: [reports/a.json]\n");
    for change in [
        json!({"status":"completed"}),
        json!({"owner":"different"}),
        json!({"verification":{"evidence_level":"released"}}),
        json!({"unknown":"ignored"}),
    ] {
        assert!(matches!(
            f.plan(EntityKind::WorkItem, "W", change),
            Err(Error::MutationUnsupported(_))
        ));
    }
    assert!(matches!(
        f.plan(
            EntityKind::Evidence,
            "W/evidence/reports/a.json",
            json!({"summary":"New"})
        ),
        Err(Error::MutationUnsupported(_))
    ));
    let f = Fixture::new(
        "work_items:\n- &target\n  id: W\n  status: ready\n  next_action: Before\nextra: *target\n",
    );
    assert!(matches!(
        f.plan(EntityKind::WorkItem, "W", json!({"next_action":"New"})),
        Err(Error::MutationUnsupported(_))
    ));
}

#[test]
fn drift_noop_and_invalid_field_types_never_prepare_an_apply_plan() {
    let f = Fixture::new("work_items:\n- id: W\n  status: ready\n  next_action: Before\n");
    assert!(matches!(
        f.plan(EntityKind::WorkItem, "W", json!({"next_action":"Before"})),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        f.plan(EntityKind::WorkItem, "W", json!({"next_action":123})),
        Err(Error::InvalidInput(_))
    ));
    fs::write(
        f.root.join("ledger.yaml"),
        "work_items: [{id: W, status: ready, next_action: Changed}]\n",
    )
    .unwrap();
    assert!(matches!(
        f.plan(EntityKind::WorkItem, "W", json!({"next_action":"New"})),
        Err(Error::SourceConflict(_))
    ));
}
