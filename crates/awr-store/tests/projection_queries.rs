mod support;
use awr_core::*;
use awr_store::Store;
use support::Fixture;

#[test]
fn typed_queries_bind_provenance_and_keep_unresolved_rules() {
    let mut f = Fixture::new();
    let mut batch = ProjectionBatch::default();
    batch.goals.push(Goal {
        meta: f.meta("G-1"),
        title: "Goal".into(),
        status: "active".into(),
        priority: None,
        success_criteria: vec!["usable".into()],
        summary: "summary".into(),
    });
    batch.plans.push(Plan {
        meta: f.meta("P-1"),
        title: "Plan".into(),
        status: "planned".into(),
        scope: vec!["G-1".into()],
        summary: "plan".into(),
        kind: None,
        acceptance: vec![],
    });
    let definitions = [
        (ScopeKind::Project, "example"),
        (ScopeKind::Path, "crates/**"),
        (ScopeKind::Tag, "rust"),
        (ScopeKind::WorkItem, "W-1"),
        (ScopeKind::Agent, "agent-1"),
    ];
    for (i, (kind, value)) in definitions.into_iter().enumerate() {
        batch.rules.push(Rule {
            meta: f.meta(&format!("R-{i}")),
            text: "Preserve source facts".into(),
            severity: Some(Severity::Hard),
            scope: Some(Scope {
                kind,
                value: value.into(),
            }),
            unresolved: vec![],
        });
    }
    batch.rules.push(Rule {
        meta: f.meta("R-unknown"),
        text: "Do the right thing".into(),
        severity: None,
        scope: None,
        unresolved: vec!["missing metadata".into()],
    });
    f.commit(batch);
    let read = Store::open_readonly(&f.root.join("state.db")).unwrap();
    let goal = read.goal(f.project.id, "G-1").unwrap();
    assert_eq!(goal.item.meta.source_ref.source_revision, 1);
    assert_eq!(goal.source.fingerprint, "snapshot-1");
    assert_eq!(
        goal.project_revision,
        read.project(f.project.id).unwrap().project_revision
    );
    assert_eq!(read.plan(f.project.id, "P-1").unwrap().item.title, "Plan");
    assert!(matches!(
        read.goal(Id::new(), "G-1"),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        read.rule(f.project.id, "missing"),
        Err(Error::NotFound(_))
    ));
    let context = RuleContext {
        project_key: Some("example".into()),
        paths: Some(vec!["crates/core/src/lib.rs".into()]),
        tags: Some(vec!["rust".into()]),
        work_item_key: Some("W-1".into()),
        agent_id: Some("agent-1".into()),
    };
    let rules = read.rules_for(f.project.id, &context).unwrap();
    assert_eq!(rules.len(), 6);
    assert_eq!(
        rules
            .iter()
            .filter(|r| r.applicability == Applicability::Applicable)
            .count(),
        5
    );
    assert_eq!(rules.last().unwrap().applicability, Applicability::Unknown);
    assert!(!rules.last().unwrap().reasons.is_empty());
    assert!(
        read.rules_for(f.project.id, &RuleContext::default())
            .unwrap()
            .iter()
            .all(|r| r.applicability == Applicability::Unknown)
    );
    let mismatch = RuleContext {
        project_key: Some("other".into()),
        paths: Some(vec![]),
        tags: Some(vec![]),
        work_item_key: Some("W-2".into()),
        agent_id: Some("agent-2".into()),
    };
    assert_eq!(
        read.rules_for(f.project.id, &mismatch)
            .unwrap()
            .iter()
            .filter(|r| r.applicability == Applicability::NotApplicable)
            .count(),
        5
    );
    f.source = f
        .store
        .mark_source_freshness(&f.source, Freshness::Unavailable)
        .unwrap();
    assert_eq!(
        read.goal(f.project.id, "G-1").unwrap().source.freshness,
        Freshness::Unavailable
    );
    assert!(
        read.rules_for(f.project.id, &context)
            .unwrap()
            .iter()
            .all(|r| r.applicability == Applicability::Unknown)
    );
    f.store.retire_source(&f.source).unwrap();
    assert!(read.goals(f.project.id).unwrap().is_empty());
    assert!(read.rules(f.project.id).unwrap().is_empty());
}

#[test]
fn path_scope_is_explicit_and_invalid_patterns_are_not_false_negatives() {
    assert!(match_path_scope("crates/**", &["crates/core/src/lib.rs".into()]).unwrap());
    assert!(!match_path_scope("crates/*.rs", &["crates/core/lib.rs".into()]).unwrap());
    assert!(match_path_scope("crates/", &["./crates/core/lib.rs".into()]).unwrap());
    assert!(match_path_scope("[", &["crates/lib.rs".into()]).is_err());
    assert!(match_path_scope("**", &["../private.rs".into()]).is_err());
    assert!(match_path_scope("/absolute/**", &[]).is_err());
}
