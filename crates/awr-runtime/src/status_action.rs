//! Daily work navigation, separate from the unchanged claim-admission query.
use crate::{
    OrganizationReport, StatusScope,
    status_summary::{brief, select_work, short},
};
use awr_core::*;
use awr_store::Store;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn action_status(
    store: &Store,
    project: &Project,
    scope: &StatusScope,
    works: &[Projected<WorkItem>],
    readiness: &ReadyReport,
    organization: &OrganizationReport,
) -> Result<Value> {
    action_status_page(store, project, scope, works, readiness, organization, None)
}

pub fn action_status_page(
    store: &Store,
    project: &Project,
    scope: &StatusScope,
    works: &[Projected<WorkItem>],
    readiness: &ReadyReport,
    organization: &OrganizationReport,
    page: Option<(&str, usize, usize)>,
) -> Result<Value> {
    let selected = select_work(store, project, scope, works)?;
    let by_key: BTreeMap<_, _> = readiness
        .ready
        .iter()
        .chain(&readiness.blocked)
        .map(|r| (r.work.item.meta.external_key.as_str(), r))
        .collect();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut current = vec![];
    let mut ready = vec![];
    let mut waiting = vec![];
    let mut blocked = vec![];
    let mut relevant: BTreeSet<&str> = scope.work.iter().map(String::as_str).collect();
    for w in &selected {
        *counts
            .entry(
                serde_json::to_value(w.item.status)?
                    .as_str()
                    .unwrap()
                    .into(),
            )
            .or_default() += 1;
        if w.item.archived || matches!(w.item.status, WorkStatus::Completed | WorkStatus::Cancelled)
        {
            continue;
        }
        let key = w.item.meta.external_key.as_str();
        relevant.insert(key);
        let Some(r) = by_key.get(key) else {
            blocked.push(json!({"key":key,"codes":["readiness_unavailable"]}));
            continue;
        };
        relevant.extend(
            r.dependencies
                .dependencies
                .iter()
                .map(|d| d.item.meta.external_key.as_str()),
        );
        let waits = store.pending_work_waits(project.id, w.item.meta.id, readiness.branch_id)?;
        let mut pending_executions = vec![];
        for e in store.executions(project.id, Some(w.item.meta.id))? {
            if e.branch_id != readiness.branch_id || e.state.terminal() {
                continue;
            }
            let report = store.latest_external_report(project.id, e.id)?;
            let phase = report
                .as_ref()
                .and_then(|r| r.payload["report"]["phase"].as_str());
            if matches!(phase, Some("succeeded" | "failed")) {
                continue;
            }
            pending_executions
                .push(json!({"id":e.id,"recorded_state":e.state,"reported_phase":phase}));
        }
        let mut item = brief(w);
        let codes: BTreeSet<_> = r
            .diagnostics
            .iter()
            .filter(|d| !matches!(d.code.as_str(), "active_claim" | "status_not_selectable"))
            .map(|d| d.code.as_str())
            .collect();
        item["codes"] = json!(codes);
        item["claims"] = json!(
            r.active_claims
                .iter()
                .map(|c| json!({"session":c.session_id,"agent":c.agent_id}))
                .collect::<Vec<_>>()
        );
        let admissible_structure = organization.executable_keys.contains(key);
        let dependency_wait = !codes.is_empty()
            && codes.iter().all(|c| *c == "dependency_not_completed")
            && organization.structured_keys.contains(key);
        if matches!(w.item.status, WorkStatus::Blocked | WorkStatus::Unknown)
            || (!admissible_structure && !dependency_wait)
        {
            let structural: Vec<_> = organization
                .gaps
                .iter()
                .filter(|g| g.target == key)
                .take(5)
                .map(|g| g.code.as_str())
                .collect();
            item["structural_codes"] = json!(structural);
            blocked.push(item);
        } else if dependency_wait || !waits.is_empty() || !pending_executions.is_empty() {
            item["wait_ids"] = json!(waits.iter().take(5).map(|w| w.id).collect::<Vec<_>>());
            item["wait_total"] = json!(waits.len());
            item["execution_total"] = json!(pending_executions.len());
            item["executions"] = json!(pending_executions.into_iter().take(5).collect::<Vec<_>>());
            item["execution_basis"] =
                json!("recorded registry and latest host report; not a live process probe");
            item["next_action"] = json!(if !waits.is_empty() {
                "Collect and record the actual user reply before resuming."
            } else if dependency_wait {
                "Continue the unfinished prerequisite; recheck its source state."
            } else {
                "Inspect the recorded execution and original executor before any retry."
            });
            waiting.push(item);
        } else if !r.active_claims.is_empty()
            || matches!(w.item.status, WorkStatus::Claimed | WorkStatus::InProgress)
        {
            item["ownership_required"] = json!(true);
            current.push(item);
        } else if r.ready {
            ready.push(item);
        } else {
            blocked.push(item);
        }
    }
    let links = store.work_goal_links(project.id)?;
    let relevant_goals: BTreeSet<_> = links
        .iter()
        .filter(|e| relevant.contains(e.from_key.as_str()))
        .map(|e| e.to_key.as_str())
        .collect();
    let gaps: Vec<_> = organization
        .gaps
        .iter()
        .filter(|g| {
            g.target == "project"
                || relevant.contains(g.target.as_str())
                || relevant_goals.contains(g.target.as_str())
        })
        .collect();
    let focus = current.first().or_else(|| ready.first());
    let guidance = if !current.is_empty() {
        json!({"when":"Current work can be continued","basis":"Source structure, dependencies and recorded claims","next_action":"Prepare the selected work; continue only with its owned session or explicitly resume it","recheck":"Source, ownership, wait or execution outcome changes"})
    } else if !ready.is_empty() {
        json!({"when":"Work is ready to claim","basis":"Readiness and current source structure","next_action":"Prepare required context, then acquire an owned session before execution","recheck":"Claim conflict, source or dependency change"})
    } else if !waiting.is_empty() {
        json!({"when":"Work is waiting","basis":"Recorded user waits, executions or unfinished dependencies","next_action":"Resolve the indicated wait; query unknown results before retrying","recheck":"Actual reply, execution report or prerequisite completion"})
    } else {
        json!({"when":"No actionable work in this selection","basis":"Current diagnostics and source status; history is not newly verified","next_action":"Inspect the cited current gap or selected completed work and its original evidence","recheck":"Source correction or explicit verification result"})
    };
    let pending = store.inspect_runtime(project.id, now_millis()?)?;
    let mut response = json!({"view":"action","schema_version":1,"project":project.name,"project_id":project.id,
        "branch_id":readiness.branch_id,"scope":scope,"scope_combination":"intersection",
        "total":selected.len(),"project_work_total":works.len(),"counts":counts,
        "count_basis":"source status; queue buckets are navigation, not execution authorization or completion proof",
        "current_total":current.len(),"ready_count":ready.len(),"waiting_count":waiting.len(),"blocked_count":blocked.len(),
        "current":current.iter().take(5).collect::<Vec<_>>(),"ready":ready.iter().take(5).collect::<Vec<_>>(),
        "waiting":waiting.iter().take(5).collect::<Vec<_>>(),"blocked":blocked.iter().take(5).collect::<Vec<_>>(),
        "suggested_work":focus,"next_action":focus.map(|w| w["next_action"].clone()).unwrap_or_else(||guidance["next_action"].clone()),"guidance":guidance,
        "organization":{"state":organization.state,"business_execution_ready":organization.business_execution_ready,
            "gaps":gaps.iter().take(5).map(|g|json!({"code":g.code,"target":g.target,"detail":short(&g.detail)})).collect::<Vec<_>>(),
            "gap_sample_total":gaps.len(),"project_gap_total":organization.gap_total,"current_gap_total":organization.gap_total-organization.historical_gap_total},
        "history":{"basis":"project-wide; original source status preserved","source_completed":organization.source_completed,"verified_completed":organization.verified_completed,
            "not_checked":organization.completion_not_checked,"check_blocked":organization.completion_check_blocked,"verification_failed":organization.completion_verification_failed,
            "user_confirmed_completed":organization.user_confirmed_completed,"business_checked_completed":organization.business_checked_completed,
            "gap_total":organization.historical_gap_total,"source_sha":organization.source_sha,
            "next_action":"Inspect original evidence and its recorded source SHA; never substitute current HEAD for historical proof",
            "details":["work show KEY","intake inspect --source-sha SHA","status --view full"]},
        "pending_operations":{"basis":"project-wide registered runtime findings; not a count of unfinished source tasks","total":pending.findings.len(),"items":pending.findings.iter().take(5).map(|f|json!({"code":f.code,"kind":f.object_kind,"id":f.object_id})).collect::<Vec<_>>(),"details":"recovery inspect"},
        "omissions":{"current":current.len().saturating_sub(5),"ready":ready.len().saturating_sub(5),"waiting":waiting.len().saturating_sub(5),"blocked":blocked.len().saturating_sub(5),
            "gaps":gaps.len().saturating_sub(5),"organization_scan_truncated":organization.truncated,"pending_operations":pending.findings.len().saturating_sub(5),"outside_scope":works.len()-selected.len(),
            "not_evaluated":["host-private or filesystem-only receipts","live execution probes"],"details":"status --view full; work show KEY; ready remains the claim queue"}});
    if let Some((queue, offset, limit)) = page {
        response["page"] = queue_page(
            &[
                ("current", &current),
                ("ready", &ready),
                ("waiting", &waiting),
                ("blocked", &blocked),
            ],
            queue,
            offset,
            limit,
        )?;
    }
    Ok(response)
}

