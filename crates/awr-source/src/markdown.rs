use crate::{Locator, Manifest, ParseContext, SourceAdapter, SourceSnapshot, SourceSpec};
use awr_core::{
    EntityKind, Error, Goal, Plan, ProjectionBatch, Result, Rule, Scope, ScopeKind, Severity,
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, Serialize)]
pub struct MarkdownSection {
    pub anchor: String,
    pub title: String,
    pub level: u8,
    pub start_line: usize,
    pub end_line: usize,
    pub fingerprint: String,
    pub body: String,
    pub attributes: BTreeMap<String, String>,
}

struct Heading {
    id: Option<String>,
    title: String,
    level: u8,
    start: usize,
    body_start: usize,
    attributes: BTreeMap<String, String>,
}

fn slug(title: &str) -> String {
    let mut result = String::new();
    for c in title.to_lowercase().chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            result.push(c);
        } else if c.is_whitespace() && !result.ends_with('-') {
            result.push('-');
        }
    }
    let result = result.trim_matches('-');
    if result.is_empty() {
        "section".into()
    } else {
        result.into()
    }
}
fn line(text: &str, offset: usize) -> usize {
    1 + text.as_bytes()[..offset]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
}

/// Disjoint source sections. Code blocks and quoted/list-contained headings do not split authority.
pub fn markdown_sections(snapshot: &SourceSnapshot) -> Result<Vec<MarkdownSection>> {
    let text = snapshot.text()?;
    if text.trim().is_empty() {
        return Ok(vec![]);
    }
    let mut headings = Vec::<Heading>::new();
    let mut pending = None;
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(
        text,
        Options::ENABLE_HEADING_ATTRIBUTES | Options::ENABLE_TABLES,
    )
    .into_offset_iter()
    {
        match event {
            Event::Start(Tag::Heading {
                level, id, attrs, ..
            }) if depth == 0 => {
                let mut attributes = BTreeMap::new();
                for (key, value) in attrs {
                    if attributes
                        .insert(
                            key.to_string(),
                            value.map(|v| v.to_string()).unwrap_or_default(),
                        )
                        .is_some()
                    {
                        return Err(Error::SourceConflict(
                            "duplicate Markdown heading attribute".into(),
                        ));
                    }
                }
                pending = Some(Heading {
                    id: id.map(|id| id.to_string()),
                    title: String::new(),
                    level: level as u8,
                    start: range.start,
                    body_start: range.end,
                    attributes,
                });
                depth += 1;
            }
            Event::Start(_) => depth += 1,
            Event::End(TagEnd::Heading(_)) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(heading) = pending.take() {
                        headings.push(heading);
                    }
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Text(value) | Event::Code(value) => {
                if let Some(heading) = pending.as_mut() {
                    heading.title.push_str(&value);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(heading) = pending.as_mut() {
                    heading.title.push(' ');
                }
            }
            _ => {}
        }
    }
    if headings
        .first()
        .is_none_or(|h| !text[..h.start].trim().is_empty())
    {
        headings.insert(
            0,
            Heading {
                id: Some("preamble".into()),
                title: "Introduction".into(),
                level: 0,
                start: 0,
                body_start: 0,
                attributes: BTreeMap::new(),
            },
        );
    }
    let mut used = BTreeSet::new();
    let mut sections = vec![];
    for (index, heading) in headings.iter().enumerate() {
        let base = heading.id.clone().unwrap_or_else(|| slug(&heading.title));
        let mut anchor = base.clone();
        let mut occurrence = 1;
        while used.contains(&anchor) {
            if heading.id.is_some() {
                return Err(Error::SourceConflict(format!(
                    "duplicate explicit Markdown anchor {anchor}"
                )));
            }
            occurrence += 1;
            anchor = format!("{base}-{occurrence}");
        }
        used.insert(anchor.clone());
        let end = headings
            .get(index + 1)
            .map_or(text.len(), |next| next.start);
        let start_line = line(text, heading.start);
        let end_line = line(text, end.saturating_sub(1));
        sections.push(MarkdownSection {
            anchor,
            title: heading.title.clone(),
            level: heading.level,
            start_line,
            end_line,
            fingerprint: snapshot.section_fingerprint(start_line, end_line)?,
            body: text[heading.body_start..end].trim().into(),
            attributes: heading.attributes.clone(),
        });
    }
    Ok(sections)
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownOptions {
    key_prefix: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    severity: Option<String>,
    scope: Option<String>,
    value: Option<String>,
}
impl MarkdownOptions {
    fn load(spec: &SourceSpec) -> Result<Self> {
        spec.options
            .clone()
            .try_into()
            .map_err(|e| Error::InvalidInput(format!("Markdown options: {e}")))
    }
}
fn setting(section: &MarkdownSection, key: &str, default: &Option<String>) -> Option<String> {
    section
        .attributes
        .get(key)
        .cloned()
        .or_else(|| default.clone())
}
fn meta(
    context: &ParseContext<'_>,
    snapshot: &SourceSnapshot,
    kind: EntityKind,
    section: &MarkdownSection,
    options: &MarkdownOptions,
) -> Result<awr_core::ProjectionMeta> {
    let prefix = options
        .key_prefix
        .as_deref()
        .unwrap_or(&context.source.locator);
    let mut meta = context.meta(
        kind,
        &format!("{prefix}#{}", section.anchor),
        snapshot,
        Some(format!("#{}", section.anchor)),
        Some((section.start_line, section.end_line)),
    )?;
    meta.source_ref.section_fingerprint = Some(section.fingerprint.clone());
    Ok(meta)
}

pub struct MarkdownHeadingAdapter;
impl SourceAdapter for MarkdownHeadingAdapter {
    fn name(&self) -> &'static str {
        "markdown-heading-v1"
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
        if !["goal", "plan"].contains(&spec.domain.as_str()) {
            return Err(Error::InvalidInput(
                "heading adapter requires goal or plan domain".into(),
            ));
        }
        let options = MarkdownOptions::load(spec)?;
        let mut batch = ProjectionBatch::default();
        for section in markdown_sections(snapshot)? {
            let status =
                setting(&section, "status", &options.status).unwrap_or_else(|| "unknown".into());
            if spec.domain == "goal" {
                batch.goals.push(Goal {
                    meta: meta(context, snapshot, EntityKind::Goal, &section, &options)?,
                    title: section.title,
                    status,
                    priority: section
                        .attributes
                        .get("priority")
                        .cloned()
                        .or_else(|| options.priority.clone()),
                    summary: section.body,
                    success_criteria: vec![],
                });
            } else {
                batch.plans.push(Plan {
                    meta: meta(context, snapshot, EntityKind::Plan, &section, &options)?,
                    title: section.title,
                    status,
                    kind: Some("markdown_section".into()),
                    summary: section.body,
                    scope: vec![],
                    acceptance: vec![],
                });
            }
        }
        Ok(batch)
    }
}

pub struct MarkdownRulesAdapter;
impl SourceAdapter for MarkdownRulesAdapter {
    fn name(&self) -> &'static str {
        "markdown-rules-v1"
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
        if spec.domain != "rules" {
            return Err(Error::InvalidInput(
                "rules adapter requires rules domain".into(),
            ));
        }
        let options = MarkdownOptions::load(spec)?;
        let mut batch = ProjectionBatch::default();
        for section in markdown_sections(snapshot)? {
            let mut unresolved = vec![];
            let raw_severity = setting(&section, "severity", &options.severity);
            let severity = raw_severity.as_ref().and_then(|s| {
                serde_json::from_value::<Severity>(serde_json::Value::String(s.clone())).ok()
            });
            if severity.is_none() {
                unresolved.push(format!(
                    "severity unresolved: {}",
                    raw_severity.as_deref().unwrap_or("not specified")
                ));
            }
            let raw_scope = setting(&section, "scope", &options.scope);
            let kind = raw_scope.as_ref().and_then(|s| {
                serde_json::from_value::<ScopeKind>(serde_json::Value::String(s.clone())).ok()
            });
            let value = setting(&section, "value", &options.value).filter(|s| !s.trim().is_empty());
            let scope = match (kind, value) {
                (Some(kind), Some(value)) => Some(Scope { kind, value }),
                _ => {
                    unresolved.push(format!(
                        "scope unresolved: type={}, explicit value required",
                        raw_scope.as_deref().unwrap_or("not specified")
                    ));
                    None
                }
            };
            batch.warnings.extend(
                unresolved
                    .iter()
                    .map(|warning| format!("#{}: {warning}", section.anchor)),
            );
            batch.rules.push(Rule {
                meta: meta(context, snapshot, EntityKind::Rule, &section, &options)?,
                text: format!("{}\n\n{}", section.title, section.body),
                severity,
                scope,
                unresolved,
            });
        }
        Ok(batch)
    }
}
