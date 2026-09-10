//! Explicit finite ADR adoption; filenames and timestamps never choose the accepted version.
use crate::*;
use awr_core::*;
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, ops::Range, path::Path};

fn ranges(text: &str) -> Result<(Range<usize>, usize)> {
    let mut lines = text.split_inclusive('\n');
    let first = lines
        .next()
        .ok_or_else(|| Error::InvalidInput("empty decision".into()))?;
    if first.trim_end() != "---" {
        return Err(Error::MutationUnsupported(
            "adoption requires explicit YAML front matter with id and status".into(),
        ));
    }
    let start = first.len();
    let mut at = start;
    for line in lines {
        if matches!(line.trim_end(), "---" | "...") {
            return Ok((start..at, at + line.len()));
        }
        at += line.len();
    }
    Err(Error::InvalidInput("unterminated decision metadata".into()))
}
/// Bind all declared content and exact body bytes, excluding only lifecycle receipts.
pub(crate) fn decision_content_fingerprint(text: &str) -> Result<String> {
    let (_, body) = ranges(text)?;
    let mut metadata = crate::directory::frontmatter(text)?;
    for key in ["status", "adoption", "superseded_by"] {
        metadata.as_object_mut().unwrap().remove(key);
    }
    Ok(fingerprint(&serde_json::to_vec(&(
        metadata,
        &text[body..],
    ))?))
}
fn write_metadata(text: &str, fields: Map<String, Value>) -> Result<String> {
    let (range, _) = ranges(text)?;
    let raw = &text[range.clone()];
    let mut expected: Value = serde_yaml_ng::from_str(raw)
        .map_err(|_| Error::InvalidInput("invalid decision metadata".into()))?;
    if expected["id"].as_str().is_none() || expected["status"].as_str().is_none() {
        return Err(Error::MutationUnsupported(
            "adoption requires canonical id and status in front matter".into(),
        ));
    }
    expected.as_object_mut().unwrap().extend(fields.clone());
    let output = crate::yaml_edit::edit_fields(raw, "", &fields)?;
    let actual: Value = serde_yaml_ng::from_str(&output)
        .map_err(|_| Error::InvalidInput("invalid edited decision metadata".into()))?;
    if actual != expected {
        return Err(Error::MutationUnsupported(
            "adoption would change other metadata".into(),
        ));
    }
    let mut out = text.to_owned();
    out.replace_range(range, &output);
    Ok(out)
}
pub fn prepare_decision_lifecycle(
    root: &Path,
    source: &Source,
    reference: &DocumentVersion,
    adoption: &DecisionAdoption,
    superseding: bool,
    ids: BTreeMap<(EntityKind, String), Id>,
) -> Result<PreparedDocument> {
    if source.id != reference.source_id
        || source.domain != "decisions"
        || source.adapter != "markdown-directory-v1"
    {
        return Err(Error::MutationUnsupported(
            "explicit adoption requires a registered decision document".into(),
        ));
    }
    let (locator, spec, before) = inspect_registered_source(root, source)?;
    let Locator::File(path) = locator else {
        return Err(Error::MutationUnsupported(
            "Git decision versions are read-only".into(),
        ));
    };
    document_path_registration(root, &path)?;
    if before.fingerprint != reference.source_fingerprint
        || source.fingerprint != before.fingerprint
    {
        return Err(Error::SourceConflict(
            "decision content version changed since review".into(),
        ));
    }
    let context = ParseContext {
        source,
        existing_ids: ids,
    };
    let adapter = source_adapter(&source.adapter)?;
    let prior = adapter.parse(&before, &context, &spec)?;
    let old = prior
        .decisions
        .first()
        .filter(|d| d.meta.external_key == reference.external_key)
        .ok_or_else(|| {
            Error::SourceConflict("decision key does not match the bound source".into())
        })?;
    let fields = if superseding {
        if old.status != DecisionStatus::Accepted {
            return Err(Error::InvalidTransition(
                "only an accepted decision can be explicitly superseded".into(),
            ));
        }
        Map::from_iter([
            ("status".into(), json!("superseded")),
            ("superseded_by".into(), json!(adoption.candidate)),
        ])
    } else {
        if old.status != DecisionStatus::Proposed
            && !(old.status == DecisionStatus::Unknown && old.adoption.is_some())
        {
            return Err(Error::InvalidTransition(
                "adoption requires a proposed decision or an invalidated previous approval".into(),
            ));
        }
        let mut receipt = adoption.clone();
        receipt.content_fingerprint = decision_content_fingerprint(before.text()?)?;
        Map::from_iter([
            ("status".into(), json!("accepted")),
            ("adoption".into(), json!(receipt)),
        ])
    };
    let output = write_metadata(before.text()?, fields)?;
    crate::limits::check_source_size(output.as_bytes(), MARKDOWN_READ_CAP)?;
    let after = SourceSnapshot {
        locator: before.locator.clone(),
        fingerprint: fingerprint(output.as_bytes()),
        bytes: output.into_bytes(),
    };
    let next = adapter.parse(&after, &context, &spec)?;
    if next.decisions.len() != 1
        || next.decisions[0].meta.id != old.meta.id
        || next.decisions[0].status
            != if superseding {
                DecisionStatus::Superseded
            } else {
                DecisionStatus::Accepted
            }
        || decision_content_fingerprint(before.text()?)?
            != decision_content_fingerprint(after.text()?)?
    {
        return Err(Error::RuleViolation(
            "decision metadata is ambiguous or adoption would change its identity/content".into(),
        ));
    }
    Ok(PreparedDocument {
        path,
        source: source.clone(),
        spec,
        before: Some(before),
        after,
    })
}
