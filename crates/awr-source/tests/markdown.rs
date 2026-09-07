use awr_core::{EntityKind, Error, Freshness, Id, ScopeKind, Severity, Source};
use awr_source::{
    Manifest, MarkdownHeadingAdapter, MarkdownRulesAdapter, ParseContext, SourceAdapter,
    SourceSnapshot, fingerprint, markdown_sections,
};
use std::collections::BTreeMap;

fn snapshot(text: &str) -> SourceSnapshot {
    SourceSnapshot {
        locator: "file:///fixture/document.md".into(),
        fingerprint: fingerprint(text.as_bytes()),
        bytes: text.as_bytes().to_vec(),
    }
}
fn source() -> Source {
    Source {
        id: Id::new(),
        project_id: Id::new(),
        domain: "rules".into(),
        role: "primary".into(),
        locator: "file:///fixture/document.md".into(),
        format: "markdown".into(),
        adapter: "markdown-rules-v1".into(),
        revision: 0,
        fingerprint: String::new(),
        freshness: Freshness::Stale,
        config: serde_json::json!({}),
    }
}

#[test]
fn heading_boundaries_preserve_exact_sections() {
    let snapshot = snapshot(include_str!("../../../tests/fixtures/markdown/goals.md"));
    let sections = markdown_sections(&snapshot).unwrap();
    assert_eq!(
        sections
            .iter()
            .map(|s| s.anchor.as_str())
            .collect::<Vec<_>>(),
        ["product", "critical", "repeated", "repeated-2"]
    );
    assert_eq!(sections[1].title, "Critical path");
    assert_eq!((sections[1].start_line, sections[1].end_line), (5, 14));
    assert_eq!(sections[2].level, 2);
    assert_eq!(sections[2].body, "First occurrence.");
    let original = snapshot.text().unwrap();
    for section in &sections {
        let bytes = original
            .split_inclusive('\n')
            .skip(section.start_line - 1)
            .take(section.end_line - section.start_line + 1)
            .collect::<String>();
        assert_eq!(section.fingerprint, fingerprint(bytes.as_bytes()));
    }
    let unicode = snapshot_text("# 中文\r\n\r\n原样保存。\r\n## 后续\r\n保留。\r\n");
    assert_eq!(unicode[0].anchor, "中文");
    assert_eq!(
        unicode[0].fingerprint,
        fingerprint("# 中文\r\n\r\n原样保存。\r\n".as_bytes())
    );
}
fn snapshot_text(text: &str) -> Vec<awr_source::MarkdownSection> {
    markdown_sections(&snapshot(text)).unwrap()
}

#[test]
fn rule_metadata_is_explicit_and_adapters_are_read_only() {
    let manifest = Manifest::parse(include_str!(
        "../../../tests/fixtures/markdown/project.toml"
    ))
    .unwrap();
    let source = source();
    let context = ParseContext {
        source: &source,
        existing_ids: BTreeMap::new(),
    };
    let snapshot = snapshot(include_str!("../../../tests/fixtures/markdown/rules.md"));
    let adapter = MarkdownRulesAdapter;
    let batch = adapter
        .parse(&snapshot, &context, &manifest.sources[1])
        .unwrap();
    assert_eq!(batch.rules.len(), 5);
    let authority = batch
        .rules
        .iter()
        .find(|r| r.meta.external_key == "fixture-rules#authority")
        .unwrap();
    assert_eq!(authority.severity, Some(Severity::Hard));
    assert!(matches!(
        authority.scope.as_ref().unwrap().kind,
        ScopeKind::Project
    ));
    assert!(authority.unresolved.is_empty());
    let local = batch
        .rules
        .iter()
        .find(|r| r.meta.external_key == "fixture-rules#local")
        .unwrap();
    assert_eq!(local.severity, Some(Severity::Soft));
    assert_eq!(local.scope.as_ref().unwrap().value, "src/**");
    for key in ["unclear", "unsupported"] {
        let rule = batch
            .rules
            .iter()
            .find(|r| r.meta.external_key == format!("fixture-rules#{key}"))
            .unwrap();
        assert!(rule.severity.is_none() && rule.scope.is_none());
        assert_eq!(rule.unresolved.len(), 2);
        assert!(rule.meta.source_ref.section_fingerprint.is_some());
    }
    assert!(matches!(
        adapter.plan_mutation(&source, &serde_json::json!({})),
        Err(Error::MutationUnsupported(_))
    ));
    assert!(matches!(
        MarkdownHeadingAdapter.plan_mutation(&source, &serde_json::json!({})),
        Err(Error::MutationUnsupported(_))
    ));
    let mut configured = manifest.sources[1].clone();
    for (key, value) in [("severity", "hard"), ("scope", "project"), ("value", "*")] {
        configured.options.insert(key.into(), value.into());
    }
    let configured = adapter.parse(&snapshot, &context, &configured).unwrap();
    assert!(
        configured
            .rules
            .iter()
            .find(|r| r.meta.external_key == "fixture-rules#unclear")
            .unwrap()
            .unresolved
            .is_empty()
    );
    assert!(
        !configured
            .rules
            .iter()
            .find(|r| r.meta.external_key == "fixture-rules#unsupported")
            .unwrap()
            .unresolved
            .is_empty()
    );
}

#[test]
fn source_bound_keys_and_conflicting_anchors() {
    let manifest = Manifest::parse(include_str!(
        "../../../tests/fixtures/markdown/project.toml"
    ))
    .unwrap();
    let source = source();
    let id = Id::new();
    let context = ParseContext {
        source: &source,
        existing_ids: BTreeMap::from([((EntityKind::Goal, "fixture-goals#product".into()), id)]),
    };
    let original = snapshot(include_str!("../../../tests/fixtures/markdown/goals.md"));
    let batch = MarkdownHeadingAdapter
        .parse(&original, &context, &manifest.sources[0])
        .unwrap();
    assert_eq!(batch.goals[0].meta.id, id);
    assert_eq!(
        batch.goals[0].summary,
        "Keep the actual project state available."
    );
    assert_eq!(
        batch.goals[0].meta.source_ref.source_fingerprint,
        original.fingerprint
    );
    assert!(matches!(
        markdown_sections(&snapshot("# One {#same}\n\n# Two {#same}\n")),
        Err(Error::SourceConflict(_))
    ));
}