fn queue_page(
    queues: &[(&str, &Vec<Value>)],
    queue: &str,
    offset: usize,
    limit: usize,
) -> Result<Value> {
    if !(1..=100).contains(&limit)
        || !["all", "current", "ready", "waiting", "blocked"].contains(&queue)
    {
        return Err(Error::InvalidInput(
            "queue must be all/current/ready/waiting/blocked; limit must be 1..100".into(),
        ));
    }
    let rows: Vec<_> = queues
        .iter()
        .filter(|(name, _)| queue == "all" || *name == queue)
        .flat_map(|(name, items)| {
            items.iter().map(move |item| {
                let mut row = item.clone();
                row["queue"] = json!(name);
                row
            })
        })
        .collect();
    let total = rows.len();
    let items: Vec<_> = rows.into_iter().skip(offset).take(limit).collect();
    let next = offset.saturating_add(items.len());
    Ok(
        json!({"queue":queue,"offset":offset,"limit":limit,"total":total,
        "has_more":next < total,"items":items}),
    )
}

#[cfg(test)]
mod pagination_tests {
    use super::*;
    #[test]
    fn pages_cover_every_queue_without_duplicates() {
        let current = (0..7).map(|i| json!({"key":format!("C{i}")})).collect();
        let ready = (0..117).map(|i| json!({"key":format!("R{i}")})).collect();
        let queues = [("current", &current), ("ready", &ready)];
        let first = queue_page(&queues, "ready", 0, 100).unwrap();
        let last = queue_page(&queues, "ready", 100, 100).unwrap();
        assert_eq!(first["total"], 117);
        assert_eq!(first["items"].as_array().unwrap().len(), 100);
        assert_eq!(last["items"].as_array().unwrap().len(), 17);
        assert_eq!(last["has_more"], false);
        assert_eq!(last["items"][0]["key"], "R100");
        assert_eq!(
            queue_page(&queues, "current", 5, 5).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(queue_page(&queues, "all", 0, 10).unwrap()["total"], 124);
        assert!(
            queue_page(&queues, "all", 999, 10).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(queue_page(&queues, "all", 0, 0).is_err());
        assert!(queue_page(&queues, "other", 0, 10).is_err());
    }
}
