use awr_core::*;
use awr_source::{IndexReport, Manifest, index_project};
use awr_store::Store;
use clap::Subcommand;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Subcommand)]
pub enum WorkCommand {
    /// Preview or accept one new source-backed, non-executable task draft.
    Create(crate::work_create::CreateArgs),
    /// Read the durable outcome for a stable creation request without applying it.
    CreateStatus(crate::work_create::StatusArgs),
    /// Explicitly recover a pending creation while retaining externally changed bytes.
    CreateRecover(crate::work_create::RecoverArgs),
    /// Record progress in the source ledger; requires an active owned runtime claim.
    Progress(crate::work_action::ActionArgs),
    /// Mark in-progress source work blocked with a concrete blocker.
    Block(crate::work_action::ActionArgs),
    /// Clear a blocked source state after rechecking required dependencies.
    Unblock(crate::work_action::ActionArgs),
    /// Cancel nonterminal source work and release this session's runtime claims.
    Cancel(crate::work_action::ActionArgs),
    /// Reopen completed/cancelled source work to planned with a reason and next action.
    Reopen(crate::work_action::ActionArgs),
    /// Complete source work only after verifying dependencies, acceptance and actual evidence reports.
    Complete(crate::work_action::CompleteArgs),
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
        #[arg(long)]
        branch: Option<String>,
        /// Explicit full source SHA for evidence currency (does not assume HEAD is clean).
        #[arg(long)]
        source_sha: Option<String>,
        /// Read last recorded facts without refreshing files; currentness is not verified.
        #[arg(long)]
        cached: bool,
    },
}

pub(crate) struct QueryProject {
    pub store: Store,
    pub project: Project,
    pub refresh: IndexReport,
    snapshot: Option<Value>,
    cached: bool,
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
            snapshot: None,
            cached: false,
        })
    }
    pub(crate) fn open_read(root: &Path, cached: bool) -> Result<Self> {
        let root = root.canonicalize()?;
        let database = super::source::runtime_dir(&root, false)?.join("state.db");
        if !database.is_file() {
            return Err(Error::NotFound("AWR database; initialize first".into()));
        }
        let snapshot = if cached {
            awr_source::recorded_snapshot(&root)?
        } else {
            let mut origin = Store::open(&database)?;
            awr_source::refresh_snapshot(&mut origin, &root)?
        };
        let project = snapshot.store.project(snapshot.refresh.project_id)?;
        let metadata = json!({"version":1,"storage":"private_memory","coherent":true,
            "project_revision":project.project_revision,"source_state_fingerprint":snapshot.source_state_fingerprint,
            "source_refresh_revision":if cached {None}else{Some(snapshot.refresh.change_window.through_revision)},
            "source_currentness_verified":!cached && snapshot.refresh.ok});
        Ok(Self {
            store: snapshot.store,
            project,
            refresh: snapshot.refresh,
            snapshot: Some(metadata),
            cached,
        })
    }
    pub(crate) fn metadata(&self) -> Value {
        let mut value = json!({"ok":self.refresh.ok,"project_revision":self.project.project_revision,
            "freshness_basis":if self.cached {"last_recorded_source_state"} else {"source_refresh"},"source_refresh_performed":!self.cached,"read_only":self.cached,"source_issues":self.refresh.issues,
            "source_warnings":self.refresh.sources.iter().map(|s|s.warnings.len()).sum::<usize>()});
        if let Some(snapshot) = &self.snapshot {
            value["snapshot"] = snapshot.clone();
        }
        value
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
    awr_core::public_summary(text, 240)
        .unwrap_or_else(|_| awr_core::SENSITIVE_CONTENT_WITHHELD.into())
}

