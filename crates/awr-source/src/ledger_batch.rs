//! Complete same-source preflight. All transformations happen in memory.
use crate::*;
use awr_core::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum LedgerBatchOperation {
    Fields {
        target: String,
        fields: Map<String, Value>,
    },
    Import {
        external_key: String,
        title: String,
        #[serde(default)]
        fields: Map<String, Value>,
        duplicate: ImportDuplicate,
    },
    Archive {
        target: String,
        archived: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportDuplicate {
    Fail,
    SkipExact,
}
pub struct PreparedLedgerBatch {
    pub path: PathBuf,
    pub spec: SourceSpec,
    pub before: SourceSnapshot,
    pub after: SourceSnapshot,
    pub projection: ProjectionBatch,
    pub outcomes: Vec<Value>,
    pub archive_targets: Vec<Id>,
}
fn invalid(s: &str) -> Error {
    Error::InvalidInput(s.into())
}
fn fields_valid(fields: &Map<String, Value>) -> Result<()> {
    if fields
        .keys()
        .any(|k| !yaml_field_writable(EntityKind::WorkItem, k))
    {
        return Err(Error::MutationUnsupported(
            "batch fields cannot write identity, lifecycle or verification fields".into(),
        ));
    }
    Ok(())
}
fn parse(
    source: &Source,
    spec: &SourceSpec,
    snapshot: &SourceSnapshot,
    ids: &BTreeMap<(EntityKind, String), Id>,
) -> Result<ProjectionBatch> {
    source_adapter(&source.adapter)?.parse(
        snapshot,
        &ParseContext {
            source,
            existing_ids: ids.clone(),
        },
        spec,
    )
}
fn raw(snapshot: &SourceSnapshot, spec: &SourceSpec, work: &WorkItem) -> Result<Value> {
    if spec.adapter == "markdown-ledger-v1" {
        let rows = crate::markdown_records::rows(snapshot.text()?, spec)?;
        return rows
            .into_iter()
            .find(|r| r.values.get("id") == Some(&json!(work.meta.external_key)))
            .map(|r| Value::Object(r.values))
            .ok_or_else(|| {
                Error::MutationUnsupported("batch writes require explicit Markdown IDs".into())
            });
    }
    let doc: Value =
        serde_yaml_ng::from_str(snapshot.text()?).map_err(|_| invalid("invalid YAML ledger"))?;
    doc.pointer(
        work.meta
            .source_ref
            .pointer
            .as_deref()
            .ok_or_else(|| invalid("exact record pointer required"))?,
    )
    .cloned()
    .ok_or_else(|| invalid("missing ledger record"))
}
fn edit(
    snapshot: &SourceSnapshot,
    spec: &SourceSpec,
    work: &WorkItem,
    changes: &Map<String, Value>,
) -> Result<String> {
    if spec.adapter == "markdown-ledger-v1" {
        return crate::markdown_mutation::edit_markdown_fields(
            snapshot.text()?,
            spec,
            &work.meta.external_key,
            changes,
        );
    }
    let mapping = LedgerMapping::from_spec(spec)?;
    let record = raw(snapshot, spec, work)?;
    let mut fields = Map::new();
    for (k, v) in changes {
        let v = mapping.write_value(k, v, &record)?;
        if record.get(mapping.source_field(k)) != Some(&v) {
            fields.insert(mapping.source_field(k).to_owned(), v);
        }
    }
    if fields.is_empty() {
        return Ok(snapshot.text()?.into());
    }
    let pointer = work.meta.source_ref.pointer.as_deref().unwrap();
    let mut expected: Value =
        serde_yaml_ng::from_str(snapshot.text()?).map_err(|_| invalid("invalid YAML"))?;
    expected
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("record mapping required"))?
        .extend(fields.clone());
    let output = crate::yaml_edit::edit_fields(snapshot.text()?, pointer, &fields)?;
    let actual: Value =
        serde_yaml_ng::from_str(&output).map_err(|_| invalid("invalid edited YAML"))?;
    if expected != actual {
        return Err(Error::MutationUnsupported(
            "batch edit changed unrelated source facts".into(),
        ));
    }
    Ok(output)
}
pub fn prepare_ledger_batch(
    root: &Path,
    source: &Source,
    operations: &[LedgerBatchOperation],
    ids: BTreeMap<(EntityKind, String), Id>,
) -> Result<PreparedLedgerBatch> {
    if operations.is_empty() || operations.len() > 100 {
        return Err(invalid("batch requires 1..100 operations"));
    }
    if source.domain != "ledger"
        || !["yaml-ledger-v1", "markdown-ledger-v1"].contains(&source.adapter.as_str())
    {
        return Err(Error::MutationUnsupported(
            "batch requires a writable registered ledger".into(),
        ));
    }
    if source.freshness != Freshness::Fresh {
        return Err(Error::SourceStale("refresh ledger before a batch".into()));
    }
    let (locator, spec, before) = inspect_registered_source(root, source)?;
    let Locator::File(path) = locator else {
        return Err(Error::MutationUnsupported(
            "Git sources are read-only".into(),
        ));
    };
    if path.starts_with(root.canonicalize()?.join(".awr")) {
        return Err(Error::RuleViolation(
            "runtime files cannot be ledgers".into(),
        ));
    }
    if before.fingerprint != source.fingerprint {
        return Err(Error::SourceConflict(
            "ledger changed before batch preflight".into(),
        ));
    }
    let mut after = before.clone();
    let mut seen = BTreeSet::new();
    let mut outcomes = vec![];
    let mut archive_targets = vec![];
    for op in operations {
        let target = match op {
            LedgerBatchOperation::Fields { target, .. }
            | LedgerBatchOperation::Archive { target, .. } => target,
            LedgerBatchOperation::Import { external_key, .. } => external_key,
        };
        if target.trim().is_empty()
            || target.len() > 512
            || target.chars().any(char::is_control)
            || !seen.insert(target.clone())
        {
            return Err(invalid(
                "batch requires unique nonempty stable keys; combine fields for the same target",
            ));
        }
        let mut projection = parse(source, &spec, &after, &ids)?;
        let existing = projection
            .work_items
            .iter()
            .find(|w| w.meta.external_key == *target)
            .cloned();
        let mut skipped = false;
        let output = match op {
            LedgerBatchOperation::Fields { fields, .. } => {
                fields_valid(fields)?;
                edit(
                    &after,
                    &spec,
                    &existing.ok_or_else(|| Error::NotFound(target.clone()))?,
                    fields,
                )?
            }
            LedgerBatchOperation::Archive { archived, .. } => {
                let work = existing.ok_or_else(|| Error::NotFound(target.clone()))?;
                if work.archived == *archived {
                    after.text()?.into()
                } else {
                    archive_targets.push(work.meta.id);
                    edit(
                        &after,
                        &spec,
                        &work,
                        &Map::from_iter([("archived".into(), json!(archived))]),
                    )?
                }
            }
            LedgerBatchOperation::Import {
                external_key,
                title,
                fields,
                duplicate,
            } => {
                fields_valid(fields)?;
                if title.trim().is_empty() || fields.contains_key("title") {
                    return Err(invalid("import requires a nonempty title declared once"));
                }
                if let Some(work) = existing {
                    if *duplicate != ImportDuplicate::SkipExact
                        || work.status != WorkStatus::Draft
                        || work.archived
                        || work.title != *title
                    {
                        return Err(Error::SourceConflict(format!(
                            "import key {target} already exists"
                        )));
                    }
                    let original = raw(&after, &spec, &work)?;
                    let record = if spec.adapter == "yaml-ledger-v1" {
                        LedgerMapping::from_spec(&spec)?.record(&original)?
                    } else {
                        original
                    };
                    if fields.iter().any(|(k, v)| record.get(k) != Some(v)) {
                        return Err(Error::SourceConflict(
                            "skip_exact import fields differ from existing draft".into(),
                        ));
                    }
                    skipped = true;
                    after.text()?.into()
                } else {
                    let output = if spec.adapter == "markdown-ledger-v1" {
                        crate::markdown_mutation::append_markdown_work(
                            after.text()?,
                            &spec,
                            external_key,
                            title,
                        )?
                    } else {
                        let mapping = LedgerMapping::from_spec(&spec)?;
                        let base = vec![
                            (mapping.source_field("id").into(), json!(external_key)),
                            (mapping.source_field("title").into(), json!(title)),
                            (
                                mapping.source_field("status").into(),
                                mapping.write_value("status", &json!("draft"), &json!({}))?,
                            ),
                        ];
                        crate::yaml_edit::append_work(after.text()?, external_key, &base)?
                    };
                    let snapshot = SourceSnapshot {
                        locator: after.locator.clone(),
                        fingerprint: fingerprint(output.as_bytes()),
                        bytes: output.into_bytes(),
                    };
                    projection = parse(source, &spec, &snapshot, &ids)?;
                    let work = projection
                        .work_items
                        .iter()
                        .find(|w| w.meta.external_key == *target && w.status == WorkStatus::Draft)
                        .ok_or_else(|| {
                            Error::MutationUnsupported("import must preserve draft status".into())
                        })?;
                    edit(&snapshot, &spec, work, fields)?
                }
            }
        };
        crate::limits::check_source_size(output.as_bytes(), source_read_cap(&source.adapter)?)?;
        let changed = output.as_bytes() != after.bytes;
        after = SourceSnapshot {
            locator: before.locator.clone(),
            fingerprint: fingerprint(output.as_bytes()),
            bytes: output.into_bytes(),
        };
        outcomes.push(json!({"external_key":target,"status":if skipped{"skipped_exact"}else if changed{"changed"}else{"no_change"}}));
    }
    let projection = parse(source, &spec, &after, &ids)?;
    Ok(PreparedLedgerBatch {
        path,
        spec,
        before,
        after,
        projection,
        outcomes,
        archive_targets,
    })
}
