mod support;
use awr_core::*;
use serde_json::json;
use support::Fixture;
fn review(f: &Fixture, text: &str) -> VerifiedSourceContentReview {
    let assessment = ContentAssessment::scan(text.as_bytes(), &f.source.locator).unwrap();
    let decisions = assessment
        .findings
        .iter()
        .map(|v| PublicContentDecision {
            finding_id: v.id.clone(),
            reason: "Verified synthetic public protocol marker.".into(),
        })
        .collect();
    SourceContentReview {
        project_root: f.project.root.to_string_lossy().into(),
        version: CONTENT_REVIEW_VERSION,
        assessment,
        reviewer: "test-agent".into(),
        reviewed_at: 1000,
        decisions,
    }
    .verify(text.as_bytes(), &f.source.locator)
    .unwrap()
}
fn batch(f: &Fixture, permit: &VerifiedSourceContentReview, text: &str) -> ProjectionBatch {
    let mut meta = f.meta("G-1");
    meta.source_ref.source_fingerprint =
        format!("sha256:{}", permit.receipt().assessment.source_sha256);
    ProjectionBatch {
        goals: vec![Goal {
            meta,
            title: "Public protocol".into(),
            status: "active".into(),
            priority: None,
            success_criteria: vec!["usable".into()],
            summary: text.into(),
        }],
        ..Default::default()
    }
}
#[test]
fn reviewed_projection_is_atomic_scoped_and_survives_reopen() {
    let mut f = Fixture::new();
    let text = "password: public-marker";
    let permit = review(&f, text);
    let fingerprint = format!("sha256:{}", permit.receipt().assessment.source_sha256);
    let mut wrong = batch(&f, &permit, text);
    wrong.goals[0].summary = "password: unrelated-marker".into();
    assert!(
        f.store
            .commit_reviewed_source_projection(&f.source, &fingerprint, wrong, &permit)
            .is_err()
    );
    assert!(f.store.goals(f.project.id).unwrap().is_empty());
    let candidate = batch(&f, &permit, text);
    assert!(
        f.store
            .commit_source_projection(&f.source, &fingerprint, candidate.clone())
            .is_err()
    );
    f.source = f
        .store
        .commit_reviewed_source_projection(&f.source, &fingerprint, candidate, &permit)
        .unwrap();
    let store = awr_store::Store::open_readonly(&f.root.join("state.db")).unwrap();
    let goal = store.goal(f.project.id, "G-1").unwrap();
    assert_eq!(goal.item.summary, text);
    store.ensure_source_output(&goal.item).unwrap();
    let mut unrelated = serde_json::to_value(&goal.item).unwrap();
    unrelated["next_action"] = json!(text);
    assert!(store.ensure_source_output(&unrelated).is_err());
    let mut wrong_ref = serde_json::to_value(&goal.item).unwrap();
    wrong_ref["source_ref"]["source_revision"] = json!(100);
    assert!(store.ensure_source_output(&wrong_ref).is_err());
    let forged_meta = json!({"meta":{"id":goal.item.meta.id,"revision":goal.item.meta.revision,"external_key":text,"source_ref":goal.item.meta.source_ref},"summary":text});
    assert!(store.ensure_source_output(&forged_meta).is_err());
    store
        .ensure_entity_text(
            f.project.id,
            &[("goal".into(), goal.item.meta.id, goal.item.meta.revision)],
            text,
        )
        .unwrap();
    assert!(
        store
            .ensure_entity_text(
                Id::new(),
                &[("goal".into(), goal.item.meta.id, goal.item.meta.revision)],
                text
            )
            .is_err()
    );
    assert!(
        store
            .ensure_entity_text(
                f.project.id,
                &[("goal".into(), goal.item.meta.id, 999)],
                text
            )
            .is_err()
    );
    assert!(store.ensure_entity_text(f.project.id, &[], text).is_err());
    assert!(ensure_public_text(text).is_err());
    drop(store);
    let configured = f
        .store
        .configure_source(&f.source, json!({"adapter_version":999}))
        .unwrap();
    assert!(f.store.goal(f.project.id, "G-1").is_err());
    assert!(configured.freshness != Freshness::Fresh);
}
#[test]
fn copied_review_cannot_authorize_another_project_or_source() {
    let f = Fixture::new();
    let permit = review(&f, "password: public-marker");
    let mut other = Fixture::new();
    let candidate = batch(&other, &permit, "password: public-marker");
    let fingerprint = format!("sha256:{}", permit.receipt().assessment.source_sha256);
    assert!(
        other
            .store
            .commit_reviewed_source_projection(&other.source, &fingerprint, candidate, &permit)
            .is_err()
    );
    assert!(other.store.goals(other.project.id).unwrap().is_empty());
}