fn brief(work: &Projected<WorkItem>) -> Value {
    json!({"id":work.item.meta.id,"external_key":work.item.meta.external_key,"title":work.item.title,
        "archived":work.item.archived,"ordinary_completion":work.item.ordinary_completion,"ordinary_work_policy":work.source.config["adapter_options"]["ordinary_work_policy"],
        "status":work.item.status,"raw_status":work.item.raw_status,"summary":short(if work.item.summary.is_empty(){&work.item.title}else{&work.item.summary}),
        "priority":work.item.priority,"milestone":work.item.milestone,"owner":work.item.owner,"next_action":work.item.next_action,
        "blocker":work.item.blocker,"revision":work.item.meta.revision,"source_revision":work.item.meta.source_ref.source_revision,"freshness":work.source.freshness})
}
fn ready_brief(work: &WorkReadiness) -> Value {
    let mut value = brief(&work.work);
    value["ready"] = json!(work.ready);
    value["diagnostics"] = json!(work.diagnostics);
    value["active_claims"]=json!(work.active_claims.iter().map(|c|json!({"id":c.id,"agent_id":c.agent_id,"session_id":c.session_id,"expires_at":c.expires_at})).collect::<Vec<_>>());
    value
}

pub(crate) fn branch(
    store: &Store,
    project: &Project,
    reference: Option<&str>,
) -> Result<Option<Id>> {
    match reference {
        Some(reference) => store.resolve_branch(project.id, reference),
        None => Ok(project.current_branch_id),
    }
}

pub fn status(
    root: &Path,
    reference: Option<&str>,
    source_sha: Option<&str>,
    json_output: bool,
    cached: bool,
) -> Result<()> {
    status_with_scope(root, reference, source_sha, json_output, cached, None)
}
pub fn status_with_scope(
    root: &Path,
    reference: Option<&str>,
    source_sha: Option<&str>,
    json_output: bool,
    cached: bool,
    scope: Option<&awr_runtime::StatusScope>,
) -> Result<()> {
    if source_sha.is_some_and(|s| !is_source_sha(s)) {
        return Err(Error::InvalidInput(
            "--source-sha requires a full source SHA".into(),
        ));
    }
    let root = root.canonicalize()?;
    let query = match QueryProject::open_read(&root, cached) {
        Ok(query) => query,
        Err(error) => {
            let initialized =
                root.join(".awr/project.toml").exists() || root.join(".awr/state.db").exists();
            let organization =
                awr_runtime::OrganizationReport::unavailable(initialized, &error.report().message);
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({"ok":false,"total":null,"organization":organization,"next_action":organization.next_action,"error":error.report()})
                    )?
                );
            } else {
                println!("{}", organization.rendered());
            }
            return Err(error);
        }
    };
    let branch_id = branch(&query.store, &query.project, reference)?;
    let works = query.store.work_items(query.project.id)?;
    let report = query
        .store
        .ready_work(query.project.id, branch_id, now_millis()?)?;
    let organization = awr_runtime::inspect_organization(
        &query.store,
        &query.project,
        branch_id,
        source_sha,
        query.refresh.ok,
        &works,
        &report,
    )?;
    if let Some(scope) = scope {
        let mut value = awr_runtime::summarize_status(
            &query.store,
            &query.project,
            scope,
            &works,
            &report,
            &organization,
        )?;
        for (key, item) in query.metadata().as_object().expect("query metadata") {
            value[key] = item.clone();
        }
        query.check_revision()?;
        if json_output {
            println!("{}", serde_json::to_string(&value)?);
        } else {
            println!(
                "Project: {} | {} selected | {} active | {} ready | {} blocked\nNext: {}\nDetails: awr status; awr work show KEY",
                query.project.name,
                value["total"],
                value["current_total"],
                value["ready_count"],
                value["blocked_count"],
                value["next_action"]
            );
        }
        return query.finish();
    }
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
        .iter()
        .find(|w| {
            organization
                .executable_work
                .contains(&w.item.meta.external_key)
        })
        .copied()
        .or_else(|| {
            report
                .ready
                .iter()
                .find(|w| {
                    organization
                        .executable_work
                        .contains(&w.work.item.meta.external_key)
                })
                .map(|w| &w.work)
        });
    let work_next = focus
        .map(|w| w.item.next_action.clone())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| organization.next_action.clone());
    let next = if organization.business_execution_ready {
        work_next
    } else {
        organization.next_action.clone()
    };
    let mut value = query.metadata();
    value["project"] = json!(query.project.name);
    value["project_id"] = json!(query.project.id);
    value["branch_id"] = json!(branch_id);
    value["counts"] = json!(counts);
    value["total"] = json!(works.len());
    value["current"] = json!(current.iter().take(5).map(|w| brief(w)).collect::<Vec<_>>());
    value["current_total"] = json!(current.len());
    value["ready_count"] = json!(report.ready.len());
    value["blocked_count"] = json!(report.blocked.len());
    value["next_action"] = json!(next);
    value["suggested_work"] = focus.map(brief).unwrap_or(Value::Null);
    value["organization"] = json!(organization);
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
        println!("{}", organization.rendered());
    }
    query.finish()
}

