//! Read-only intake of existing Markdown task tables and checklists.
//! Source-declared completion is retained; no evidence or execution is inferred.
use crate::{Locator, Manifest, ParseContext, SourceAdapter, SourceSnapshot, SourceSpec};
use awr_core::{Edge, EntityKind, Error, ProjectionBatch, Result, WorkItem, WorkStatus};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub struct MarkdownLedgerAdapter;

fn column(s: &str) -> &str {
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

fn normalized(raw: &str, mapping: &crate::LedgerMapping) -> WorkStatus {
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
        let mapping = crate::LedgerMapping::from_spec(spec)?;
        let text = snapshot.text()?;
        awr_core::ensure_public_text(text)?;
        let mut rows: Vec<(usize, BTreeMap<String, String>)> = vec![];
        let mut headers = Vec::<String>::new();
        let mut cells = Vec::<String>::new();
        let mut cell = String::new();
        let mut in_cell = false;
        let mut row_line = 0;
        for (event, range) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
            match event {
                Event::Start(Tag::Table(_)) => headers.clear(),
                Event::Start(Tag::TableHead | Tag::TableRow) => {
                    cells.clear();
                    row_line = text[..range.start].bytes().filter(|b| *b == b'\n').count() + 1;
                }
                Event::Start(Tag::TableCell) => {
                    cell.clear();
                    in_cell = true;
                }
                Event::Text(v) | Event::Code(v) if in_cell => cell.push_str(&v),
                Event::SoftBreak | Event::HardBreak if in_cell => cell.push(' '),
                Event::End(TagEnd::TableCell) => {
                    cells.push(cell.trim().to_owned());
                    in_cell = false;
                }
                Event::End(TagEnd::TableHead) => {
                    headers = cells
                        .iter()
                        .map(|s| {
                            mapping
                                .column(s)
                                .unwrap_or_else(|| column(s))
                                .to_lowercase()
                        })
                        .collect();
                    if headers.iter().collect::<BTreeSet<_>>().len() != headers.len() {
                        return Err(Error::SourceConflict(
                            "duplicate Markdown ledger columns".into(),
                        ));
                    }
                }
                Event::End(TagEnd::TableRow) if headers.iter().any(|v| v == "title") => {
                    rows.push((
                        row_line,
                        headers.iter().cloned().zip(cells.clone()).collect(),
                    ));
                }
                _ => {}
            }
        }
        // Fenced examples are not project tasks. The parser's task-list markers identify real lists.
        let mut task: Option<(usize, bool, String)> = None;
        for (event, range) in Parser::new_ext(text, Options::ENABLE_TASKLISTS).into_offset_iter() {
            match event {
                Event::TaskListMarker(done) => {
                    if task.is_some() {
                        return Err(Error::Unsupported(
                            "nested Markdown task lists require an explicit YAML mapping".into(),
                        ));
                    }
                    task = Some((
                        text[..range.start].bytes().filter(|b| *b == b'\n').count() + 1,
                        done,
                        String::new(),
                    ));
                }
                Event::Text(v) | Event::Code(v) => {
                    if let Some((_, _, title)) = &mut task {
                        title.push_str(&v);
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if let Some((_, _, title)) = &mut task {
                        title.push(' ');
                    }
                }
                Event::End(TagEnd::Item) => {
                    if let Some((line, done, title)) = task.take() {
                        rows.push((
                            line,
                            BTreeMap::from([
                                ("title".into(), title),
                                ("status".into(), if done { "[x]" } else { "[ ]" }.into()),
                            ]),
                        ));
                    }
                }
                _ => {}
            }
        }
        if rows.is_empty() {
            return Err(Error::InvalidInput(
                "no supported task table or checklist in Markdown ledger".into(),
            ));
        }
        let mut batch = ProjectionBatch::default();
        let mut keys = BTreeSet::new();
        for (line, row) in rows {
            let get = |key: &str| row.get(key).map(|s| s.trim()).unwrap_or("");
            let title = get("title");
            if title.is_empty() {
                return Err(Error::InvalidInput("Markdown task title is empty".into()));
            }
            let key = if get("id").is_empty() {
                format!("md-{:x}", Sha256::digest(title.as_bytes()))
            } else {
                get("id").to_owned()
            };
            if !keys.insert(key.clone()) {
                return Err(Error::SourceConflict(
                    "duplicate Markdown task ID/title; add explicit unique IDs".into(),
                ));
            }
            let status = normalized(get("status"), &mapping);
            if status == WorkStatus::Unknown {
                batch.warnings.push(format!(
                    "line {line}: unrecognized status; retained as unknown"
                ));
            }
            let meta = context.meta(
                EntityKind::WorkItem,
                &key,
                snapshot,
                None,
                Some((line, line)),
            )?;
            let source_ref = meta.source_ref.clone();
            for goal in get("goal")
                .split([',', '，', ';', '；'])
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != "—" && *s != "-")
            {
                batch.edges.push(Edge {
                    id: awr_core::Id::new(),
                    project_id: context.source.project_id,
                    from_kind: EntityKind::WorkItem,
                    from_key: key.clone(),
                    to_kind: EntityKind::Goal,
                    to_key: goal.into(),
                    relation: "supports".into(),
                    required: true,
                    revision: 1,
                    source_ref: source_ref.clone(),
                });
            }
            batch.work_items.push(WorkItem {ordinary_completion:None,meta,title:title.into(),kind:None,owner:(!get("owner").is_empty()).then(||get("owner").into()),required:false,
                raw_status:get("status").into(),status,priority:(!get("priority").is_empty()).then(||get("priority").into()),milestone:None,score:None,evidence_level:None,
                summary:"Imported from source-declared Markdown; completion evidence has not been inferred.".into(),next_action:get("next_action").into(),blocker:None,
                acceptance:(!get("acceptance").is_empty()).then(||vec![get("acceptance").into()]).unwrap_or_default(),tags:vec![],paths:vec![]});
            for dependency in get("depends_on")
                .split([',', '，', ';', '；'])
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != "—" && *s != "-")
            {
                batch.edges.push(Edge {
                    id: awr_core::Id::new(),
                    project_id: context.source.project_id,
                    from_kind: EntityKind::WorkItem,
                    from_key: key.clone(),
                    to_kind: EntityKind::WorkItem,
                    to_key: dependency.into(),
                    relation: "depends_on".into(),
                    required: true,
                    revision: 1,
                    source_ref: source_ref.clone(),
                });
            }
        }
        Ok(batch)
    }
}
