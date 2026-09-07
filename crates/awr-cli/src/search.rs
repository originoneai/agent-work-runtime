use crate::query::QueryProject;
use awr_core::Result;
use awr_store::SearchQuery;
use clap::Args;
use serde_json::json;
use std::path::Path;

#[derive(Debug, Args)]
pub struct SearchArgs {
    text: Option<String>,
    #[arg(long="type",value_parser=["goal","plan","rule","work_item","work","decision","evidence","event"])]
    kind: Option<String>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long = "work")]
    work_item_key: Option<String>,
    #[arg(long, default_value_t = 10)]
    limit: usize,
}
pub fn run(root: &Path, args: &SearchArgs, json_output: bool) -> Result<()> {
    let mut project = QueryProject::open(root)?;
    let kind = args.kind.as_ref().map(|s| {
        if s == "work" {
            "work_item".into()
        } else {
            s.clone()
        }
    });
    let query = SearchQuery {
        text: args.text.clone(),
        kind,
        status: args.status.clone(),
        work_item_key: args.work_item_key.clone(),
        limit: args.limit,
    };
    let report = project.store.search(project.project.id, &query)?;
    let mut value = project.metadata();
    value["hits"] = json!(report.hits);
    value["truncated"] = json!(report.truncated);
    value["index_policy_version"] = json!(report.index_policy_version);
    value["query"] = json!(query);
    project.check_revision()?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "Results: {}; revision: {}",
            report.hits.len(),
            report.project_revision
        );
        for hit in &report.hits {
            println!(
                "{} {} (rank {:.3e})\n  {}",
                hit.kind, hit.external_key, hit.rank, hit.summary
            );
            if let Some(source) = &hit.source_ref {
                println!(
                    "  Source: {} r{} ({:?})",
                    source.locator,
                    source.source_revision,
                    hit.source_freshness.unwrap()
                );
            } else {
                println!("  Source: runtime {}", hit.id);
            }
        }
        if report.truncated {
            println!("More matches exist; narrow the query or increase --limit.");
        }
    }
    project.finish()
}
