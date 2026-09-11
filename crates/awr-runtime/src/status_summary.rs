//! Bounded status projection. Detailed evidence and organization remain on demand.
use crate::OrganizationReport;
use awr_core::*;
use awr_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StatusScope {
    pub work: Vec<String>,
    pub goal: Option<String>,
    pub milestone: Option<String>,
}
impl StatusScope {
    pub fn is_empty(&self) -> bool {
        self.work.is_empty() && self.goal.is_none() && self.milestone.is_none()
    }
}
fn short(s: &str) -> String {
    public_summary(s, 240).unwrap_or_else(|_| SENSITIVE_CONTENT_WITHHELD.into())
}
fn brief(w: &Projected<WorkItem>) -> Value {
    json!({"key":w.item.meta.external_key,"title":short(&w.item.title),"status":w.item.status,
        "raw_status":short(&w.item.raw_status),"owner":w.item.owner,"next_action":short(&w.item.next_action),
        "blocker":w.item.blocker.as_deref().map(short),"source_revision":w.item.meta.source_ref.source_revision,
        "details":{"cli":["work","show",&w.item.meta.external_key],"mcp":"awr_work_get"}})
}
/// Scope uses explicit source associations. Omitted verification never becomes a completion claim.
pub fn summarize_status(
    store: &Store,
    project: &Project,
    scope: &StatusScope,
    works: &[Projected<WorkItem>],
    readiness: &ReadyReport,
    organization: &OrganizationReport,
) -> Result<Value> {
    if scope.work.len() > 100 {
        return Err(Error::InvalidInput(
            "summary accepts at most 100 work keys".into(),
        ));
    }
    for key in &scope.work {
        if !works.iter().any(|w| &w.item.meta.external_key == key) {
            return Err(Error::NotFound(format!("work {key}")));
        }
    }
    let goal_keys = if let Some(goal) = &scope.goal {
        if !store
            .goals(project.id)?
            .iter()
            .any(|g| &g.item.meta.external_key == goal)
        {
            return Err(Error::NotFound(format!("goal {goal}")));
        }
        Some(
            store
                .work_goal_links(project.id)?
                .into_iter()
                .filter(|e| &e.to_key == goal)
                .map(|e| e.from_key)
                .collect::<BTreeSet<_>>(),
        )
    } else {
        None
    };
    if let Some(milestone) = &scope.milestone {
        store.plan(project.id, milestone)?;
    }
    let mut selected = works
        .iter()
        .filter(|w| {
            (scope.work.is_empty() || scope.work.contains(&w.item.meta.external_key))
                && goal_keys
                    .as_ref()
                    .is_none_or(|keys| keys.contains(&w.item.meta.external_key))
                && scope
                    .milestone
                    .as_ref()
                    .is_none_or(|m| w.item.milestone.as_ref() == Some(m))
        })
        .collect::<Vec<_>>();
    selected.sort_by_key(|w| (&w.item.priority, &w.item.meta.external_key));
    let keys = selected
        .iter()
        .map(|w| w.item.meta.external_key.as_str())
        .collect::<BTreeSet<_>>();
    let mut counts = BTreeMap::<String, usize>::new();
    for w in &selected {
        *counts
            .entry(
                serde_json::to_value(w.item.status)?
                    .as_str()
                    .unwrap()
                    .into(),
            )
            .or_default() += 1;
    }
    let current = selected
        .iter()
        .filter(|w| matches!(w.item.status, WorkStatus::Claimed | WorkStatus::InProgress))
        .copied()
        .collect::<Vec<_>>();
    let ready = readiness
        .ready
        .iter()
        .filter(|r| keys.contains(r.work.item.meta.external_key.as_str()))
        .collect::<Vec<_>>();
    let blocked = readiness
        .blocked
        .iter()
        .filter(|r| keys.contains(r.work.item.meta.external_key.as_str()))
        .collect::<Vec<_>>();
    let focus = current
        .iter()
        .copied()
        .find(|w| {
            organization
                .executable_work
                .contains(&w.item.meta.external_key)
        })
        .or_else(|| {
            ready.iter().map(|r| &r.work).find(|w| {
                organization
                    .executable_work
                    .contains(&w.item.meta.external_key)
            })
        });
    let sources = store.sources(project.id)?;
    let runtime = store.inspect_runtime(project.id, now_millis()?)?;
    let pending = runtime
        .findings
        .iter()
        .filter(|f| f.code.contains("mutation") || f.code.contains("checkpoint"))
        .collect::<Vec<_>>();
    let gaps = organization
        .gaps
        .iter()
        .filter(|g| {
            g.target == "project"
                || keys.contains(g.target.as_str())
                || scope.goal.as_ref() == Some(&g.target)
        })
        .collect::<Vec<_>>();
    let mut diagnostic_counts = BTreeMap::<String, usize>::new();
    for r in &blocked {
        for d in &r.diagnostics {
            *diagnostic_counts.entry(d.code.clone()).or_default() += 1;
        }
    }
    Ok(
        json!({"view":"summary","schema_version":1,"project":project.name,"project_id":project.id,
        "branch_id":readiness.branch_id,"scope":scope,"scope_combination":"intersection","total":selected.len(),"counts":counts,
        "count_basis":"source status; acceptance and delivery are not inferred","project_work_total":works.len(),
        "current":current.iter().take(5).map(|w|brief(w)).collect::<Vec<_>>(),"current_total":current.len(),
        "ready_count":ready.len(),"blocked_count":blocked.len(),"suggested_work":focus.map(brief),
        "next_action":focus.map(|w|short(&w.item.next_action)).unwrap_or_else(|| if selected.is_empty(){"No work in this selection; review its source scope.".into()}else{short(&organization.next_action)}),
        "blockers":blocked.iter().take(5).map(|r|json!({"work":r.work.item.meta.external_key,"codes":r.diagnostics.iter().map(|d|&d.code).collect::<BTreeSet<_>>(),"blocker":r.work.item.blocker.as_deref().map(short)})).collect::<Vec<_>>(),"diagnostic_counts":diagnostic_counts,
        "source_freshness":{"basis":"all registered sources in this snapshot","total":sources.len(),"fresh":sources.iter().filter(|s|s.freshness==Freshness::Fresh).count(),"not_fresh":sources.iter().filter(|s|s.freshness!=Freshness::Fresh).map(|s|json!({"id":s.id,"freshness":s.freshness})).take(5).collect::<Vec<_>>()},
        "organization":{"project_state":organization.state,"project_gap_total":organization.gap_total,"project_business_execution_ready":organization.business_execution_ready,"selected_gaps":gaps.iter().take(5).map(|g|json!({"code":g.code,"target":g.target})).collect::<Vec<_>>(),"selected_gap_total":gaps.len()},
        "pending_operations":{"basis":"registered runtime mutation and checkpoint findings, project-wide","total":pending.len(),"items":pending.iter().take(5).map(|f|json!({"code":f.code,"kind":f.object_kind,"id":f.object_id})).collect::<Vec<_>>(),"runtime_attention_total":runtime.findings.len()},
        "omissions":{"current":current.len().saturating_sub(5),"blockers":blocked.len().saturating_sub(5),"selected_gaps":gaps.len().saturating_sub(5),"organization_scan_truncated":organization.truncated,"pending_operations":pending.len().saturating_sub(5),"outside_scope":works.len()-selected.len(),"source_details":sources.len(),"not_fresh_sources":sources.iter().filter(|s|s.freshness!=Freshness::Fresh).count().saturating_sub(5),"text_cap_characters":240,
            "not_evaluated":["host-private or filesystem-only pending receipts","full evidence verification unless source_sha supplied"],"details":["status (full view)","work show KEY","source list","recovery inspect","host status REQUEST"]}}),
    )
}
