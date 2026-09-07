use awr_core::*;
use awr_source::{IndexReport, Manifest, index_project};
use awr_store::Store;
use clap::Subcommand;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Subcommand)]
pub enum WorkCommand {
    /// Acquire a runtime claim for the selected session; never rewrites source ownership.
    Claim(crate::session::ClaimArgs),
    /// Release an explicit claim held by the selected session.
    Release(crate::session::ReleaseArgs),
    /// Read bounded event summaries for one work item, including retained history.
    History(crate::session::HistoryArgs),
    /// Close a session and hand off its latest checkpoint, optionally transferring its claim.
    Handoff(crate::session::HandoffArgs),
    /// Show source state, readiness, acceptance and related decision/evidence summaries.
    Show {
        id: String,
        /// Explicit full source SHA for evidence currency (does not assume HEAD is clean).
        #[arg(long)]
        source_sha: Option<String>,
    },
}

pub(crate) struct QueryProject {
    pub store: Store,
    pub project: Project,
    pub refresh: IndexReport,
}
impl QueryProject {
    /// Refresh the rebuildable projection cache only; authoritative files are never modified.
    pub(crate) fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let runtime = super::source::runtime_dir(&root, false)?;
        let database = runtime.join("state.db");
        if !database.is_file() {
            return Err(Error::NotFound(
                "AWR database; initialize this project first".into(),
            ));
        }
        let manifest = Manifest::load(&root)?;
        let mut store = Store::open(&database)?;
        let refresh = index_project(&mut store, &root, &manifest, false)?;
        let project = store.project(refresh.project_id)?;
        Ok(Self {
            store,
            project,
            refresh,
        })
    }
    pub(crate) fn metadata(&self) -> Value {
        json!({"ok":self.refresh.ok,"project_revision":self.project.project_revision,
            "freshness_basis":"source_refresh","source_issues":self.refresh.issues,
            "source_warnings":self.refresh.sources.iter().map(|s|s.warnings.len()).sum::<usize>()})
    }
    pub(crate) fn finish(&self) -> Result<()> {
        if self.refresh.ok {
            Ok(())
        } else {
            Err(Error::SourceStale(
                "source refresh incomplete; run source reindex or use --json to inspect source_issues".into(),
            ))
        }
    }
    // Other processes may advance runtime state during a multi-query view. Never label a mixed view coherent.
    pub(crate) fn check_revision(&self) -> Result<()> {
        let actual = self.store.project(self.project.id)?.project_revision;
        if actual == self.project.project_revision {
            Ok(())
        } else {
            Err(Error::RevisionConflict {
                expected: self.project.project_revision,
                actual,
            })
        }
    }
}

pub(crate) fn short(text: &str) -> String {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > 240 {
        format!("{}…", clean.chars().take(240).collect::<String>())
    } else {
        clean
    }
}

fn brief(work: &Projected<WorkItem>) -> Value {
    json!({"id":work.item.meta.id,"external_key":work.item.meta.external_key,"title":work.item.title,
        "status":work.item.status,"raw_status":work.item.raw_status,"summary":short(if work.item.summary.is_empty(){&work.item.title}else{&work.item.summary}),
        "priority":work.item.priority,"milestone":work.item.milestone,"owner":work.item.owner,"next_action":work.item.next_action,
        "blocker":work.item.blocker,"revision":work.item.meta.revision,"source_revision":work.item.meta.source_ref.source_revision,"freshness":work.source.freshness})
}
fn ready_brief(work: &WorkReadiness) -> Value {
    let mut value = brief(&work.work);
    value["ready"] = json!(work.ready);
    value["diagnostics"] = json!(work.diagnostics);
    value["active_claims"]=json!(work.active_claims.iter().map(|c|json!({"agent_id":c.agent_id,"session_id":c.session_id,"expires_at":c.expires_at})).collect::<Vec<_>>());
    value
}

