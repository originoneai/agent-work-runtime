//! Finite Markdown records. Parser-selected rows exclude fenced examples and prose.
use crate::{LedgerMapping, SourceSpec};
use awr_core::{Error, Result};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde_json::{Map, Value};
use std::{collections::BTreeSet, ops::Range};

pub(crate) struct Row {
    pub line: usize,
    pub range: Range<usize>,
    pub table: Option<Vec<String>>,
    pub values: Map<String, Value>,
    pub metadata: Vec<(String, Range<usize>)>,
}
pub(crate) fn pointer(key: &str) -> String {
    format!("/work_items/{}", key.replace('~', "~0").replace('/', "~1"))
}
fn bounds(text: &str, start: usize) -> (usize, Range<usize>) {
    let begin = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    (
        text[..begin].bytes().filter(|b| *b == b'\n').count() + 1,
        begin..end,
    )
}
pub(crate) fn cell_value(key: &str, s: &str) -> Value {
    if matches!(
        key,
        "acceptance"
            | "depends_on"
            | "dependencies"
            | "goals"
            | "tags"
            | "paths"
            | "deliverables"
            | "evidence"
            | "verification"
            | "ordinary_completion"
            | "archived"
            | "required"
            | "required_for_v1"
            | "score"
            | "blocker"
    ) {
        if let Ok(v) = serde_json::from_str(s) {
            return v;
        }
    }
    Value::String(s.trim().into())
}
pub(crate) fn rows(text: &str, spec: &SourceSpec) -> Result<Vec<Row>> {
    let html: BTreeSet<usize> =
        Parser::new_ext(text, Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS)
            .into_offset_iter()
            .filter_map(|(e, r)| {
                matches!(e, Event::InlineHtml(_) | Event::Html(_)).then_some(r.start)
            })
            .collect();
    let mapping = LedgerMapping::from_spec(spec)?;
    let mut rows = vec![];
    let mut headers = vec![];
    let mut cells = vec![];
    let mut cell = String::new();
    let mut in_cell = false;
    let mut row_start = 0;
    for (event, range) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
        match event {
            Event::Start(Tag::Table(_)) => headers.clear(),
            Event::Start(Tag::TableHead | Tag::TableRow) => {
                cells.clear();
                row_start = range.start;
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
                            .unwrap_or_else(|| crate::markdown_ledger::column(s))
                            .to_lowercase()
                    })
                    .collect::<Vec<_>>();
                if headers.iter().collect::<BTreeSet<_>>().len() != headers.len() {
                    return Err(Error::SourceConflict(
                        "duplicate Markdown ledger columns".into(),
                    ));
                }
            }
            Event::End(TagEnd::TableRow) if headers.iter().any(|s| s == "title") => {
                let (line, range) = bounds(text, row_start);
                rows.push(Row {
                    line,
                    range,
                    table: Some(headers.clone()),
                    values: headers
                        .iter()
                        .zip(&cells)
                        .map(|(k, v)| (k.clone(), cell_value(k, v)))
                        .collect(),
                    metadata: vec![],
                });
            }
            _ => (),
        }
    }
    let mut task: Option<(usize, bool, String)> = None;
    for (event, range) in Parser::new_ext(text, Options::ENABLE_TASKLISTS).into_offset_iter() {
        match event {
            Event::TaskListMarker(done) => {
                if task.is_some() {
                    return Err(Error::Unsupported(
                        "nested Markdown task lists require an explicit mapping".into(),
                    ));
                }
                task = Some((range.start, done, String::new()));
            }
            Event::Text(v) | Event::Code(v) => {
                if let Some((_, _, t)) = &mut task {
                    t.push_str(&v);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, _, t)) = &mut task {
                    t.push(' ');
                }
            }
            Event::End(TagEnd::Item) => {
                if let Some((start, done, title)) = task.take() {
                    let (line, range) = bounds(text, start);
                    rows.push(Row {
                        line,
                        range,
                        table: None,
                        values: Map::from_iter([
                            ("title".into(), Value::String(title.trim().into())),
                            (
                                "status".into(),
                                Value::String(if done { "[x]" } else { "[ ]" }.into()),
                            ),
                        ]),
                        metadata: vec![],
                    });
                }
            }
            _ => (),
        }
    }
    if rows.is_empty() {
        return Err(Error::InvalidInput(
            "no supported task table or checklist in Markdown ledger".into(),
        ));
    }
    for row in &mut rows {
        let raw = &text[row.range.clone()];
        let mut offset = 0;
        while let Some(i) = raw[offset..].find("<!-- awr:") {
            if !html.contains(&(row.range.start + offset + i)) {
                offset += i + 9;
                continue;
            }
            let start = offset + i + 9;
            let end = start
                + raw[start..]
                    .find("-->")
                    .ok_or_else(|| Error::InvalidInput("unclosed Markdown work metadata".into()))?;
            let field = &raw[start..end];
            let (key, value) = field.split_once('=').ok_or_else(|| {
                Error::InvalidInput("Markdown work metadata requires field=JSON".into())
            })?;
            let key = key.trim();
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(Error::InvalidInput(
                    "invalid Markdown work metadata field".into(),
                ));
            }
            let value_start = start + field.find('=').unwrap() + 1;
            let leading = value.len() - value.trim_start().len();
            let trailing = value.trim_end().len();
            let value: Value = serde_json::from_str(value.trim())
                .map_err(|_| Error::InvalidInput("invalid Markdown work metadata JSON".into()))?;
            if row.metadata.iter().any(|(k, _)| k == key) {
                return Err(Error::SourceConflict(
                    "duplicate Markdown work metadata".into(),
                ));
            }
            if let Some(old) = row.values.get(key) {
                if row.table.is_none() && key == "status" {
                    let done =
                        crate::markdown_ledger::normalized(value.as_str().unwrap_or(""), &mapping)
                            == awr_core::WorkStatus::Completed;
                    if done != (old == "[x]") {
                        return Err(Error::SourceConflict(
                            "checkbox and explicit status disagree".into(),
                        ));
                    }
                } else {
                    return Err(Error::SourceConflict(
                        "table/title and metadata declare the same field".into(),
                    ));
                }
            }
            row.values.insert(key.into(), value);
            row.metadata.push((
                key.into(),
                row.range.start + value_start + leading..row.range.start + value_start + trailing,
            ));
            offset = end + 3;
        }
    }
    rows.sort_by_key(|r| r.line);
    Ok(rows)
}

