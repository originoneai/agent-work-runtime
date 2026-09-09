use crate::{
    Locator, ParseContext, SourceAdapter, SourceSnapshot, SourceSpec, YamlLedgerAdapter,
    fingerprint, inspect_mutation_source,
};
use awr_core::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ops::Range,
    path::{Path, PathBuf},
};
use yaml_rust2::parser::{Event, Parser};

pub struct PreparedYamlMutation {
    pub path: PathBuf,
    pub before: SourceSnapshot,
    pub after: SourceSnapshot,
    pub plan: MutationWritePlan,
}

fn unsupported(message: &str) -> Error {
    Error::MutationUnsupported(message.into())
}
fn bytes_at(text: &str, index: usize) -> Result<usize> {
    text.char_indices()
        .nth(index)
        .map(|(byte, _)| byte)
        .or_else(|| (text.chars().count() == index).then_some(text.len()))
        .ok_or_else(|| Error::InvalidInput("YAML marker is outside its source".into()))
}
fn token(parser: &mut Parser<std::str::Chars<'_>>) -> Result<(Event, usize)> {
    parser
        .next_token()
        .map(|(event, mark)| (event, mark.index()))
        .map_err(|e| Error::InvalidInput(format!("YAML mutation syntax: {e}")))
}
fn node(
    parser: &mut Parser<std::str::Chars<'_>>,
    first: (Event, usize),
    path: &mut Vec<String>,
    target: &[String],
    text: &str,
    found: &mut Option<Range<usize>>,
    depth: usize,
    in_target: bool,
) -> Result<()> {
    if depth > 128 {
        return Err(unsupported("YAML mutation nesting exceeds 128 levels"));
    }
    let here = path == target;
    let in_target = in_target || here;
    match first.0 {
        Event::MappingStart(anchor, tag) => {
            if in_target && (anchor != 0 || tag.is_some()) {
                return Err(unsupported(
                    "anchored or tagged target mappings require manual mutation",
                ));
            }
            let flow = here && text[bytes_at(text, first.1)?..].starts_with('{');
            let mut mapping_start = first.1;
            let mut first_key = true;
            let end = loop {
                let key = token(parser)?;
                if key.0 == Event::MappingEnd {
                    break key.1;
                }
                if first_key && !flow {
                    mapping_start = key.1;
                }
                first_key = false;
                let Event::Scalar(key, _, anchor, tag) = key.0 else {
                    return Err(unsupported(
                        "complex YAML mapping keys require manual mutation",
                    ));
                };
                if anchor != 0 || tag.is_some() {
                    return Err(unsupported(
                        "tagged or anchored YAML keys require manual mutation",
                    ));
                }
                path.push(key);
                let value = token(parser)?;
                node(
                    parser,
                    value,
                    path,
                    target,
                    text,
                    found,
                    depth + 1,
                    in_target,
                )?;
                path.pop();
            };
            if here {
                if found.is_some() {
                    return Err(Error::SourceConflict(
                        "YAML pointer resolved more than once".into(),
                    ));
                }
                let start = bytes_at(text, mapping_start)?;
                let mut end = bytes_at(text, end)?;
                if text[start..].starts_with('{') {
                    if !text[end..].starts_with('}') {
                        return Err(unsupported("could not bound the flow mapping exactly"));
                    }
                    end += 1;
                } else {
                    // Leave separators, indentation and standalone trailing comments outside
                    // the replacement. Comments within the selected record may be normalized.
                    end = start + text[start..end].trim_end().len();
                    loop {
                        let line = text[..end]
                            .rfind('\n')
                            .map_or(start, |n| (n + 1).max(start));
                        if line <= start || !text[line..end].trim_start().starts_with('#') {
                            break;
                        }
                        end = start + text[start..line].trim_end().len();
                    }
                }
                *found = Some(start..end);
            }
        }
        Event::SequenceStart(anchor, tag) => {
            if in_target && (anchor != 0 || tag.is_some()) {
                return Err(unsupported(
                    "anchored or tagged target values require manual mutation",
                ));
            }
            if here {
                return Err(unsupported("a mutation target must be a YAML mapping"));
            }
            let mut index = 0;
            loop {
                let child = token(parser)?;
                if child.0 == Event::SequenceEnd {
                    break;
                }
                path.push(index.to_string());
                node(
                    parser,
                    child,
                    path,
                    target,
                    text,
                    found,
                    depth + 1,
                    in_target,
                )?;
                path.pop();
                index += 1;
            }
        }
        Event::Scalar(_, _, anchor, tag) => {
            if here || (in_target && (anchor != 0 || tag.is_some())) {
                return Err(unsupported(
                    "scalar, anchored or tagged targets require manual mutation",
                ));
            }
        }
        Event::Alias(_) if !here => (),
        Event::Alias(_) => return Err(unsupported("an aliased target requires manual mutation")),
        _ => return Err(unsupported("unexpected YAML node boundary")),
    }
    Ok(())
}
fn target_range(text: &str, pointer: &str) -> Result<Range<usize>> {
    let parts = pointer
        .strip_prefix('/')
        .ok_or_else(|| unsupported("YAML target requires a JSON pointer"))?
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    let mut parser = Parser::new_from_str(text);
    if token(&mut parser)?.0 != Event::StreamStart || token(&mut parser)?.0 != Event::DocumentStart
    {
        return Err(unsupported("YAML stream has no document"));
    }
    let first = token(&mut parser)?;
    let mut found = None;
    node(
        &mut parser,
        first,
        &mut Vec::new(),
        &parts,
        text,
        &mut found,
        0,
        false,
    )?;
    if token(&mut parser)?.0 != Event::DocumentEnd || token(&mut parser)?.0 != Event::StreamEnd {
        return Err(unsupported(
            "multiple YAML documents require manual mutation",
        ));
    }
    found.ok_or_else(|| Error::SourceConflict("YAML mutation target pointer is missing".into()))
}
fn document(snapshot: &SourceSnapshot) -> Result<Value> {
    crate::limits::check_source_size(&snapshot.bytes, crate::YAML_READ_CAP)?;
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(snapshot.text()?)
        .map_err(|e| Error::InvalidInput(format!("YAML mutation: {e}")))?;
    Ok(serde_json::to_value(yaml)?)
}

