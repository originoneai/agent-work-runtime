use awr_context::*;
use awr_core::{Error, Result};
use awr_source::{Manifest, index_project};
use awr_store::Store;
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !(3..=4).contains(&args.len()) {
        return Err(Error::InvalidInput(
            "usage: budget_project <root> <work-key> <budget> [agent-id]".into(),
        ));
    }
    let root = std::path::Path::new(&args[0]).canonicalize()?;
    let database = root.join(".awr/state.db");
    if !database.is_file() {
        return Err(Error::NotFound("initialized AWR database".into()));
    }
    let mut store = Store::open(&database)?;
    let report = index_project(&mut store, &root, &Manifest::load(&root)?, false)?;
    if !report.ok {
        return Err(Error::SourceStale("source refresh incomplete".into()));
    }
    let project = store.project(report.project_id)?;
    let input = RuleScopeInput {
        agent_id: args.get(3).cloned(),
        ..Default::default()
    };
    let hard = hard_context(
        &store,
        project.id,
        &args[1],
        project.current_branch_id,
        &input,
    )?;
    let work = store.work_item(project.id, &args[1])?;
    let rules = select_rules(&store, &project, Some(&work), &input)?;
    let optional = rules
        .soft
        .into_iter()
        .chain(rules.info)
        .map(|rule| RankedChunk {
            priority: if rule.item.severity == Some(awr_core::Severity::Soft) {
                100
            } else {
                200
            },
            recency: rule.item.meta.revision,
            chunk: ContextChunk {
                key: rule.item.meta.external_key,
                section: ContextSection::Rules,
                text: rule.item.text,
                entities: vec![SelectedEntity {
                    kind: "rule".into(),
                    id: rule.item.meta.id,
                    revision: rule.item.meta.revision,
                }],
            },
        })
        .collect::<Vec<_>>();
    let identity = ContextIdentity {
        project_id: project.id,
        project_key: project.external_key,
        project_revision: project.project_revision,
        work_item_id: hard.work.meta.id,
        work_item_key: args[1].clone(),
        work_item_revision: hard.work.meta.revision,
        branch_id: project.current_branch_id,
        source_versions: hard.source_revisions.clone(),
    };
    let pack = budget_context(
        &identity,
        &serde_json::to_value(input)?,
        &hard_chunks(&hard)?,
        &optional,
        args[2]
            .parse()
            .map_err(|_| Error::InvalidInput("budget must be an integer".into()))?,
    )?;
    let actual = store.project(project.id)?.project_revision;
    if actual != project.project_revision {
        return Err(Error::RevisionConflict {
            expected: project.project_revision,
            actual,
        });
    }
    println!("{}", serde_json::to_string_pretty(&pack)?);
    Ok(())
}
