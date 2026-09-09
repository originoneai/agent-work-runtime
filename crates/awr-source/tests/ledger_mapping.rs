use awr_core::{Error, Freshness, Id, Source, WorkStatus};
use awr_source::{
    Manifest, MarkdownLedgerAdapter, ParseContext, SourceAdapter, SourceSnapshot,
    YamlLedgerAdapter, fingerprint,
};
use std::collections::BTreeMap;

fn parse(text: &str, adapter: &str, options: &str) -> awr_core::Result<awr_core::ProjectionBatch> {
    let manifest = Manifest::parse(&format!(
        "[project]\nname='Mapping fixture'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='{adapter}'\n{options}"
    ))?;
    let source = Source {
        id: Id::new(),
        project_id: Id::new(),
        domain: "ledger".into(),
        role: "primary".into(),
        locator: "file:///fixture/work.yaml".into(),
        format: "yaml".into(),
        adapter: adapter.into(),
        revision: 1,
        fingerprint: String::new(),
        freshness: Freshness::Fresh,
        config: serde_json::json!({}),
    };
    let snapshot = SourceSnapshot {
        locator: source.locator.clone(),
        bytes: text.as_bytes().into(),
        fingerprint: fingerprint(text.as_bytes()),
    };
    let context = ParseContext {
        source: &source,
        existing_ids: BTreeMap::new(),
    };
    if adapter == "yaml-ledger-v1" {
        YamlLedgerAdapter.parse(&snapshot, &context, &manifest.sources[0])
    } else {
        MarkdownLedgerAdapter.parse(&snapshot, &context, &manifest.sources[0])
    }
}

#[test]
fn yaml_preserves_original_status_and_actual_relationship_pointers() {
    let text = "work_items:\n- ticket: W\n  name: 保留既有任务\n  phase: Pending\n  done_when: [Produce the report]\n  next: Write report\n  links: [DEP]\n  objective: G\n";
    let options = "[sources.options.field_map]\nid='ticket'\ntitle='name'\nstatus='phase'\nacceptance='done_when'\nnext_action='next'\ndepends_on='links'\ngoal='objective'\n[sources.options.status_map]\nPending='planned'\ncomplete='completed'\n";
    let batch = parse(text, "yaml-ledger-v1", options).unwrap();
    let work = &batch.work_items[0];
    assert_eq!(work.meta.external_key, "W");
    assert_eq!(work.raw_status, "Pending");
    assert_eq!(work.status, WorkStatus::Planned);
    assert_eq!(work.title, "保留既有任务");
    assert_eq!(work.acceptance, ["Produce the report"]);
    assert_eq!(work.next_action, "Write report");
    assert_eq!(
        batch.edges[0].source_ref.pointer.as_deref(),
        Some("/work_items/0/links/0")
    );
    assert_eq!(
        batch.edges[1].source_ref.pointer.as_deref(),
        Some("/work_items/0/objective")
    );
    let unknown = parse(
        "work_items: [{id: W, status: pending}]",
        "yaml-ledger-v1",
        "",
    )
    .unwrap();
    assert_eq!(unknown.work_items[0].status, WorkStatus::Unknown);
    assert!(!unknown.warnings.is_empty());
}

#[test]
fn markdown_original_chinese_columns_and_custom_names_are_retained() {
    let text = "# 权威台账\n\n| ID | 工作项 | 状态 | Owner 角色 | 完成硬门槛 | 当前证据 / 下一动作 |\n|---|---|---|---|---|---|\n| W | 生成报告 | pending | Delivery | 读者收到报告 | 整理报告 |\n| D | 历史工作 | complete | Delivery | 报告正确 | 复核历史证据 |\n";
    let options = "[sources.options.status_map]\npending='planned'\ncomplete='completed'\n";
    let b = parse(text, "markdown-ledger-v1", options).unwrap();
    assert_eq!(b.work_items.len(), 2);
    assert_eq!(b.work_items[0].status, WorkStatus::Planned);
    assert_eq!(b.work_items[0].owner.as_deref(), Some("Delivery"));
    assert_eq!(b.work_items[0].next_action, "整理报告");
    assert_eq!(b.work_items[0].acceptance, ["读者收到报告"]);
    assert_eq!(b.work_items[0].raw_status, "pending");
    assert_eq!(b.work_items[1].status, WorkStatus::Completed);
    assert!(b.evidence.is_empty());
    assert_eq!(b.work_items[0].meta.source_ref.start_line, Some(5));
    let custom = text.replace("工作项", "业务事项").replace("状态", "阶段");
    let b = parse(
        &custom,
        "markdown-ledger-v1",
        &format!("{options}[sources.options.field_map]\ntitle='业务事项'\nstatus='阶段'\n"),
    )
    .unwrap();
    assert_eq!(b.work_items[0].title, "生成报告");
    assert_eq!(b.work_items[0].status, WorkStatus::Planned);
}

#[test]
fn mapping_conflicts_and_invalid_semantics_are_rejected() {
    for options in [
        "[sources.options.status_map]\ncompleted='planned'\n",
        "[sources.options.status_map]\npending='almost_ready'\n",
        "[sources.options.status_map]\nPending='planned'\npending='ready'\n",
        "[sources.options.field_map]\ntitle='same'\nstatus='same'\n",
        "[sources.options.field_map]\nstatus='evidence'\n",
        "[sources.options.field_map]\nverification='checks'\n",
    ] {
        assert!(
            parse("work_items: []", "yaml-ledger-v1", options).is_err(),
            "{options}"
        );
    }
    assert!(matches!(
        parse(
            "work_items: [{id: W, status: ready, phase: pending}]",
            "yaml-ledger-v1",
            "[sources.options.field_map]\nstatus='phase'\n"
        ),
        Err(Error::SourceConflict(_))
    ));
    assert!(matches!(
        parse(
            "| ID | 工作项 | 事项 | 状态 |\n|---|---|---|---|\n| W | One | Two | ready |\n",
            "markdown-ledger-v1",
            "[sources.options.field_map]\ntitle='事项'\n"
        ),
        Err(Error::SourceConflict(_))
    ));
}
