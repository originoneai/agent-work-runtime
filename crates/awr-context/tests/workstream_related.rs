use awr_context::related_work_in_workstream;
use awr_core::*;
use serde_json::json;
#[allow(dead_code)]
#[path = "../../awr-store/tests/support/workstreams.rs"]
mod fixture;
use fixture::Fixture;

#[test]
fn scoped_related_facts_keep_opaque_blockers_and_filter_private_decisions_and_evidence() {
    let mut f = Fixture::new();
    let boundary = f.dependency("W0", "W1", true);
    f.dependency("W0", "W2", true);
    for i in [1, 2] {
        f.batch.work_items[i].status = WorkStatus::Completed;
        f.batch.work_items[i].raw_status = "completed".into();
    }
    for (key, affected) in [
        ("local", vec!["W0"]),
        ("shared", vec!["*"]),
        ("private", vec!["W1"]),
        ("mixed", vec!["W0", "W1"]),
    ] {
        f.batch.decisions.push(serde_json::from_value(json!({
            "id":Id::new(),"external_key":key,"revision":1,"source_ref":f.batch.work_items[0].meta.source_ref,
            "title":key,"raw_status":"accepted","status":"accepted","decision":format!("{key} decision text"),
            "affected_keys":affected,"paths":["src/**"],"summary":"Synthetic decision","alternatives":[],"rationale":"Unused rationale",
        })).unwrap());
    }
    f.reproject();
    let own = f.evidence(0);
    f.evidence(1);
    let context =
        related_work_in_workstream(&f.read(0), "W0", None, None, Some(&["src/main.rs".into()]))
            .unwrap();
    assert!(context.unresolved_dependencies.is_empty());
    assert_eq!(context.resolved_dependencies.len(), 1);
    assert_eq!(context.resolved_dependencies[0].meta.external_key, "W2");
    assert_eq!(context.unavailable_dependencies[0].edge_id, boundary);
    assert_eq!(
        context
            .accepted_decisions
            .iter()
            .map(|d| d.meta.external_key.as_str())
            .collect::<Vec<_>>(),
        vec!["local", "shared"]
    );
    assert_eq!(context.evidence.len(), 1);
    assert_eq!(context.evidence[0].id, own.id);
    assert!(
        context
            .evidence_gaps
            .iter()
            .any(|g| g.code == "evidence_level_unverified")
    );
    let output = serde_json::to_string(&context).unwrap();
    for private in [
        "\"W1\"",
        "private decision text",
        "mixed decision text",
        "Private evidence 1",
    ] {
        assert!(!output.contains(private), "leaked {private}");
    }
}

#[test]
fn legacy_related_context_retains_its_serialized_shape() {
    let mut f = Fixture::legacy();
    f.dependency("W0", "W1", true);
    f.reproject();
    f.evidence(0);
    let scoped = related_work_in_workstream(&f.read(0), "W0", None, None, None).unwrap();
    let legacy = awr_context::related_work(&f.store, f.project, "W0", None, None, None).unwrap();
    assert_eq!(
        serde_json::to_value(scoped).unwrap(),
        serde_json::to_value(legacy).unwrap()
    );
}
