use awr_context::*;
use awr_core::{Error, Freshness, Id};
use serde_json::json;

fn identity() -> ContextIdentity {
    ContextIdentity {
        project_id: Id::new(),
        project_key: "example".into(),
        project_revision: 10,
        work_item_id: Id::new(),
        work_item_key: "WORK-1".into(),
        work_item_revision: 3,
        branch_id: None,
        source_versions: vec![
            SourceVersion {
                id: Id::new(),
                revision: 2,
                fingerprint: "a".repeat(64),
                freshness: Freshness::Fresh,
                locator: "file:///project/ledger.yaml".into(),
            },
            SourceVersion {
                id: Id::new(),
                revision: 1,
                fingerprint: "b".repeat(64),
                freshness: Freshness::Fresh,
                locator: "file:///project/rules.md".into(),
            },
        ],
    }
}
fn required(id: &ContextIdentity) -> Vec<ContextChunk> {
    vec![ContextChunk {key: "work".into(), section: ContextSection::Work,
        text: "Status: blocked\nBlocker: 等待真实来源\nNext Action: 读取文件并继续\nAcceptance: 必须保留全部内容".into(),
        entities: vec![SelectedEntity {kind: "work_item".into(), id: id.work_item_id, revision: id.work_item_revision}]},
        ContextChunk {key: "rule".into(), section: ContextSection::Rules, text: "Hard rule: 正文和符号 <|endoftext|> 原样保留。".into(),
            entities: vec![SelectedEntity {kind: "rule".into(), id: Id::new(), revision: 1}]}]
}
fn optional(key: &str, text: &str, priority: u16) -> RankedChunk {
    RankedChunk {
        priority,
        recency: 1,
        chunk: ContextChunk {
            key: key.into(),
            section: ContextSection::Evidence,
            text: text.into(),
            entities: vec![],
        },
    }
}

#[test]
fn whole_chunks_keep_hard_facts_and_later_small_chunks_fit_after_a_large_rejection() {
    let id = identity();
    let required = required(&id);
    let candidates = vec![
        optional("large", &"OPTIONAL-HISTORY-SENTINEL ".repeat(4000), 0),
        optional("small", "A small evidence summary.", 1),
    ];
    let hard = budget_context(&id, &json!({}), &required, &[], 10000).unwrap();
    let budget = hard.token_estimate + 50;
    let pack = budget_context(&id, &json!({}), &required, &candidates, budget).unwrap();
    assert!(pack.token_estimate <= budget);
    assert_eq!(pack.token_estimate, token_count(&pack.rendered_context));
    assert_eq!(pack.tokenizer, "o200k_base");
    assert!(pack.token_count_scope.contains("envelope"));
    for chunk in &required {
        assert!(pack.rendered_context.contains(&chunk.text));
    }
    assert!(pack.rendered_context.contains("A small evidence summary."));
    assert!(!pack.rendered_context.contains("OPTIONAL-HISTORY-SENTINEL"));
    assert_eq!(pack.omitted_chunks.len(), 1);
    assert_eq!(pack.omitted_chunks[0].key, "large");
    assert!(
        matches!(budget_context(&id, &json!({}), &required, &[], hard.token_estimate-1), Err(Error::BudgetExceeded {required: n, ..}) if n == hard.token_estimate)
    );
    assert_eq!(
        budget_context(&id, &json!({}), &required, &[], hard.token_estimate)
            .unwrap()
            .token_estimate,
        hard.token_estimate
    );
    assert!(serde_json::to_string(&pack).unwrap().len() > pack.rendered_context.len());
}

#[test]
fn input_order_and_json_object_order_do_not_change_selection_text_or_hash() {
    let mut id = identity();
    let mut required = required(&id);
    let mut candidates = vec![optional("z", "Last key", 1), optional("a", "First key", 1)];
    let first = budget_context(
        &id,
        &json!({"b":2,"a":{"z":3,"x":1}}),
        &required,
        &candidates,
        5000,
    )
    .unwrap();
    id.source_versions.reverse();
    id.source_versions.push(id.source_versions[0].clone());
    required.reverse();
    let duplicate = required[0].entities[0].clone();
    required[0].entities.push(duplicate);
    candidates.reverse();
    let second = budget_context(
        &id,
        &json!({"a":{"x":1,"z":3},"b":2}),
        &required,
        &candidates,
        5000,
    )
    .unwrap();
    assert_eq!(first.rendered_context, second.rendered_context);
    assert_eq!(first.context_hash, second.context_hash);
    assert_eq!(second.identity.source_versions.len(), 2);
    assert!(
        first.rendered_context.find("[a]").unwrap() < first.rendered_context.find("[z]").unwrap()
    );
}

