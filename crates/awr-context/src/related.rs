use crate::SourceVersion;
use awr_core::*;
use awr_store::Store;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize)]
pub struct DependencyFact {
    pub meta: ProjectionMeta,
    pub title: String,
    pub status: WorkStatus,
    pub raw_status: String,
    pub next_action: String,
    pub blocker: Option<String>,
    pub freshness: Freshness,
}
#[derive(Debug, Clone, Serialize)]
pub struct DecisionFact {
    pub meta: ProjectionMeta,
    pub title: String,
    pub status: DecisionStatus,
    pub raw_status: String,
    pub statement: String,
    pub affected_keys: Vec<String>,
    pub paths: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct DecisionGap {
    pub meta: ProjectionMeta,
    pub reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceSummary {
    pub id: Id,
    pub external_key: String,
    pub evidence_type: String,
    pub level: EvidenceLevel,
    pub summary: String,
    pub locator: String,
    pub sha256: Option<String>,
    pub source_sha: Option<String>,
    pub scope: Vec<String>,
    pub branch_id: Option<Id>,
    pub source_ref: Option<SourceRef>,
    pub revision: Revision,
    pub verified_at: Option<i64>,
    pub currency: EvidenceCurrency,
    pub missing_bindings: Vec<String>,
    pub reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceGap {
    pub code: &'static str,
    pub reference: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RelatedWorkContext {
    pub project_id: Id,
    pub project_revision: Revision,
    pub work_item_id: Id,
    pub work_item_key: String,
    pub branch_id: Option<Id>,
    pub requested_source_sha: Option<String>,
    pub unresolved_dependencies: Vec<DependencyFact>,
    pub resolved_dependencies: Vec<DependencyFact>,
    pub required_edges: Vec<Edge>,
    pub missing_dependencies: Vec<String>,
    pub dependency_cycles: Vec<String>,
    pub accepted_decisions: Vec<DecisionFact>,
    pub uncertain_decisions: Vec<DecisionGap>,
    pub evidence: Vec<EvidenceSummary>,
    pub evidence_gaps: Vec<EvidenceGap>,
    pub source_revisions: Vec<SourceVersion>,
    pub source_issues: Vec<String>,
}
fn summary(text: &str) -> Result<String> {
    awr_core::public_summary(text, 240).map_err(|_| {
        Error::ContextIncomplete("selected evidence summary contains sensitive content".into())
    })
}

/// Select related facts from one refreshed project revision without opening raw bodies.
pub fn related_work(
    store: &Store,
    project: Id,
    work_key: &str,
    branch: Option<Id>,
    source_sha: Option<&str>,
    paths: Option<&[String]>,
) -> Result<RelatedWorkContext> {
    let mut selection = RelatedSelection::dependencies(store, project, work_key, branch)?;
    selection.decisions(store, paths)?;
    selection.evidence(store, source_sha)?;
    crate::public_context(selection.finish(store))
}

/// Internal staged selection lets L1 place rule and delta resolution between these read phases.
pub(crate) struct RelatedSelection {
    context: RelatedWorkContext,
    ids: BTreeSet<Id>,
}
impl RelatedSelection {
    pub(crate) fn dependencies(
        store: &Store,
        project: Id,
        work_key: &str,
        branch: Option<Id>,
    ) -> Result<Self> {
        let project = store.project(project)?;
        crate::branch::branch_binding(store, &project, branch)?;
        let work = store.work_item(project.id, work_key)?;
        let graph = store.dependency_closure(project.id, work_key, true)?;
        let mut ids = BTreeSet::from([work.source.id]);
        let mut unresolved_dependencies = Vec::new();
        let mut resolved_dependencies = Vec::new();
        for dependency in graph.dependencies {
            ids.insert(dependency.source.id);
            let item = dependency.item;
            let fact = DependencyFact {
                meta: item.meta,
                title: item.title,
                status: item.status,
                raw_status: item.raw_status,
                next_action: item.next_action,
                blocker: item.blocker,
                freshness: dependency.source.freshness,
            };
            if fact.status == WorkStatus::Completed && fact.freshness == Freshness::Fresh {
                resolved_dependencies.push(fact);
            } else {
                unresolved_dependencies.push(fact);
            }
        }
        let required_edges = graph
            .edges
            .into_iter()
            .map(|edge| {
                ids.insert(edge.source.id);
                edge.item
            })
            .collect();
        Ok(Self {
            context: RelatedWorkContext {
                project_id: project.id,
                project_revision: project.project_revision,
                work_item_id: work.item.meta.id,
                work_item_key: work_key.into(),
                branch_id: branch,
                requested_source_sha: None,
                unresolved_dependencies,
                resolved_dependencies,
                required_edges,
                missing_dependencies: graph.missing_keys,
                dependency_cycles: graph.cycle_keys,
                accepted_decisions: vec![],
                uncertain_decisions: vec![],
                evidence: vec![],
                evidence_gaps: vec![],
                source_revisions: vec![],
                source_issues: vec![],
            },
            ids,
        })
    }
    pub(crate) fn decisions(&mut self, store: &Store, paths: Option<&[String]>) -> Result<()> {
        let work_key = self.context.work_item_key.as_str();
        let mut accepted_decisions = Vec::new();
        let mut uncertain_decisions = Vec::new();
        for related in
            store.decisions_for_work_with_paths(self.context.project_id, work_key, paths)?
        {
            self.ids.insert(related.decision.source.id);
            let item = related.decision.item;
            if related.relevance == Applicability::Applicable {
                if item.decision.trim().is_empty() {
                    uncertain_decisions.push(DecisionGap {
                        meta: item.meta,
                        reasons: vec!["accepted decision statement is empty".into()],
                    });
                } else {
                    accepted_decisions.push(DecisionFact {
                        meta: item.meta,
                        title: item.title,
                        status: item.status,
                        raw_status: item.raw_status,
                        statement: item.decision,
                        affected_keys: item.affected_keys,
                        paths: item.paths,
                    });
                }
            } else {
                uncertain_decisions.push(DecisionGap {
                    meta: item.meta,
                    reasons: related.reasons,
                });
            }
        }
        self.context.accepted_decisions = accepted_decisions;
        self.context.uncertain_decisions = uncertain_decisions;
        Ok(())
    }
    pub(crate) fn evidence(&mut self, store: &Store, source_sha: Option<&str>) -> Result<()> {
        self.select_evidence(store, source_sha, false)
    }
    pub(crate) fn branch_evidence(
        &mut self,
        store: &Store,
        source_sha: Option<&str>,
    ) -> Result<()> {
        self.select_evidence(store, source_sha, true)
    }
    fn select_evidence(
        &mut self,
        store: &Store,
        source_sha: Option<&str>,
        isolate_runtime: bool,
    ) -> Result<()> {
        let work_key = self.context.work_item_key.as_str();
        let branch = self.context.branch_id;
        let assessments = store
            .evidence_for_work(self.context.project_id, work_key, source_sha, branch)?
            .into_iter()
            .filter(|a| {
                !isolate_runtime
                    || a.evidence.source.is_some()
                    || a.evidence.item.branch_id == branch
            })
            .collect::<Vec<_>>();
        let mut evidence = Vec::new();
        let mut evidence_gaps = Vec::new();
        if assessments.is_empty() {
            evidence_gaps.push(EvidenceGap {
                code: "no_evidence",
                reference: work_key.into(),
                reason: "no evidence is associated with this work".into(),
            });
        }
        for assessment in assessments {
            if let Some(source) = &assessment.evidence.source {
                self.ids.insert(source.id);
            }
            let item = assessment.evidence.item;
            if !assessment.missing_bindings.is_empty() {
                evidence_gaps.push(EvidenceGap {
                    code: "missing_evidence_bindings",
                    reference: item.external_key.clone(),
                    reason: assessment.missing_bindings.join(", "),
                });
            }
            if assessment.currency != EvidenceCurrency::Current {
                evidence_gaps.push(EvidenceGap {
                    code: if assessment.currency == EvidenceCurrency::Historical {
                        "historical_evidence"
                    } else {
                        "evidence_currency_unknown"
                    },
                    reference: item.external_key.clone(),
                    reason: assessment.reasons.join("; "),
                });
            }
            if !matches!(
                item.level,
                EvidenceLevel::LocallyVerified
                    | EvidenceLevel::RealEnvironmentValidated
                    | EvidenceLevel::ReleaseCandidate
                    | EvidenceLevel::Released
            ) {
                evidence_gaps.push(EvidenceGap {
                    code: "evidence_level_unverified",
                    reference: item.external_key.clone(),
                    reason: "evidence level does not claim completed verification".into(),
                });
            }
            evidence.push(EvidenceSummary {
                id: item.id,
                external_key: item.external_key,
                evidence_type: item.evidence_type,
                level: item.level,
                summary: summary(&item.summary)?,
                locator: item.locator,
                sha256: item.sha256,
                source_sha: item.source_sha,
                scope: item.scope,
                branch_id: item.branch_id,
                source_ref: item.source_ref,
                revision: item.revision,
                verified_at: item.verified_at,
                currency: assessment.currency,
                missing_bindings: assessment.missing_bindings,
                reasons: assessment.reasons,
            });
        }
        self.context.evidence = evidence;
        self.context.evidence_gaps = evidence_gaps;
        self.context.requested_source_sha = source_sha.map(str::to_owned);
        Ok(())
    }
    pub(crate) fn finish(mut self, store: &Store) -> Result<RelatedWorkContext> {
        let source_revisions = store
            .sources(self.context.project_id)?
            .into_iter()
            .filter(|s| self.ids.contains(&s.id))
            .map(SourceVersion::from)
            .collect::<Vec<_>>();
        let source_issues = source_revisions
            .iter()
            .filter(|s| s.freshness != Freshness::Fresh)
            .map(|s| format!("source {} is {:?}", s.id, s.freshness))
            .collect();
        let actual = store.project(self.context.project_id)?.project_revision;
        if actual != self.context.project_revision {
            return Err(Error::RevisionConflict {
                expected: self.context.project_revision,
                actual,
            });
        }
        self.context.source_revisions = source_revisions;
        self.context.source_issues = source_issues;
        Ok(self.context)
    }
}
