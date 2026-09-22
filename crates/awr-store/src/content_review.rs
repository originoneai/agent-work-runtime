//! Projection-scoped review provenance. Runtime/request values never inherit it.
use crate::{
    Store,
    catalog::{SOURCE_COLUMNS, source_row},
    db_error,
};
use awr_core::{Error, Result, Source, SourceContentReview, VerifiedSourceContentReview};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
fn hash(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
pub(crate) fn bind_projection(
    conn: &Connection,
    source: &Source,
    fingerprint: &str,
    payload: &Value,
    review: Option<&VerifiedSourceContentReview>,
) -> Result<()> {
    if let Some(review) = review {
        review.ensure_value(payload)?;
        let receipt = serde_json::to_value(review.receipt())?;
        conn.execute("INSERT INTO source_content_reviews(source_id,project_id,source_fingerprint,source_config_json,adapter,receipt_json,receipt_id,policy_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(source_id) DO UPDATE SET source_fingerprint=excluded.source_fingerprint,source_config_json=excluded.source_config_json,adapter=excluded.adapter,receipt_json=excluded.receipt_json,receipt_id=excluded.receipt_id,policy_version=excluded.policy_version", params![source.id.to_string(),source.project_id.to_string(),fingerprint,serde_json::to_string(&source.config)?,source.adapter,serde_json::to_string(&receipt)?,hash(&receipt)?,awr_core::SECRET_POLICY_VERSION]).map_err(db_error)?;
    } else {
        conn.execute(
            "DELETE FROM source_content_reviews WHERE source_id=?1",
            [source.id.to_string()],
        )
        .map_err(db_error)?;
    }
    Ok(())
}
pub(crate) fn source_review(
    conn: &Connection,
    source: &Source,
) -> Result<Option<SourceContentReview>> {
    let row = conn.query_row("SELECT source_fingerprint,source_config_json,adapter,receipt_json,receipt_id,policy_version FROM source_content_reviews WHERE source_id=?1 AND project_id=?2", params![source.id.to_string(),source.project_id.to_string()], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,u32>(5)?))).optional().map_err(db_error)?;
    let Some((fingerprint, config, adapter, receipt, id, policy)) = row else {
        return Ok(None);
    };
    if source.freshness != awr_core::Freshness::Fresh
        || fingerprint != source.fingerprint
        || config != serde_json::to_string(&source.config)?
        || adapter != source.adapter
        || policy != awr_core::SECRET_POLICY_VERSION
    {
        return Err(Error::SourceConflict(
            "source content review is stale; refresh and review the current source".into(),
        ));
    }
    let value: Value = serde_json::from_str(&receipt)?;
    if hash(&value)? != id {
        return Err(Error::SourceConflict(
            "content review archive is inconsistent".into(),
        ));
    }
    Ok(Some(serde_json::from_value(value)?))
}
pub(crate) fn ensure_source_value(conn: &Connection, source: &Source, value: &Value) -> Result<()> {
    if awr_core::ensure_public_value(value).is_ok() {
        return Ok(());
    }
    match source_review(conn, source)? {
        Some(review) => review.ensure_derived_value(value),
        None => awr_core::ensure_public_value(value),
    }
}
fn reference(value: &Value) -> Option<&Value> {
    value
        .get("source_ref")
        .filter(|v| v.is_object())
        .or_else(|| {
            value
                .get("meta")
                .and_then(|m| m.get("source_ref"))
                .filter(|v| v.is_object())
        })
}
fn source_for_ref(conn: &Connection, reference: &Value) -> Result<Source> {
    let source: awr_core::SourceRef = serde_json::from_value(reference.clone())?;
    let current = conn
        .query_row(
            &format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE id=?1 AND active=1"),
            [source.source_id.to_string()],
            source_row,
        )
        .map_err(db_error)?;
    if current.fingerprint != source.source_fingerprint
        || current.revision != source.source_revision
    {
        return Err(Error::SourceConflict(
            "review provenance does not match the selected source version".into(),
        ));
    }
    Ok(current)
}
fn entity_payload(
    conn: &Connection,
    project: Option<awr_core::Id>,
    id: &str,
    revision: u64,
    kind: Option<&str>,
) -> Result<Value> {
    let mut found = None;
    for (name, table) in [
        ("goal", "goals"),
        ("plan", "plans"),
        ("rule", "rules"),
        ("work_item", "work_items"),
        ("decision", "decisions"),
        ("evidence", "evidence"),
    ] {
        if kind.is_some_and(|k| k != name) {
            continue;
        }
        let row: Option<String> = conn.query_row(&format!("SELECT payload_json FROM {table} WHERE id=?1 AND revision=?2 AND active=1 AND source_id IS NOT NULL AND (?3 IS NULL OR project_id=?3)"), params![id,i64::try_from(revision).map_err(|_|Error::InvalidInput("entity revision overflow".into()))?,project.map(|p|p.to_string())], |r|r.get(0)).optional().map_err(db_error)?;
        if let Some(row) = row {
            if found.is_some() {
                return Err(Error::SourceConflict(
                    "ambiguous reviewed entity identity".into(),
                ));
            }
            found = Some(serde_json::from_str::<Value>(&row)?);
        }
    }
    found.ok_or_else(|| {
        Error::SourceConflict("reviewed entity is not a current source projection".into())
    })
}
fn entity_signatures(
    conn: &Connection,
    payload: &Value,
) -> Result<std::collections::BTreeSet<String>> {
    let reference = reference(payload)
        .ok_or_else(|| Error::SourceConflict("reviewed entity has no source provenance".into()))?;
    let source = source_for_ref(conn, reference)?;
    ensure_source_value(conn, &source, payload)?;
    Ok(awr_core::ContentAssessment::derived_value(payload)?
        .findings
        .into_iter()
        .map(|f| f.signature)
        .collect())
}
fn ensure_signatures(value: &Value, signatures: &std::collections::BTreeSet<String>) -> Result<()> {
    awr_core::ensure_no_credentials_value(value)?;
    if awr_core::ContentAssessment::derived_value(value)?
        .findings
        .iter()
        .any(|f| !f.reviewable || !signatures.contains(&f.signature))
    {
        return Err(Error::RuleViolation(
            "output is not covered by the selected reviewed entity fields".into(),
        ));
    }
    Ok(())
}
pub(crate) fn ensure_output(conn: &Connection, value: &Value) -> Result<()> {
    if awr_core::ensure_public_value(value).is_ok() {
        return Ok(());
    }
    // Work-detail responses place acceptance and source provenance beside the
    // brief. Reassemble only those exact entity fields before checking them;
    // all other envelope fields retain their own independent validation.
    if value.get("id").is_none()
        && value.get("meta").is_none()
        && value.get("source_ref").is_some_and(Value::is_object)
        && value.get("work").is_some_and(Value::is_object)
    {
        let mut work = value["work"].clone();
        if work
            .get("source_ref")
            .is_some_and(|r| r != &value["source_ref"])
        {
            return Err(Error::SourceConflict(
                "work detail provenance is inconsistent".into(),
            ));
        }
        work["source_ref"] = value["source_ref"].clone();
        if let Some(acceptance) = value.get("acceptance") {
            work["acceptance"] = acceptance.clone();
        }
        ensure_output(conn, &work)?;
        let mut envelope = value.clone();
        for field in ["work", "acceptance", "source_ref"] {
            envelope.as_object_mut().unwrap().remove(field);
        }
        return ensure_output(conn, &envelope);
    }
    if let Some(reference) = reference(value) {
        let meta = value.get("meta").unwrap_or(value);
        let id = meta.get("id").and_then(Value::as_str).ok_or_else(|| {
            Error::SourceConflict("reviewed output lacks an entity identity".into())
        })?;
        let revision = meta
            .get("revision")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                Error::SourceConflict("reviewed output lacks an entity revision".into())
            })?;
        let payload = entity_payload(
            conn,
            None,
            id,
            revision,
            value.get("kind").and_then(Value::as_str).filter(|k| {
                matches!(
                    *k,
                    "goal" | "plan" | "rule" | "work_item" | "decision" | "evidence"
                )
            }),
        )?;
        if crate::content_review::reference(&payload) != Some(reference) {
            return Err(Error::SourceConflict(
                "reviewed output provenance differs from its persisted entity".into(),
            ));
        }
        if let Some(meta) = value.get("meta").and_then(Value::as_object) {
            if meta
                .iter()
                .any(|(key, field)| payload.get(key) != Some(field))
            {
                return Err(Error::SourceConflict(
                    "reviewed output metadata differs from its persisted entity".into(),
                ));
            }
        }
        // Provenance on one entity cannot approve unrelated/runtime fields added
        // to the envelope. Suspect fields must match the actual selected row.
        for (key, field) in value.as_object().unwrap() {
            if key == "meta" || awr_core::ensure_public_value(field).is_ok() {
                continue;
            }
            let source_key = if key == "statement" {
                "decision"
            } else if key == "phase" {
                "milestone"
            } else if key == "summary" && value["kind"] == "rule" {
                "text"
            } else if key == "summary" && value["kind"] == "decision" {
                "decision"
            } else {
                key.as_str()
            };
            let stored = payload.get(source_key);
            let same_summary = key == "summary"
                && stored
                    .and_then(Value::as_str)
                    .zip(field.as_str())
                    .is_some_and(|(original, summary)| original.trim().starts_with(summary));
            if stored != Some(field) && !same_summary {
                return Err(Error::SourceConflict(
                    "reviewed output field differs from the selected persisted field".into(),
                ));
            }
        }
        return ensure_signatures(value, &entity_signatures(conn, &payload)?);
    }
    match value {
        Value::Array(items) => items.iter().try_for_each(|v| ensure_output(conn, v)),
        Value::Object(items) => items.iter().try_for_each(|(key, value)| {
            awr_core::ensure_public_text(key)?;
            // Structured field guards remain strict outside explicit source objects.
            if awr_core::sensitive_field_category(key, value).is_some() {
                return awr_core::ensure_public_value(&serde_json::json!({key:value}));
            }
            ensure_output(conn, value)
        }),
        _ => awr_core::ensure_public_value(value),
    }
}
impl Store {
    /// Fast-path eligibility still depends on the exact archived review, even
    /// when source bytes are unchanged (for example after a schema upgrade).
    pub fn source_review_matches(
        &self,
        source: &Source,
        review: Option<&VerifiedSourceContentReview>,
    ) -> Result<bool> {
        let current = source_review(&self.conn, source);
        match (current, review) {
            (Ok(None), None) => Ok(true),
            (Ok(Some(current)), Some(review)) => Ok(current == *review.receipt()),
            (Err(Error::SourceConflict(_)), _) => Ok(false),
            (Err(error), _) => Err(error),
            _ => Ok(false),
        }
    }

