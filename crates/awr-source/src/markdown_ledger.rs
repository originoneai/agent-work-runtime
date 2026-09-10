//! Source-declared Markdown records; runtime evidence is never inferred from prose.
use crate::{Locator, Manifest, ParseContext, SourceAdapter, SourceSnapshot, SourceSpec};
use awr_core::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
pub struct MarkdownLedgerAdapter;
pub(crate) fn column(s: &str) -> &str {
    match s.trim() {
        "ID" | "编号" | "任务编号" | "任务ID" => "id",
        "任务" | "功能" | "工作" | "工作项" | "标题" | "名称" => "title",
        "状态" | "进度" => "status",
        "负责人" | "执行者" | "Owner 角色" => "owner",
        "优先级" => "priority",
        "依赖" => "depends_on",
        "下一步" | "后续动作" | "当前证据 / 下一动作" => "next_action",
        "验收" | "验收标准" | "完成硬门槛" => "acceptance",
        "目标" | "关联目标" | "goals" => "goal",
        other => other,
    }
}

pub(crate) fn normalized(raw: &str, mapping: &crate::LedgerMapping) -> WorkStatus {
    if mapping.status(raw) != WorkStatus::Unknown {
        return mapping.status(raw);
    }
    WorkStatus::normalize(match raw.trim() {
        "待开始" | "未开始" | "计划中" | "[ ]" => "planned",
        "就绪" => "ready",
        "进行中" | "开发中" => "in_progress",
        "已完成" | "完成" | "[x]" | "[X]" => "completed",
        "阻塞" | "已阻塞" => "blocked",
        "已取消" => "cancelled",
        other => other,
    })
}

impl SourceAdapter for MarkdownLedgerAdapter {
    fn name(&self) -> &'static str {
        "markdown-ledger-v1"
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
        if spec.domain != "ledger" {
            return Err(Error::InvalidInput(
                "Markdown ledger requires domain ledger".into(),
            ));
        }
        ensure_public_text(snapshot.text()?)?;
        let mapping = crate::LedgerMapping::from_spec(spec)?;
        let rows = crate::markdown_records::rows(snapshot.text()?, spec)?;
        let mut records = vec![];
        let mut refs = BTreeMap::new();
        let mut raw_statuses = BTreeMap::new();
        for row in &rows {
            let key = crate::markdown_records::key(row)?;
            if refs.contains_key(&key) {
                return Err(Error::SourceConflict(
                    "duplicate Markdown task ID/title; add explicit unique IDs".into(),
                ));
            }
            let mut value = Value::Object(row.values.clone());
            let explicit = row
                .values
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty());
            value["id"] = json!(key);
            let raw = value["status"].as_str().unwrap_or("").to_owned();
            let status = normalized(&raw, &mapping);
            value["status"] = if status == WorkStatus::Unknown {
                json!(raw)
            } else {
                json!(status)
            };
            raw_statuses.insert(key.clone(), raw);
            for field in [
                "acceptance",
                "depends_on",
                "dependencies",
                "goals",
                "tags",
                "paths",
                "deliverables",
            ] {
                if let Some(v) = value.get(field).and_then(Value::as_str) {
                    let list: Vec<String> = if field == "acceptance" {
                        if v.is_empty() { vec![] } else { vec![v.into()] }
                    } else {
                        v.split([',', '，', ';', '；'])
                            .map(str::trim)
                            .filter(|s| !s.is_empty() && *s != "—" && *s != "-")
                            .map(str::to_owned)
                            .collect()
                    };
                    value[field] = json!(list);
                }
            }
            if value.get("goal").and_then(Value::as_str).is_some() {
                let goals: Vec<_> = value["goal"]
                    .as_str()
                    .unwrap()
                    .split([',', '，', ';', '；'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && *s != "—" && *s != "-")
                    .map(str::to_owned)
                    .collect();
                value.as_object_mut().unwrap().remove("goal");
                value["goals"] = json!(goals);
            }
            for field in [
                "owner",
                "priority",
                "milestone",
                "kind",
                "blocker",
                "score",
                "required",
                "required_for_v1",
                "ordinary_completion",
                "verification",
                "evidence_level",
            ] {
                if value.get(field) == Some(&json!("")) {
                    value.as_object_mut().unwrap().remove(field);
                }
            }
            if value.get("summary").is_none() {
                value["summary"] = json!(
                    "Imported from source-declared Markdown; completion evidence has not been inferred."
                );
            }
            refs.insert(
                key.clone(),
                context.meta(
                    EntityKind::WorkItem,
                    &key,
                    snapshot,
                    explicit.then(|| crate::markdown_records::pointer(&key)),
                    Some((row.line, row.line)),
                )?,
            );
            records.push(value);
        }
        // Reuse the domain decoder in memory, then restore actual Markdown provenance.
        let bytes = serde_json::to_vec(&json!({"work_items":records}))?;
        let synthetic = SourceSnapshot {
            locator: snapshot.locator.clone(),
            fingerprint: snapshot.fingerprint.clone(),
            bytes,
        };
        let mut canonical = spec.clone();
        canonical.adapter = "yaml-ledger-v1".into();
        canonical.options.clear();
        let mut ids = context.existing_ids.clone();
        for (key, meta) in &refs {
            ids.insert((EntityKind::WorkItem, key.clone()), meta.id);
        }
        let mut batch = crate::YamlLedgerAdapter.parse(
            &synthetic,
            &ParseContext {
                source: context.source,
                existing_ids: ids,
            },
            &canonical,
        )?;
        for work in &mut batch.work_items {
            work.meta = refs[&work.meta.external_key].clone();
            work.raw_status = raw_statuses[&work.meta.external_key].clone();
        }
        for edge in &mut batch.edges {
            if let Some(meta) = refs.get(&edge.from_key) {
                edge.source_ref = meta.source_ref.clone();
            }
        }
        for evidence in &mut batch.evidence {
            if let Some(meta) = refs.values().find(|m| Some(m.id) == evidence.work_item_id) {
                evidence.source_ref = Some(meta.source_ref.clone());
            }
        }
        Ok(batch)
    }
}