pub fn status(root: &Path, json_output: bool) -> Result<()> {
    let query = QueryProject::open(root)?;
    let works = query.store.work_items(query.project.id)?;
    let report = query.store.ready_work(
        query.project.id,
        query.project.current_branch_id,
        now_millis()?,
    )?;
    let mut counts = BTreeMap::<String, usize>::new();
    for work in &works {
        *counts
            .entry(
                serde_json::to_value(work.item.status)?
                    .as_str()
                    .unwrap()
                    .into(),
            )
            .or_default() += 1;
    }
    let mut current = works
        .iter()
        .filter(|w| matches!(w.item.status, WorkStatus::Claimed | WorkStatus::InProgress))
        .collect::<Vec<_>>();
    current.sort_by_key(|w| (&w.item.priority, &w.item.meta.external_key));
    let focus = current
        .first()
        .copied()
        .or_else(|| report.ready.first().map(|w| &w.work));
    let next = focus
        .map(|w| w.item.next_action.clone())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if report.blocked.is_empty() {
                "No nonterminal work remains in current source projections.".into()
            } else {
                "Inspect readiness diagnostics and resolve the blocking prerequisite.".into()
            }
        });
    let mut value = query.metadata();
    value["project"] = json!(query.project.name);
    value["counts"] = json!(counts);
    value["total"] = json!(works.len());
    value["current"] = json!(current.iter().take(5).map(|w| brief(w)).collect::<Vec<_>>());
    value["current_total"] = json!(current.len());
    value["ready_count"] = json!(report.ready.len());
    value["blocked_count"] = json!(report.blocked.len());
    value["next_action"] = json!(next);
    value["suggested_work"] = focus.map(brief).unwrap_or(Value::Null);
    query.check_revision()?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "Project: {}\nRevision: {}\nWork: {} total; {} ready; {} not selectable",
            query.project.name,
            query.project.project_revision,
            works.len(),
            report.ready.len(),
            report.blocked.len()
        );
        if let Some(work) = focus {
            println!(
                "Current: {} — {}\nStatus: {}\nBlocker: {}",
                work.item.meta.external_key,
                work.item.title,
                work.item.raw_status,
                work.item.blocker.as_deref().unwrap_or("none")
            );
        }
        println!(
            "Next: {next}\nSource warnings: {}; issues: {}",
            value["source_warnings"],
            query.refresh.issues.len()
        );
    }
    query.finish()
}

pub fn ready(root: &Path, limit: usize, json_output: bool) -> Result<()> {
    if limit == 0 || limit > 100 {
        return Err(Error::InvalidInput("ready limit must be 1..100".into()));
    }
    let query = QueryProject::open(root)?;
    let report = query.store.ready_work(
        query.project.id,
        query.project.current_branch_id,
        now_millis()?,
    )?;
    let mut counts = BTreeMap::<String, usize>::new();
    for work in &report.blocked {
        for code in work
            .diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<std::collections::BTreeSet<_>>()
        {
            *counts.entry(code).or_default() += 1;
        }
    }
    let mut value = query.metadata();
    value["ready"] = json!(
        report
            .ready
            .iter()
            .take(limit)
            .map(ready_brief)
            .collect::<Vec<_>>()
    );
    value["ready_total"] = json!(report.ready.len());
    value["truncated"] = json!(report.ready.len() > limit);
    value["blocked_total"] = json!(report.blocked.len());
    value["diagnostic_counts"] = json!(counts);
    value["blocked_sample"] = json!(
        report
            .blocked
            .iter()
            .take(3)
            .map(ready_brief)
            .collect::<Vec<_>>()
    );
    query.check_revision()?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "Ready: {}; not selectable: {}; revision: {}",
            report.ready.len(),
            report.blocked.len(),
            report.project_revision
        );
        for work in report.ready.iter().take(limit) {
            println!(
                "{} {}\n  Next: {}",
                work.work.item.meta.external_key, work.work.item.title, work.work.item.next_action
            );
        }
        if report.ready.is_empty() {
            println!("Next: inspect a blocked item with work show <id>.");
        }
        println!("Blocking reasons: {}", serde_json::to_string(&counts)?);
        if report.ready.len() > limit {
            println!(
                "Showing {limit} of {}; increase --limit for more.",
                report.ready.len()
            );
        }
    }
    query.finish()
}

