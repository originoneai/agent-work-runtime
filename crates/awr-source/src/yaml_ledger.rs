use crate::{Locator, Manifest, ParseContext, SourceAdapter, SourceSnapshot, SourceSpec};
use awr_core::{
    Edge, EntityKind, Error, Evidence, EvidenceLevel, Goal, Id, Plan, ProjectionBatch, Result,
    SourceRef, WorkItem, WorkStatus,
};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

pub struct YamlLedgerAdapter;

struct Entry<'a> {
    key: String,
    pointer: String,
    value: &'a Value,
}

fn string(value: &Value, field: &str) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(Error::InvalidInput(format!("{field} must be a string"))),
    }
}
fn strings(value: &Value, field: &str) -> Result<Vec<String>> {
    match value {
        Value::Null => Ok(vec![]),
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                string(v, field)?
                    .ok_or_else(|| Error::InvalidInput(format!("{field} contains null")))
            })
            .collect(),
        _ => Err(Error::InvalidInput(format!(
            "{field} must be a string or string list"
        ))),
    }
}
fn boolean(value: &Value, default: bool, field: &str) -> Result<bool> {
    if value.is_null() {
        Ok(default)
    } else {
        value
            .as_bool()
            .ok_or_else(|| Error::InvalidInput(format!("{field} must be boolean")))
    }
}
fn alias<'a>(value: &'a Value, first: &str, second: &str) -> Result<&'a Value> {
    if !value[first].is_null() && !value[second].is_null() && value[first] != value[second] {
        return Err(Error::InvalidInput(format!(
            "conflicting {first} and {second}"
        )));
    }
    Ok(if value[first].is_null() {
        &value[second]
    } else {
        &value[first]
    })
}
fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn entries<'a>(document: &'a Value, field: &str) -> Result<Vec<Entry<'a>>> {
    let raw: Vec<(Option<&str>, String, &Value)> = match &document[field] {
        Value::Null => vec![],
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (None, format!("/{}/{i}", escape(field)), v))
            .collect(),
        Value::Object(items) => items
            .iter()
            .map(|(key, v)| {
                (
                    Some(key.as_str()),
                    format!("/{}/{}", escape(field), escape(key)),
                    v,
                )
            })
            .collect(),
        _ => {
            return Err(Error::InvalidInput(format!(
                "{field} must be a list or keyed map"
            )));
        }
    };
    let mut keys = BTreeSet::new();
    raw.into_iter()
        .map(|(fallback, pointer, value)| {
            if !value.is_object() {
                return Err(Error::InvalidInput(format!("{pointer} must be a mapping")));
            }
            let key = string(alias(value, "external_key", "id")?, &pointer)?
                .or(fallback.map(str::to_owned))
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    Error::InvalidInput(format!("{pointer} needs an id or external_key"))
                })?;
            if let Some(fallback) = fallback {
                if fallback != key {
                    return Err(Error::SourceConflict(format!(
                        "{pointer}: map key and entity key differ"
                    )));
                }
            }
            if !keys.insert(key.clone()) {
                return Err(Error::SourceConflict(format!(
                    "duplicate {field} key {key}"
                )));
            }
            Ok(Entry {
                key,
                pointer,
                value,
            })
        })
        .collect()
}

