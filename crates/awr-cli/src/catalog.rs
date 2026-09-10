use awr_core::*;
use awr_store::{CatalogCursor, CatalogKind, CatalogScope};
use clap::Args;
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(value_parser=["goal","plan","rule","work","decision","source","relation","artifact","evidence"])]
    kind: String,
    #[arg(long, default_value="active", value_parser=["active","retired","all"])]
    scope: String,
    /// JSON next_cursor from the preceding page. Changes require a fresh traversal.
    #[arg(long)]
    cursor: Option<String>,
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

pub fn list(root: &Path, args: &ListArgs, json_output: bool) -> Result<()> {
    let kind: CatalogKind = serde_json::from_value(json!(args.kind))?;
    let scope: CatalogScope = serde_json::from_value(json!(args.scope))?;
    let cursor: Option<CatalogCursor> = args
        .cursor
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| Error::InvalidInput("invalid catalog cursor JSON".into()))?;
    if !(1..=200).contains(&args.limit) {
        return Err(Error::InvalidInput("catalog limit must be 1..200".into()));
    }
    let query = crate::query::QueryProject::open(root)?;
    let page = query.store.catalog_page(
        query.project.id,
        query.project.project_revision,
        kind,
        scope,
        cursor.as_ref(),
        args.limit,
    )?;
    let mut value = query.metadata();
    let items = page
        .items
        .iter()
        .map(|row| {
            let mut item = serde_json::Map::new();
            for field in [
                "id",
                "external_key",
                "title",
                "revision",
                "source_ref",
                "status",
                "raw_status",
                "owner",
                "priority",
                "milestone",
                "kind",
                "severity",
                "unresolved",
                "domain",
                "role",
                "locator",
                "format",
                "adapter",
                "fingerprint",
                "freshness",
                "from_kind",
                "from_key",
                "relation",
                "to_kind",
                "to_key",
                "required",
                "artifact_type",
                "sha256",
                "size",
                "mime",
                "source_event_id",
                "evidence_type",
                "level",
                "source_sha",
                "work_item_id",
                "branch_id",
                "verified_at",
            ] {
                if let Some(v) = row.item.get(field) {
                    item.insert(field.into(), v.clone());
                }
            }
            for field in [
                "summary",
                "text",
                "decision",
                "rationale",
                "next_action",
                "blocker",
            ] {
                if let Some(v) = row.item.get(field).and_then(Value::as_str) {
                    item.insert(field.into(), json!(crate::query::short(v)));
                }
            }
            item.insert("active".into(), json!(row.active));
            item.insert("source".into(), json!(row.source));
            item.insert("project_revision".into(), json!(page.project_revision));
            item.insert("content_included".into(), json!(false));
            if let Some(source) = &row.source {
                item.insert("source_revision".into(), json!(source.revision));
                item.insert("freshness".into(), json!(source.freshness));
            }
            Value::Object(item)
        })
        .collect::<Vec<_>>();
    value["project_id"] = json!(page.project_id);
    value["kind"] = json!(page.kind);
    value["scope"] = json!(page.scope);
    value["total"] = json!(page.total);
    value["active_total"] = json!(page.active_total);
    value["retired_total"] = json!(page.retired_total);
    value["total_basis"] = json!("retained_indexed_objects");
    value["total_is_current"] = json!(query.refresh.ok);
    value["items"] = json!(items);
    value["has_more"] = json!(page.has_more);
    value["next_cursor"] = json!(page.next_cursor);
    value["content_included"] = json!(false);
    query.check_revision()?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "{} {} objects ({} returned); current={}\n{}",
            args.scope,
            args.kind,
            items.len(),
            query.refresh.ok,
            serde_json::to_string_pretty(&value)?
        );
    }
    query.finish()
}
