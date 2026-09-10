use awr_core::*;
use awr_store::{EventCursor, EventQuery};
use clap::Args;
use serde_json::json;
use std::path::Path;

#[derive(Debug, Args)]
pub struct ChangesArgs {
    #[arg(long, default_value_t = 0, conflicts_with = "cursor")]
    after_revision: Revision,
    #[arg(long)]
    through_revision: Option<Revision>,
    /// Existing event next_cursor JSON; repeat the returned through_revision.
    #[arg(long, requires = "through_revision")]
    cursor: Option<String>,
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

pub fn run(root: &Path, args: &ChangesArgs, _json_output: bool) -> Result<()> {
    if !(1..=100).contains(&args.limit) {
        return Err(Error::InvalidInput(
            "source change limit must be 1..100".into(),
        ));
    }
    let cursor: Option<EventCursor> = args
        .cursor
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| Error::InvalidInput("invalid source event cursor JSON".into()))?;
    let (store, project) = crate::execution::read_state(root)?;
    let through = args.through_revision.unwrap_or(project.project_revision);
    let page = store.query_source_events(
        project.id,
        &EventQuery {
            after_revision: args.after_revision,
            through_revision: Some(through),
            cursor,
            limit: args.limit,
            ..Default::default()
        },
    )?;
    let mut unsupported = 0;
    let changes=page.events.iter().map(|event| {
        let supported=event.payload["change_schema"]==1 && event.payload["changes"].is_array() && event.payload["after"].is_object();
        if !supported {unsupported+=1;}
        let before=&event.payload["before"];let after=&event.payload["after"];
        let count=event.payload["changes"].as_array().map(Vec::len);
        let content_changed=supported && before["fingerprint"]!=after["fingerprint"] && after["fingerprint"].as_str().is_some_and(|s|!s.is_empty());
        let membership_changed=matches!(event.event_type.as_str(),"source.registered"|"source.retired");
        let configuration_changed=event.event_type=="source.configured";
        let freshness_changed=before["freshness"]!=after["freshness"];
        json!({"event_id":event.id,"event_type":event.event_type,"project_revision":event.project_revision,"created_at":event.created_at,"source_id":event.payload["source_id"],"change_schema":event.payload["change_schema"],"change_details_available":supported,
            "before":before,"after":after,"content_changed":if supported {Some(content_changed)}else{None},"membership_changed":membership_changed,"configuration_changed":configuration_changed,"freshness_changed":freshness_changed,
            "projection_change_count":count,"observation_only":supported && !content_changed && !membership_changed && !configuration_changed && count==Some(0),
            "changes_included":false,"detail_command":["event","show",&event.id.to_string(),"--full","--max-bytes","16777216"]})
    }).collect::<Vec<_>>();
    let pending = store
        .sources(project.id)?
        .into_iter()
        .filter(|s| s.freshness != Freshness::Fresh)
        .collect::<Vec<_>>();
    let final_revision = store.project(project.id)?.project_revision;
    if final_revision != project.project_revision {
        return Err(Error::RevisionConflict {
            expected: project.project_revision,
            actual: final_revision,
        });
    }
    let complete = pending.is_empty() && unsupported == 0;
    let can_finish = complete && page.next_cursor.is_none();
    let value = json!({"ok":complete,"project_id":project.id,"project_revision":project.project_revision,"after_revision":args.after_revision,"through_revision":through,"changes":changes,"has_more":page.next_cursor.is_some(),"next_cursor":page.next_cursor,
        "pending_source_total":pending.len(),"pending_sources":pending.iter().take(50).map(|s|json!({"id":s.id,"domain":s.domain,"locator":s.locator,"revision":s.revision,"freshness":s.freshness})).collect::<Vec<_>>(),"pending_sources_omitted":pending.len().saturating_sub(50),"pending_basis":"current_retained_source_state",
        "unsupported_event_count":unsupported,"next_after_revision_when_processed":if can_finish {Some(through)} else {None},"consumer_checkpoint_updated":false,"read_only":true,"source_refresh_performed":false,"side_effects_performed":false});
    println!("{}", serde_json::to_string_pretty(&value)?);
    if !complete {
        return Err(Error::SourceStale("source changes include unindexed/unavailable sources or unsupported historical receipts; process the full window and resolve source issues before acknowledging it".into()));
    }
    Ok(())
}