impl SourceAdapter for YamlLedgerAdapter {
    fn name(&self) -> &'static str {
        "yaml-ledger-v1"
    }
    fn discover(
        &self,
        root: &Path,
        manifest: &Manifest,
        spec: &SourceSpec,
    ) -> Result<Vec<Locator>> {
        Ok(vec![Locator::from_spec(root, manifest, spec)?])
    }
    fn parse(
        &self,
        snapshot: &SourceSnapshot,
        context: &ParseContext<'_>,
        spec: &SourceSpec,
    ) -> Result<ProjectionBatch> {
        crate::limits::check_source_size(&snapshot.bytes, crate::YAML_READ_CAP)?;
        if spec.domain != "ledger" {
            return Err(Error::InvalidInput(
                "yaml-ledger-v1 requires the ledger domain".into(),
            ));
        }
        let mapping = crate::LedgerMapping::from_spec(spec)?;
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(snapshot.text()?)
            .map_err(|_| Error::InvalidInput("invalid YAML ledger document".into()))?;
        let document = serde_json::to_value(yaml)?;
        awr_core::ensure_public_value(&document)?;
        let document = mapping.document(document)?;
        if !document.is_object()
            || !["work_items", "milestones", "goals"]
                .iter()
                .any(|key| document.get(key).is_some())
        {
            return Err(Error::InvalidInput(
                "YAML ledger needs work_items, milestones or goals".into(),
            ));
        }
        let mut batch = ProjectionBatch::default();
        for Entry {
            key,
            pointer,
            value,
        } in entries(&document, "goals")?
        {
            let title = string(&value["title"], &pointer)?.unwrap_or_else(|| key.clone());
            batch.goals.push(Goal {
                meta: context.meta(
                    EntityKind::Goal,
                    &key,
                    snapshot,
                    Some(pointer.clone()),
                    None,
                )?,
                title,
                status: string(&value["status"], &pointer)?.unwrap_or_else(|| "unknown".into()),
                priority: string(&value["priority"], &pointer)?,
                summary: string(&value["summary"], &pointer)?.unwrap_or_default(),
                success_criteria: strings(
                    alias(value, "success_criteria", "acceptance")?,
                    &pointer,
                )?,
            });
        }
        for Entry {
            key,
            pointer,
            value,
        } in entries(&document, "milestones")?
        {
            let title = string(&value["title"], &pointer)?
                .or(string(&value["name"], &pointer)?)
                .unwrap_or_else(|| key.clone());
            batch.plans.push(Plan {
                meta: context.meta(
                    EntityKind::Plan,
                    &key,
                    snapshot,
                    Some(pointer.clone()),
                    None,
                )?,
                title,
                status: string(&value["status"], &pointer)?.unwrap_or_else(|| "unknown".into()),
                kind: Some("milestone".into()),
                summary: string(&value["summary"], &pointer)?.unwrap_or_default(),
                scope: strings(&value["scope"], &pointer)?,
                acceptance: strings(alias(value, "acceptance", "success_criteria")?, &pointer)?,
            });
        }
        for Entry {
            key,
            pointer,
            value,
        } in entries(&document, "work_items")?
        {
            let raw_status = string(&value["status"], &pointer)?.unwrap_or_default();
            let status = mapping.status(&raw_status);
            if status == WorkStatus::Unknown {
                batch.warnings.push(format!(
                    "{pointer}/status: unknown raw status {raw_status:?}; no transition inferred"
                ));
            }
            let meta = context.meta(
                EntityKind::WorkItem,
                &key,
                snapshot,
                Some(pointer.clone()),
                None,
            )?;
            let direct_level = &value["evidence_level"];
            let nested_level = &value["verification"]["evidence_level"];
            if !direct_level.is_null() && !nested_level.is_null() && direct_level != nested_level {
                return Err(Error::InvalidInput(format!(
                    "{pointer}: conflicting evidence levels"
                )));
            }
            let raw_level = if direct_level.is_null() {
                nested_level
            } else {
                direct_level
            };
            let evidence_level = match string(raw_level, &pointer)? {
                None => None,
                Some(level) if level == "none" => None,
                Some(level) => Some(
                    serde_json::from_value(Value::String(level.clone())).unwrap_or_else(|_| {
                        batch
                            .warnings
                            .push(format!("{pointer}: unknown evidence level {level:?}"));
                        EvidenceLevel::Unknown
                    }),
                ),
            };
            let work = WorkItem {
                ordinary_completion: value
                    .get("ordinary_completion")
                    .filter(|v| !v.is_null())
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()?,
                meta,
                title: string(&value["title"], &pointer)?.unwrap_or_else(|| key.clone()),
                kind: string(&value["kind"], &pointer)?,
                owner: string(&value["owner"], &pointer)?,
                required: boolean(
                    alias(value, "required", "required_for_v1")?,
                    false,
                    &pointer,
                )?,
                raw_status,
                status,
                priority: string(&value["priority"], &pointer)?,
                milestone: string(&value["milestone"], &pointer)?,
                score: if value["score"].is_null() {
                    None
                } else {
                    Some(value["score"].as_i64().ok_or_else(|| {
                        Error::InvalidInput(format!("{pointer}/score must be an integer"))
                    })?)
                },
                evidence_level,
                summary: string(&value["summary"], &pointer)?.unwrap_or_default(),
                next_action: string(&value["next_action"], &pointer)?.unwrap_or_default(),
                blocker: string(&value["blocker"], &pointer)?,
                acceptance: strings(&value["acceptance"], &pointer)?,
                tags: strings(&value["tags"], &pointer)?,
                paths: strings(alias(value, "paths", "deliverables")?, &pointer)?,
            };
            let deps = alias(value, "depends_on", "dependencies")?;
            let dependencies = if deps.is_null() {
                vec![]
            } else if let Some(a) = deps.as_array() {
                a.iter().collect()
            } else {
                return Err(Error::InvalidInput(format!(
                    "{pointer}: dependencies must be a list"
                )));
            };
            let mut targets = BTreeSet::new();
            for (index, dependency) in dependencies.into_iter().enumerate() {
                let (target, required) = if dependency.is_object() {
                    (
                        string(alias(dependency, "id", "key")?, &pointer)?.ok_or_else(|| {
                            Error::InvalidInput("dependency needs id or key".into())
                        })?,
                        boolean(&dependency["required"], true, &pointer)?,
                    )
                } else {
                    (
                        string(dependency, &pointer)?.ok_or_else(|| {
                            Error::InvalidInput("dependency cannot be null".into())
                        })?,
                        true,
                    )
                };
                if !targets.insert(target.clone()) {
                    return Err(Error::SourceConflict(format!(
                        "{pointer}: duplicate dependency {target}"
                    )));
                }
                let mut reference = work.meta.source_ref.clone();
                let field = if value.get("depends_on").is_some() {
                    "depends_on"
                } else {
                    "dependencies"
                };
                reference.pointer = Some(format!(
                    "{pointer}/{}/{index}",
                    escape(mapping.source_field(field))
                ));
                batch.edges.push(edge(
                    context,
                    &key,
                    "depends_on",
                    EntityKind::WorkItem,
                    &target,
                    required,
                    reference,
                ));
            }
            if let Some(milestone) = &work.milestone {
                let mut reference = work.meta.source_ref.clone();
                reference.pointer = Some(format!(
                    "{pointer}/{}",
                    escape(mapping.source_field("milestone"))
                ));
                batch.edges.push(edge(
                    context,
                    &key,
                    "part_of",
                    EntityKind::Plan,
                    milestone,
                    true,
                    reference,
                ));
            }
            for goal in strings(alias(value, "goal", "goals")?, &pointer)? {
                let mut reference = work.meta.source_ref.clone();
                reference.pointer = Some(format!(
                    "{pointer}/{}",
                    escape(mapping.source_field(if value.get("goal").is_some() {
                        "goal"
                    } else {
                        "goals"
                    }))
                ));
                batch.edges.push(edge(
                    context,
                    &key,
                    "supports",
                    EntityKind::Goal,
                    &goal,
                    true,
                    reference,
                ));
            }
            parse_evidence(context, snapshot, value, &work, &pointer, &mut batch)?;
            batch.work_items.push(work);
        }
        Ok(batch)
    }
}