/// Read the exact current YAML record when constructing a preserving domain mutation.
pub fn read_yaml_mutation_record(
    root: &Path,
    source: &Source,
    patch: &MutationPatch,
) -> Result<Value> {
    crate::verify_mutation_source(root, source, patch)?;
    if source.adapter != "yaml-ledger-v1" {
        return Err(unsupported(
            "completion requires a writable YAML ledger source",
        ));
    }
    let (_, spec, snapshot) = inspect_mutation_source(root, source, patch)?;
    if snapshot.fingerprint != patch.target.meta.source_ref.source_fingerprint {
        return Err(Error::SourceConflict(
            "source changed while reading its record".into(),
        ));
    }
    let record = document(&snapshot)?
        .pointer(patch.target.meta.source_ref.pointer.as_deref().unwrap())
        .cloned()
        .ok_or_else(|| Error::SourceConflict("exact YAML record is missing".into()))?;
    if patch.target.kind == EntityKind::WorkItem {
        crate::LedgerMapping::from_spec(&spec)?.record(&record)
    } else {
        Ok(record)
    }
}
fn allowed(kind: EntityKind, field: &str) -> bool {
    match kind {
        EntityKind::Goal => matches!(
            field,
            "title" | "status" | "priority" | "summary" | "success_criteria" | "acceptance"
        ),
        EntityKind::Plan => matches!(
            field,
            "title" | "name" | "status" | "summary" | "scope" | "acceptance" | "success_criteria"
        ),
        // Runtime domain actions will enable guarded work state/ownership changes. Generic
        // update_fields cannot become a back door around completion and evidence checks.
        EntityKind::WorkItem => matches!(
            field,
            "title"
                | "kind"
                | "priority"
                | "required"
                | "required_for_v1"
                | "summary"
                | "next_action"
                | "score"
                | "tags"
                | "paths"
                | "deliverables"
                | "acceptance"
                | "milestone"
                | "depends_on"
                | "dependencies"
                | "goal"
                | "goals"
        ),
        EntityKind::Evidence => field == "summary",
        _ => false,
    }
}
pub fn parse_mutation_projection(
    source: &Source,
    spec: &SourceSpec,
    snapshot: &SourceSnapshot,
    existing_ids: BTreeMap<(EntityKind, String), Id>,
    target: &MutationTarget,
) -> Result<(ProjectionBatch, String)> {
    let batch = YamlLedgerAdapter.parse(
        snapshot,
        &ParseContext {
            source,
            existing_ids,
        },
        spec,
    )?;
    let value = serde_json::to_value(&batch)?;
    let key = match target.kind {
        EntityKind::Goal => "goals",
        EntityKind::Plan => "plans",
        EntityKind::WorkItem => "work_items",
        EntityKind::Evidence => "evidence",
        _ => return Err(unsupported("this entity kind has no YAML writer")),
    };
    let rows = value[key].as_array().unwrap();
    let object = rows
        .iter()
        .find(|row| {
            row["id"] == serde_json::json!(target.meta.id)
                && row["external_key"] == target.meta.external_key
        })
        .ok_or_else(|| {
            Error::SourceConflict("source mutation changed the target identity".into())
        })?;
    Ok((batch, mutation_projection_hash(object)?))
}
pub fn prepare_yaml_mutation(
    root: &Path,
    source: &Source,
    proposal: &MutationProposal,
    existing_ids: BTreeMap<(EntityKind, String), Id>,
) -> Result<PreparedYamlMutation> {
    let patch = proposal.bound_patch()?;
    if source.adapter != "yaml-ledger-v1" {
        return Err(unsupported("this source adapter has no automatic writer"));
    }
    let (locator, spec, before) = inspect_mutation_source(root, source, &patch)?;
    let Locator::File(path) = locator else {
        return Err(unsupported("Git sources remain read-only"));
    };
    if path.starts_with(root.canonicalize()?.join(".awr")) {
        return Err(unsupported(
            "runtime-owned files cannot be source mutation targets",
        ));
    }
    if before.fingerprint != proposal.base_fingerprint {
        return Err(Error::SourceConflict(
            "source changed before preparing YAML mutation".into(),
        ));
    }
    let changes = patch.changes.as_object().unwrap();
    if let Some(field) = changes
        .keys()
        .find(|field| patch.work_action.is_none() && !allowed(patch.target.kind, field))
    {
        return Err(unsupported(&format!(
            "field {field} is not supported by this writer; work state, ownership and verification changes require domain actions"
        )));
    }
    let pointer = patch.target.meta.source_ref.pointer.as_deref().unwrap();
    let original = document(&before)?;
    let mapping = if patch.target.kind == EntityKind::WorkItem {
        crate::LedgerMapping::from_spec(&spec)?
    } else {
        crate::LedgerMapping::default()
    };
    if let Some(binding) = patch
        .work_action
        .as_ref()
        .and_then(|a| a.completion.as_ref())
    {
        let record = original
            .pointer(pointer)
            .ok_or_else(|| unsupported("exact YAML record is missing"))?;
        if patch.changes != completion_source_changes(&mapping.record(record)?, binding)? {
            return Err(Error::InvalidInput("completion patch must preserve existing evidence and verification metadata exactly".into()));
        }
    }
    let mut expected = original.clone();
    let record = expected
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| unsupported("the exact YAML record is not a mapping"))?;
    for (key, value) in changes {
        let original_record = original.pointer(pointer).unwrap();
        record.insert(
            mapping.source_field(key).into(),
            if patch.target.kind == EntityKind::WorkItem {
                mapping.write_value(key, value, original_record)?
            } else {
                value.clone()
            },
        );
    }
    if expected == original {
        return Err(Error::InvalidInput(
            "proposal would not change its source record".into(),
        ));
    }
    let replacement = serde_json::to_string(&expected.pointer(pointer).unwrap())?
        .replace('\u{85}', "\\u0085")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    let range = target_range(before.text()?, pointer)?;
    let mut output = before.text()?.to_string();
    output.replace_range(range, &replacement);
    crate::limits::check_source_size(output.as_bytes(), crate::YAML_READ_CAP)?;
    let after = SourceSnapshot {
        locator: before.locator.clone(),
        fingerprint: fingerprint(output.as_bytes()),
        bytes: output.into_bytes(),
    };
    if document(&after)? != expected {
        return Err(unsupported(
            "exact target replacement cannot preserve the document's other facts",
        ));
    }
    let (_, target_after_hash) =
        parse_mutation_projection(source, &spec, &after, existing_ids, &patch.target)?;
    let plan = MutationWritePlan {
        id: Id::new(),
        before_fingerprint: before.fingerprint.clone(),
        after_fingerprint: after.fingerprint.clone(),
        before_size: before.bytes.len() as u64,
        after_size: after.bytes.len() as u64,
        target_after_hash,
    };
    plan.validate()?;
    Ok(PreparedYamlMutation {
        path,
        before,
        after,
        plan,
    })
}
