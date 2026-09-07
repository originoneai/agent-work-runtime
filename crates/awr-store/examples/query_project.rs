use awr_core::{Applicability, RuleContext};
use awr_store::Store;
fn main() -> awr_core::Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            awr_core::Error::InvalidInput("usage: query_project <project root>".into())
        })?;
    let store = Store::open_readonly(&root.join(".awr/state.db"))?;
    let project = store.project_by_root(&root)?;
    let goals = store.goals(project.id)?;
    let plans = store.plans(project.id)?;
    let rules = store.rules_for(
        project.id,
        &RuleContext {
            project_key: Some(project.external_key.clone()),
            ..Default::default()
        },
    )?;
    // Round-trip every projected external ID through the public typed getters.
    for goal in &goals {
        assert_eq!(
            store
                .goal(project.id, &goal.item.meta.external_key)?
                .item
                .meta
                .id,
            goal.item.meta.id
        );
    }
    for plan in &plans {
        assert_eq!(
            store
                .plan(project.id, &plan.item.meta.external_key)?
                .item
                .meta
                .id,
            plan.item.meta.id
        );
    }
    for rule in &rules {
        assert_eq!(
            store
                .rule(project.id, &rule.rule.item.meta.external_key)?
                .item
                .meta
                .id,
            rule.rule.item.meta.id
        );
    }
    println!(
        "{}",
        serde_json::json!({"project_revision":project.project_revision,"goals":goals.len(),"plans":plans.len(),"rules":rules.len(),"rules_unknown":rules.iter().filter(|r|r.applicability==Applicability::Unknown).count(),"source_revisions":rules.iter().map(|r|r.rule.source.revision).collect::<Vec<_>>()})
    );
    Ok(())
}