pub fn work(root: &Path, command: &WorkCommand, json_output: bool) -> Result<()> {
    match command {
        WorkCommand::Claim(_)
        | WorkCommand::Release(_)
        | WorkCommand::History(_)
        | WorkCommand::Handoff(_) => crate::session::work(root, command, json_output),
        WorkCommand::Show { id, source_sha } => {
            if source_sha.as_ref().is_some_and(|s| !is_source_sha(s)) {
                return Err(Error::InvalidInput(
                    "--source-sha requires a full source SHA".into(),
                ));
            }
            let query = QueryProject::open(root)?;
            let work = query.store.work_readiness(
                query.project.id,
                id,
                query.project.current_branch_id,
                now_millis()?,
            )?;
            let decisions = query.store.decisions_for_work(query.project.id, id)?;
            let evidence = query.store.evidence_for_work(
                query.project.id,
                id,
                source_sha.as_deref(),
                query.project.current_branch_id,
            )?;
            let mut value = query.metadata();
            value["work"] = ready_brief(&work);
            value["acceptance"] = json!(work.work.item.acceptance);
            value["source_ref"] = json!(work.work.item.meta.source_ref);
            value["required_dependencies"]=json!(work.dependencies.dependencies.iter().map(|d|json!({"external_key":d.item.meta.external_key,"status":d.item.status,"revision":d.item.meta.revision,"source_revision":d.item.meta.source_ref.source_revision,"freshness":d.source.freshness})).collect::<Vec<_>>());
            value["missing_dependencies"] = json!(work.dependencies.missing_keys);
            value["dependency_cycles"] = json!(work.dependencies.cycle_keys);
            value["decisions"]=json!(decisions.iter().map(|d|json!({"external_key":d.decision.item.meta.external_key,"summary":short(&d.decision.item.decision),"relevance":d.relevance,"reasons":d.reasons,"source_ref":d.decision.item.meta.source_ref})).collect::<Vec<_>>());
            value["evidence"]=json!(evidence.iter().map(|e|json!({"external_key":e.evidence.item.external_key,"summary":short(&e.evidence.item.summary),"level":e.evidence.item.level,"currency":e.currency,"missing_bindings":e.missing_bindings,"locator":e.evidence.item.locator,"source_sha":e.evidence.item.source_sha,"reasons":e.reasons})).collect::<Vec<_>>());
            value["evidence_currency_basis"] = json!({"requested_source_sha":source_sha,"branch_id":query.project.current_branch_id});
            query.check_revision()?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!(
                    "{} — {}\nStatus: {}; ready: {}; revision: {} (project {})\nSummary: {}\nBlocker: {}\nNext: {}",
                    id,
                    work.work.item.title,
                    work.work.item.raw_status,
                    work.ready,
                    work.work.item.meta.revision,
                    work.work.project_revision,
                    short(if work.work.item.summary.is_empty() {
                        &work.work.item.title
                    } else {
                        &work.work.item.summary
                    }),
                    work.work.item.blocker.as_deref().unwrap_or("none"),
                    work.work.item.next_action
                );
                for diagnostic in &work.diagnostics {
                    println!(
                        "{} [{}]: {}",
                        diagnostic.code, diagnostic.work_item_key, diagnostic.detail
                    );
                }
                println!("Acceptance:");
                for criterion in &work.work.item.acceptance {
                    println!("- {criterion}");
                }
                println!(
                    "Dependencies: {}; decisions: {}; evidence: {}\nSource: {} r{} ({:?})",
                    work.dependencies.dependencies.len(),
                    decisions.len(),
                    evidence.len(),
                    work.work.item.meta.source_ref.locator,
                    work.work.item.meta.source_ref.source_revision,
                    work.work.source.freshness
                );
            }
            query.finish()
        }
    }
}