pub(crate) fn key(row: &Row) -> Result<String> {
    let title = row
        .values
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::InvalidInput("Markdown task title is empty".into()))?;
    Ok(
        if let Some(id) = row
            .values
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            id.into()
        } else {
            use sha2::{Digest, Sha256};
            format!("md-{:x}", Sha256::digest(title.as_bytes()))
        },
    )
}
pub(crate) fn table_cells(text: &str, range: Range<usize>) -> Result<Vec<Range<usize>>> {
    let raw = &text[range.clone()];
    let left = raw.len() - raw.trim_start().len();
    let right = raw.trim_end().len();
    if !raw[left..right].starts_with('|') || !raw[left..right].ends_with('|') {
        return Err(Error::MutationUnsupported(
            "table writes require explicit outer pipes and a single unindented row".into(),
        ));
    }
    if left != 0 {
        return Err(Error::MutationUnsupported(
            "nested table writes require manual editing".into(),
        ));
    }
    let mut pipes = vec![];
    let mut escaped = false;
    for (i, c) in raw[..right].char_indices() {
        if c == '|' && !escaped {
            pipes.push(i);
        }
        escaped = if c == '\\' { !escaped } else { false };
    }
    Ok(pipes
        .windows(2)
        .map(|p| range.start + p[0] + 1..range.start + p[1])
        .collect())
}
pub(crate) fn escaped(value: &Value) -> Result<String> {
    let input = if let Some(s) = value.as_str() {
        s.to_owned()
    } else {
        serde_json::to_string(value)?
    };
    if input.contains(['\n', '\r']) {
        return Err(Error::MutationUnsupported(
            "visible Markdown cells/titles require a single line".into(),
        ));
    }
    let mut out = String::new();
    for c in input.chars() {
        if "\\|`*_[]<>#!".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    Ok(out)
}
pub(crate) fn metadata_value(v: &Value) -> Result<String> {
    Ok(serde_json::to_string(v)?
        .replace('|', "\\u007c")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('-', "\\u002d"))
}