fn edge(
    context: &ParseContext<'_>,
    key: &str,
    relation: &str,
    to_kind: EntityKind,
    target: &str,
    required: bool,
    source_ref: SourceRef,
) -> Edge {
    Edge {
        id: Id::new(),
        project_id: context.source.project_id,
        from_kind: EntityKind::WorkItem,
        from_key: key.into(),
        relation: relation.into(),
        to_kind,
        to_key: target.into(),
        required,
        revision: 1,
        source_ref,
    }
}

fn parse_evidence(
    context: &ParseContext<'_>,
    snapshot: &SourceSnapshot,
    value: &Value,
    work: &WorkItem,
    pointer: &str,
    batch: &mut ProjectionBatch,
) -> Result<()> {
    let items = match &value["evidence"] {
        Value::Null => return Ok(()),
        Value::Array(items) => items,
        _ => {
            return Err(Error::InvalidInput(format!(
                "{pointer}/evidence must be a list"
            )));
        }
    };
    let mut locators = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let location = if item.is_object() {
            alias(item, "locator", "path")?
        } else {
            item
        };
        let locator = string(location, pointer)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| Error::InvalidInput("evidence reference needs a locator".into()))?;
        if !locators.insert(locator.clone()) {
            return Err(Error::SourceConflict(format!(
                "{pointer}: duplicate evidence locator {locator}"
            )));
        }
        let key = format!("{}/evidence/{locator}", work.meta.external_key);
        let meta = context.meta(
            EntityKind::Evidence,
            &key,
            snapshot,
            Some(format!("{pointer}/evidence/{index}")),
            None,
        )?;
        batch.evidence.push(Evidence {
            id: meta.id,
            project_id: context.source.project_id,
            work_item_id: Some(work.meta.id),
            external_key: key,
            evidence_type: "source_reference".into(),
            level: EvidenceLevel::Unknown,
            summary: if item.is_object() {
                string(&item["summary"], pointer)?
                    .unwrap_or_else(|| "Evidence reference; not yet verified by AWR".into())
            } else {
                "Evidence reference; not yet verified by AWR".into()
            },
            locator,
            sha256: None,
            source_sha: None,
            command: None,
            scope: vec![work.meta.external_key.clone()],
            source_ref: Some(meta.source_ref),
            branch_id: None,
            revision: 1,
            verified_at: None,
        });
    }
    Ok(())
}
