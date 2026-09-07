use crate::{Locator, Manifest, SourceSnapshot, SourceSpec};
use awr_core::{
    Decision, Edge, EntityKind, Error, Evidence, Goal, Id, MutationProposal, Plan, ProjectionMeta,
    Result, Rule, Source, SourceRef, WorkItem,
};
use awr_store::Store;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Default)]
pub struct ProjectionBatch {
    pub goals: Vec<Goal>,
    pub plans: Vec<Plan>,
    pub rules: Vec<Rule>,
    pub work_items: Vec<WorkItem>,
    pub edges: Vec<Edge>,
    pub decisions: Vec<Decision>,
    pub evidence: Vec<Evidence>,
    pub warnings: Vec<String>,
}

pub struct ParseContext<'a> {
    pub source: &'a Source,
    pub existing_ids: BTreeMap<(EntityKind, String), Id>,
}
impl ParseContext<'_> {
    pub fn meta(
        &self,
        kind: EntityKind,
        key: &str,
        snapshot: &SourceSnapshot,
        pointer: Option<String>,
        lines: Option<(usize, usize)>,
    ) -> Result<ProjectionMeta> {
        Ok(ProjectionMeta {
            id: self
                .existing_ids
                .get(&(kind, key.to_owned()))
                .copied()
                .unwrap_or_else(Id::new),
            external_key: key.into(),
            revision: 1,
            source_ref: SourceRef {
                source_id: self.source.id,
                locator: snapshot.locator.clone(),
                source_revision: self
                    .source
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| Error::InvalidInput("source revision overflow".into()))?,
                source_fingerprint: snapshot.fingerprint.clone(),
                pointer,
                start_line: lines.map(|p| p.0),
                end_line: lines.map(|p| p.1),
            },
        })
    }
}

pub trait SourceAdapter {
    fn name(&self) -> &'static str;
    fn discover(&self, root: &Path, manifest: &Manifest, spec: &SourceSpec)
    -> Result<Vec<Locator>>;
    fn fingerprint(&self, snapshot: &SourceSnapshot) -> String {
        snapshot.fingerprint.clone()
    }
    fn parse(
        &self,
        snapshot: &SourceSnapshot,
        context: &ParseContext<'_>,
        spec: &SourceSpec,
    ) -> Result<ProjectionBatch>;
    fn project(
        &self,
        store: &mut Store,
        source: &Source,
        snapshot: &SourceSnapshot,
        batch: ProjectionBatch,
    ) -> Result<()>;
    fn plan_mutation(
        &self,
        _source: &Source,
        _intent: &serde_json::Value,
    ) -> Result<MutationProposal> {
        Err(Error::MutationUnsupported(format!(
            "{} is read-only",
            self.name()
        )))
    }
    fn apply_mutation(&self, _root: &Path, _proposal: &MutationProposal) -> Result<()> {
        Err(Error::MutationUnsupported(format!(
            "{} cannot apply source changes",
            self.name()
        )))
    }
}