#[test]
fn equal_priority_budget_prefers_recent_optional_context_regardless_of_input_order() {
    let id = identity();
    let required = required(&id);
    let old = optional("old", &"An event detail. ".repeat(30), 1);
    let mut new = optional("new", &"An event detail. ".repeat(30), 1);
    new.recency = 2;
    let budget = budget_context(&id, &json!({}), &required, &[new.clone()], 5000)
        .unwrap()
        .token_estimate
        + 1;
    let pack = budget_context(&id, &json!({}), &required, &[old, new], budget).unwrap();
    assert!(
        pack.selected_chunks
            .iter()
            .any(|c| !c.required && c.key == "new")
    );
    assert_eq!(pack.omitted_chunks.len(), 1);
    assert_eq!(pack.omitted_chunks[0].key, "old");
}

#[test]
fn context_hash_binds_project_work_branch_sources_entities_request_and_rendered_text() {
    let id = identity();
    let required = required(&id);
    let base = budget_context(&id, &json!({"agent":"a"}), &required, &[], 5000).unwrap();
    let compile = |id: &ContextIdentity, chunks: &[ContextChunk]| {
        budget_context(id, &json!({"agent":"a"}), chunks, &[], 5000)
            .unwrap()
            .context_hash
    };
    let mut changed = id.clone();
    changed.project_revision += 1;
    assert_ne!(base.context_hash, compile(&changed, &required));
    let mut changed = id.clone();
    changed.branch_id = Some(Id::new());
    assert_ne!(base.context_hash, compile(&changed, &required));
    let mut changed = id.clone();
    changed.source_versions[0].revision += 1;
    assert_ne!(base.context_hash, compile(&changed, &required));
    let mut changed = id.clone();
    changed.source_versions[0].fingerprint = "c".repeat(64);
    assert_ne!(base.context_hash, compile(&changed, &required));
    let mut changed = id.clone();
    changed.source_versions[0].freshness = Freshness::Stale;
    assert_ne!(base.context_hash, compile(&changed, &required));
    let mut changed = required.clone();
    changed[1].entities[0].id = Id::new();
    assert_ne!(base.context_hash, compile(&id, &changed));
    let mut changed = required.clone();
    changed[1].text.push_str(" Additional mandatory statement.");
    assert_ne!(base.context_hash, compile(&id, &changed));
    let mut changed = id.clone();
    changed.work_item_revision += 1;
    let mut chunks = required.clone();
    chunks[0].entities[0].revision += 1;
    assert_ne!(base.context_hash, compile(&changed, &chunks));
    assert_ne!(
        base.context_hash,
        budget_context(&id, &json!({"agent":"b"}), &required, &[], 5000)
            .unwrap()
            .context_hash
    );
    assert_ne!(
        base.context_hash,
        budget_context(&id, &json!({"agent":"a"}), &required, &[], 5001)
            .unwrap()
            .context_hash
    );
}

#[test]
fn conflicting_snapshots_and_ambiguous_chunk_keys_are_rejected() {
    let id = identity();
    let required = required(&id);
    let mut changed = id.clone();
    let mut source = changed.source_versions[0].clone();
    source.revision += 1;
    changed.source_versions.push(source);
    assert!(matches!(
        budget_context(&changed, &json!({}), &required, &[], 5000),
        Err(Error::InvalidInput(_))
    ));
    let duplicate = RankedChunk {
        priority: 0,
        recency: 1,
        chunk: required[0].clone(),
    };
    assert!(matches!(
        budget_context(&id, &json!({}), &required, &[duplicate], 5000),
        Err(Error::InvalidInput(_))
    ));
    let mut conflicting = required.clone();
    conflicting[0].entities[0].revision += 1;
    assert!(matches!(
        budget_context(&id, &json!({}), &conflicting, &[], 5000),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        token_count("中文 English <|endoftext|> 👨‍👩‍👧‍👦\n"),
        tiktoken_rs::o200k_base_singleton()
            .encode_ordinary("中文 English <|endoftext|> 👨‍👩‍👧‍👦\n")
            .len()
    );
}
