//! Whole-chunk deterministic selection. No LLM, character/token slicing, or hard-fact rewriting.
use crate::{HardContext, SourceVersion};
use awr_core::{Error, Id, Result, Revision};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub const TOKENIZER: &str = "o200k_base";
pub const BUDGET_POLICY: &str = "awr.whole_chunks.v1";
pub const TOKEN_COUNT_SCOPE: &str = "Exact o200k_base ordinary-text token count of rendered_context, including headings, references and omission footer. JSON transport/envelope, tool framing and surrounding conversation are excluded. Counts for other model tokenizers may differ; no cross-tokenizer error bound is claimed.";

pub fn token_count(text: &str) -> usize {
    tiktoken_rs::o200k_base_singleton()
        .encode_ordinary(text)
        .len()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSection {
    Goal,
    Work,
    Acceptance,
    Rules,
    Dependencies,
    Decisions,
    Delta,
    Evidence,
    Metadata,
}
impl ContextSection {
    fn title(self) -> &'static str {
        match self {
            Self::Goal => "Goal",
            Self::Work => "Current Work",
            Self::Acceptance => "Acceptance",
            Self::Rules => "Rules",
            Self::Dependencies => "Dependencies",
            Self::Decisions => "Decisions",
            Self::Delta => "Recent Delta",
            Self::Evidence => "Evidence",
            Self::Metadata => "Metadata",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SelectedEntity {
    pub kind: String,
    pub id: Id,
    pub revision: Revision,
}
#[derive(Debug, Clone, Serialize)]
pub struct ContextIdentity {
    pub project_id: Id,
    pub project_key: String,
    pub project_revision: Revision,
    pub work_item_id: Id,
    pub work_item_key: String,
    pub work_item_revision: Revision,
    pub branch_id: Option<Id>,
    pub source_versions: Vec<SourceVersion>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ContextChunk {
    /// Unique within a section. It is an identifier, never truncated by the budgeter.
    pub key: String,
    pub section: ContextSection,
    pub text: String,
    pub entities: Vec<SelectedEntity>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RankedChunk {
    /// Lower is preferred. Equal priority uses newer recency, section, then exact key.
    pub priority: u16,
    pub recency: Revision,
    pub chunk: ContextChunk,
}
#[derive(Debug, Clone, Serialize)]
pub struct ChunkSelection {
    pub key: String,
    pub section: ContextSection,
    pub required: bool,
    pub entities: Vec<SelectedEntity>,
}
#[derive(Debug, Clone, Serialize)]
pub struct OmittedChunk {
    pub key: String,
    pub section: ContextSection,
    pub entities: Vec<SelectedEntity>,
    pub reason: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct BudgetedContext {
    pub identity: ContextIdentity,
    pub rendered_context: String,
    pub context_hash: String,
    pub token_estimate: usize,
    pub required_tokens: usize,
    pub token_budget: usize,
    pub tokenizer: &'static str,
    pub token_count_scope: &'static str,
    pub policy: &'static str,
    pub selected_chunks: Vec<ChunkSelection>,
    pub selected_entities: Vec<SelectedEntity>,
    pub omitted_chunks: Vec<OmittedChunk>,
}

/// Preserve this hard subset verbatim; L1 callers must also add unresolved required dependencies
/// and other mandatory facts. This function does not certify whole-context completeness.
pub fn hard_chunks(hard: &HardContext) -> Result<Vec<ContextChunk>> {
    let entity = SelectedEntity {
        kind: "work_item".into(),
        id: hard.work.meta.id,
        revision: hard.work.meta.revision,
    };
    let mut chunks = vec![
        ContextChunk {
            key: hard.work.meta.external_key.clone(),
            section: ContextSection::Work,
            text: format!(
                "Status: {}\nRaw status: {}\nBlocker present: {}\nBlocker:\n{}\nNext Action:\n{}",
                serde_json::to_string(&hard.work.status)?,
                serde_json::to_string(&hard.work.raw_status)?,
                hard.work.blocker.is_some(),
                hard.work.blocker.as_deref().unwrap_or(""),
                hard.work.next_action
            ),
            entities: vec![entity.clone()],
        },
        ContextChunk {
            key: "acceptance".into(),
            section: ContextSection::Acceptance,
            text: hard
                .work
                .acceptance
                .iter()
                .enumerate()
                .map(|(i, text)| format!("{}. {text}", i + 1))
                .collect::<Vec<_>>()
                .join("\n"),
            entities: vec![entity],
        },
    ];
    for rule in &hard.rules {
        chunks.push(ContextChunk {
            key: rule.meta.external_key.clone(),
            section: ContextSection::Rules,
            text: format!(
                "Severity: hard\nScope: {}\n{}",
                serde_json::to_string(&rule.scope)?,
                rule.text
            ),
            entities: vec![SelectedEntity {
                kind: "rule".into(),
                id: rule.meta.id,
                revision: rule.meta.revision,
            }],
        });
    }
    let mut notes = hard.issues.clone();
    for rule in &hard.unresolved {
        notes.push(format!(
            "{}: {}",
            rule.rule.item.meta.external_key,
            rule.reasons.join("; ")
        ));
    }
    if !hard.complete {
        chunks.push(ContextChunk {
            key: "hard_context_gaps".into(),
            section: ContextSection::Metadata,
            text: format!("CONTEXT INCOMPLETE (hard subset)\n{}", notes.join("\n")),
            entities: hard
                .unresolved
                .iter()
                .map(|r| SelectedEntity {
                    kind: "rule".into(),
                    id: r.rule.item.meta.id,
                    revision: r.rule.item.meta.revision,
                })
                .collect(),
        });
    }
    Ok(chunks)
}

fn normalized_identity(input: &ContextIdentity) -> Result<ContextIdentity> {
    if input.project_key.trim().is_empty() || input.work_item_key.trim().is_empty() {
        return Err(Error::InvalidInput(
            "context identity keys must not be blank".into(),
        ));
    }
    let mut sources = BTreeMap::new();
    for source in &input.source_versions {
        if let Some(prior) = sources.insert(source.id, source.clone()) {
            if serde_json::to_value(prior)? != serde_json::to_value(source)? {
                return Err(Error::InvalidInput(format!(
                    "conflicting context source {}",
                    source.id
                )));
            }
        }
    }
    let mut identity = input.clone();
    identity.source_versions = sources.into_values().collect();
    Ok(identity)
}
fn normalize_chunk(chunk: &ContextChunk) -> Result<ContextChunk> {
    if chunk.key.trim().is_empty() || chunk.entities.iter().any(|e| e.kind.trim().is_empty()) {
        return Err(Error::InvalidInput(
            "context chunk and entity kinds must have keys".into(),
        ));
    }
    let mut chunk = chunk.clone();
    chunk.entities.sort();
    chunk.entities.dedup();
    Ok(chunk)
}

fn render(identity: &ContextIdentity, chunks: &[(&ContextChunk, bool)], omitted: usize) -> String {
    let mut text = format!(
        "Project: {} [{}] r{}\nWork: {} [{}] r{}\nBranch: {}\nSources:\n",
        identity.project_key,
        identity.project_id,
        identity.project_revision,
        identity.work_item_key,
        identity.work_item_id,
        identity.work_item_revision,
        identity
            .branch_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "main".into())
    );
    for source in &identity.source_versions {
        let _ = writeln!(
            text,
            "{} r{} {} {:?} {}",
            source.id, source.revision, source.fingerprint, source.freshness, source.locator
        );
    }
    let mut ordered = chunks.to_vec();
    ordered.sort_by(|(a, _), (b, _)| (a.section, &a.key).cmp(&(b.section, &b.key)));
    let mut section = None;
    for (chunk, _) in ordered {
        if section != Some(chunk.section) {
            let _ = write!(text, "\n## {}\n", chunk.section.title());
            section = Some(chunk.section);
        }
        let _ = writeln!(text, "[{}]", chunk.key);
        for entity in &chunk.entities {
            let _ = writeln!(text, "{}:{}@r{}", entity.kind, entity.id, entity.revision);
        }
        text.push_str(&chunk.text);
        text.push('\n');
    }
    let _ = write!(
        text,
        "\nOptional chunks omitted: {omitted}. Full details remain available through explicit queries.\n"
    );
    text
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| (k.clone(), canonical(v)))
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect(),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// Required chunks are indivisible. Optional candidates are tried in stable priority order;
/// a non-fitting candidate is skipped, allowing a later smaller chunk to fit. Render order is
/// fixed section/key order. Every trial counts the complete text, not the sum of chunk counts.
pub fn budget_context(
    identity: &ContextIdentity,
    request_binding: &Value,
    required: &[ContextChunk],
    optional: &[RankedChunk],
    token_budget: usize,
) -> Result<BudgetedContext> {
    if token_budget == 0 || token_budget > 100_000 {
        return Err(Error::InvalidInput(
            "context token budget must be 1..100000".into(),
        ));
    }
    let identity = normalized_identity(identity)?;
    let required = required
        .iter()
        .map(normalize_chunk)
        .collect::<Result<Vec<_>>>()?;
    let mut optional = optional
        .iter()
        .map(|candidate| {
            Ok(RankedChunk {
                priority: candidate.priority,
                recency: candidate.recency,
                chunk: normalize_chunk(&candidate.chunk)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    optional.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| b.recency.cmp(&a.recency))
            .then_with(|| (a.chunk.section, &a.chunk.key).cmp(&(b.chunk.section, &b.chunk.key)))
    });
    let mut keys = BTreeSet::new();
    let mut versions = BTreeMap::new();
    // Even an omitted candidate must not silently introduce an inconsistent snapshot.
    for chunk in required.iter().chain(optional.iter().map(|c| &c.chunk)) {
        if !keys.insert((chunk.section, &chunk.key)) {
            return Err(Error::InvalidInput(format!(
                "duplicate context chunk {}",
                chunk.key
            )));
        }
        for entity in &chunk.entities {
            if let Some(revision) = versions.insert((&entity.kind, entity.id), entity.revision) {
                if revision != entity.revision {
                    return Err(Error::InvalidInput(format!(
                        "conflicting entity revision {}",
                        entity.id
                    )));
                }
            }
            if entity.kind == "work_item"
                && entity.id == identity.work_item_id
                && entity.revision != identity.work_item_revision
            {
                return Err(Error::InvalidInput(
                    "work revision conflicts with context identity".into(),
                ));
            }
        }
    }
    let mut selected = required
        .iter()
        .map(|chunk| (chunk, true))
        .collect::<Vec<_>>();
    let required_tokens = token_count(&render(&identity, &selected, optional.len()));
    if required_tokens > token_budget {
        return Err(Error::BudgetExceeded {
            required: required_tokens,
            budget: token_budget,
        });
    }
    let mut selected_optional = 0;
    let mut omitted_chunks = Vec::new();
    for candidate in &optional {
        selected.push((&candidate.chunk, false));
        let trial = render(&identity, &selected, optional.len() - selected_optional - 1);
        if token_count(&trial) <= token_budget {
            selected_optional += 1;
        } else {
            selected.pop();
            omitted_chunks.push(OmittedChunk {
                key: candidate.chunk.key.clone(),
                section: candidate.chunk.section,
                entities: candidate.chunk.entities.clone(),
                reason: "insufficient_budget_for_whole_chunk",
            });
        }
    }
    let rendered_context = render(&identity, &selected, omitted_chunks.len());
    let token_estimate = token_count(&rendered_context);
    if token_estimate > token_budget {
        return Err(Error::BudgetExceeded {
            required: token_estimate,
            budget: token_budget,
        });
    }
    selected.sort_by(|(a, _), (b, _)| (a.section, &a.key).cmp(&(b.section, &b.key)));
    let selected_chunks = selected
        .iter()
        .map(|(chunk, required)| ChunkSelection {
            key: chunk.key.clone(),
            section: chunk.section,
            required: *required,
            entities: chunk.entities.clone(),
        })
        .collect::<Vec<_>>();
    let mut selected_entities = selected
        .iter()
        .flat_map(|(chunk, _)| chunk.entities.clone())
        .collect::<Vec<_>>();
    selected_entities.push(SelectedEntity {
        kind: "work_item".into(),
        id: identity.work_item_id,
        revision: identity.work_item_revision,
    });
    selected_entities.sort();
    selected_entities.dedup();
    let binding = canonical(
        &serde_json::json!({"policy":BUDGET_POLICY,"tokenizer":TOKENIZER,"budget":token_budget,
        "identity":identity,"request":request_binding,"selected_chunks":selected_chunks,"selected_entities":selected_entities,
        "omitted_chunks":omitted_chunks,"rendered_context":rendered_context}),
    );
    let context_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&binding)?));
    Ok(BudgetedContext {
        identity,
        rendered_context,
        context_hash,
        token_estimate,
        required_tokens,
        token_budget,
        tokenizer: TOKENIZER,
        token_count_scope: TOKEN_COUNT_SCOPE,
        policy: BUDGET_POLICY,
        selected_chunks,
        selected_entities,
        omitted_chunks,
    })
}