pub fn ready(
    root: &Path,
    limit: usize,
    reference: Option<&str>,
    json_output: bool,
    cached: bool,
) -> Result<()> {
    if limit == 0 || limit > 100 {
        return Err(Error::InvalidInput("ready limit must be 1..100".into()));
    }
    let query = QueryProject::open_read(root, cached)?;
    let branch_id = branch(&query.store, &query.project, reference)?;
    let report = query
        .store
        .ready_work(query.project.id, branch_id, now_millis()?)?;
    let organization = awr_runtime::inspect_organization(
        &query.store,
        &query.project,
        branch_id,
        None,
        query.refresh.ok,
        &query.store.work_items(query.project.id)?,
        &report,
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
    value["branch_id"] = json!(branch_id);
    value["organization"] = json!(organization);
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
        if report.ready.is_empty() || !organization.business_execution_ready {
            println!("Next: {}", organization.next_action);
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
        WorkCommand::Create(args) => crate::work_create::create(root, args, json_output),
        WorkCommand::CreateStatus(args) => crate::work_create::status(root, args, json_output),
        WorkCommand::CreateRecover(args) => crate::work_create::recover(root, args, json_output),
        WorkCommand::Progress(args) => {
            crate::work_action::run(root, args, WorkAction::Progress, json_output)
        }
        WorkCommand::Block(args) => {
            crate::work_action::run(root, args, WorkAction::Block, json_output)
        }
        WorkCommand::Unblock(args) => {
            crate::work_action::run(root, args, WorkAction::Unblock, json_output)
        }
        WorkCommand::Cancel(args) => {
            crate::work_action::run(root, args, WorkAction::Cancel, json_output)
        }
        WorkCommand::Reopen(args) => {
            crate::work_action::run(root, args, WorkAction::Reopen, json_output)
        }
        WorkCommand::Complete(args) => crate::work_action::complete(root, args, json_output),
        WorkCommand::Claim(_)
        | WorkCommand::Release(_)
        | WorkCommand::History(_)
        | WorkCommand::Handoff(_) => crate::session::work(root, command, json_output),
        WorkCommand::Show {
            id,
            source_sha,
            branch: reference,
            cached,
        } => {
            if source_sha.as_ref().is_some_and(|s| !is_source_sha(s)) {
                return Err(Error::InvalidInput(
                    "--source-sha requires a full source SHA".into(),
                ));
            }
            let query = QueryProject::open_read(root, *cached)?;
            let branch_id = branch(&query.store, &query.project, reference.as_deref())?;
            let work =
                query
                    .store
                    .work_readiness(query.project.id, id, branch_id, now_millis()?)?;
            let decisions = query.store.decisions_for_work(query.project.id, id)?;
            let evidence = query.store.evidence_for_work(
                query.project.id,
                id,
                source_sha.as_deref(),
                branch_id,
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
            value["evidence_currency_basis"] =
                json!({"requested_source_sha":source_sha,"branch_id":branch_id});
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
