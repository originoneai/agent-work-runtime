//! Explicit metadata fields outside projected entities; all other YAML bytes stay authoritative.
use awr_core::{Error, Result};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn organization_fields(text: &str, mapping: &BTreeMap<String, String>) -> Result<Value> {
    let document: Value =
        serde_yaml_ng::from_str(text).map_err(|e| Error::InvalidInput(e.to_string()))?;
    let mut fields = Map::new();
    let mut paths = BTreeSet::new();
    if mapping.is_empty() || mapping.len() > 4 {
        return Err(Error::InvalidInput("map 1..4 organization fields".into()));
    }
    for (name, pointer) in mapping {
        if !["phase", "scope", "focus", "next_action"].contains(&name.as_str()) {
            return Err(Error::MutationUnsupported(
                "only phase, scope, focus and next_action metadata are supported".into(),
            ));
        }
        let components = pointer
            .strip_prefix('/')
            .ok_or_else(|| Error::InvalidInput("organization fields require JSON pointers".into()))?
            .split('/')
            .collect::<Vec<_>>();
        if components.len() < 2
            || components.len() > 8
            || components.iter().any(|p| {
                p.is_empty() || p.contains('~') || p.chars().all(|c| c.is_ascii_digit()) || {
                    let s = p.to_ascii_lowercase();
                    s.contains("contract")
                        || s.contains("release")
                        || s.contains("count")
                        || [
                            "status",
                            "verification",
                            "acceptance",
                            "evidence",
                            "schema_version",
                            "total",
                        ]
                        .contains(&s.as_str())
                }
            })
            || !paths.insert(pointer)
            || [
                "project",
                "work_items",
                "milestones",
                "plans",
                "goals",
                "decisions",
                "rules",
            ]
            .contains(&components[0])
        {
            return Err(Error::MutationUnsupported("organization mapping must select distinct metadata leaves outside contracts, lifecycle, counts and entity collections".into()));
        }
        let parent = pointer.rsplit_once('/').unwrap().0;
        // Mapping-only ancestry prevents using this route to patch array records.
        let mut at = &document;
        for part in &components[..components.len() - 1] {
            at = at.as_object().and_then(|o| o.get(*part)).ok_or_else(|| {
                Error::SourceConflict(format!(
                    "metadata parent {parent} is missing or not a mapping"
                ))
            })?;
        }
        if !at.is_object() {
            return Err(Error::MutationUnsupported(
                "metadata parent must be a mapping".into(),
            ));
        }
        fields.insert(
            name.clone(),
            document.pointer(pointer).cloned().unwrap_or(Value::Null),
        );
    }
    if paths.iter().any(|a| {
        paths
            .iter()
            .any(|b| a != b && b.starts_with(&format!("{a}/")))
    }) {
        return Err(Error::MutationUnsupported(
            "overlapping organization pointers".into(),
        ));
    }
    Ok(Value::Object(fields))
}

pub fn edit_organization_fields(
    text: &str,
    mapping: &BTreeMap<String, String>,
    changes: &Map<String, Value>,
) -> Result<(String, Value, Value)> {
    let before = organization_fields(text, mapping)?;
    if changes.is_empty() || changes.keys().any(|k| !mapping.contains_key(k)) {
        return Err(Error::InvalidInput(
            "every changed field requires its explicit mapping".into(),
        ));
    }
    let mut groups = BTreeMap::<String, Map<String, Value>>::new();
    let mut expected: Value =
        serde_yaml_ng::from_str(text).map_err(|e| Error::InvalidInput(e.to_string()))?;
    for (name, value) in changes {
        let pointer = &mapping[name];
        let (parent, leaf) = pointer.rsplit_once('/').unwrap();
        groups
            .entry(parent.into())
            .or_default()
            .insert(leaf.into(), value.clone());
        expected
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(leaf.into(), value.clone());
    }
    let mut after = text.to_owned();
    for (parent, fields) in groups {
        after = crate::yaml_edit::edit_fields(&after, &parent, &fields)?;
    }
    let actual: Value =
        serde_yaml_ng::from_str(&after).map_err(|e| Error::InvalidInput(e.to_string()))?;
    if actual != expected {
        return Err(Error::SourceConflict(
            "metadata edit changed unselected YAML values".into(),
        ));
    }
    let after_fields = organization_fields(&after, mapping)?;
    Ok((after, before, after_fields))
}