    /// The caller supplies typed output assembled by AWR, never user-supplied proof.
    pub fn ensure_source_output<T: serde::Serialize>(&self, value: &T) -> Result<()> {
        ensure_output(&self.conn, &serde_json::to_value(value)?)
    }
    /// Coverage is restricted to exact current source entities selected for one chunk.
    pub fn ensure_entity_text(
        &self,
        project: awr_core::Id,
        entities: &[(String, awr_core::Id, u64)],
        text: &str,
    ) -> Result<()> {
        ensure_entity_text(&self.conn, project, entities, text)
    }
}

impl crate::WorkstreamRead {
    pub fn ensure_source_output<T: serde::Serialize>(&self, value: &T) -> Result<()> {
        self.store.ensure_source_output(value)
    }
}

pub(crate) fn ensure_entity_text(
    conn: &Connection,
    project: awr_core::Id,
    entities: &[(String, awr_core::Id, u64)],
    text: &str,
) -> Result<()> {
    if awr_core::ensure_public_text(text).is_ok() {
        return Ok(());
    }
    let mut signatures = std::collections::BTreeSet::new();
    for (kind, id, revision) in entities {
        if !matches!(
            kind.as_str(),
            "goal" | "plan" | "rule" | "work_item" | "decision" | "evidence"
        ) {
            continue;
        }
        if let Ok(payload) =
            entity_payload(conn, Some(project), &id.to_string(), *revision, Some(kind))
        {
            signatures.extend(entity_signatures(conn, &payload)?);
        }
    }
    ensure_signatures(&Value::String(text.into()), &signatures)
}
