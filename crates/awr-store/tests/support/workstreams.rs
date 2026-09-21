use awr_core::*;
use awr_store::{SourceRegistration, Store, WorkstreamRead, WorkstreamReadSelection};
use serde_json::json;
use std::{fs, path::PathBuf};

pub const SNAPSHOT: u64 = 32 * 1024 * 1024;

pub struct Fixture {
    pub root: PathBuf,
    pub store: Store,
    pub project: Id,
    pub source: Source,
    pub scopes: [Id; 2],
    pub works: [Id; 4],
    pub batch: ProjectionBatch,
}
impl Fixture {
    pub fn new() -> Self {
        Self::with_workstreams(true)
    }
    pub fn legacy() -> Self {
        Self::with_workstreams(false)
    }
    fn with_workstreams(explicit: bool) -> Self {
        let root = std::env::temp_dir().join(format!("awr-scoped-read-{}", Id::new()));
        fs::create_dir(&root).unwrap();
        let mut store = Store::open(&root.join("state.db")).unwrap();
        let project = store
            .register_project(&root, "read-fixture", "Read fixture")
            .unwrap()
            .id;
        let source = store
            .register_source(
                project,
                &SourceRegistration {
                    domain: "ledger",
                    role: "primary",
                    locator: "file:///fixture/work.yaml",
                    format: "yaml",
                    adapter: "yaml-workstream-ledger-v1",
                },
            )
            .unwrap();
        let scopes = [Id::new(), Id::new()];
        let works = [Id::new(), Id::new(), Id::new(), Id::new()];
        let reference = json!({"source_id":source.id,"locator":source.locator,"source_revision":source.revision+1,"source_fingerprint":"v1"});
        let goals = (0..2).map(|i|serde_json::from_value(json!({
            "id":Id::new(),"external_key":format!("G{i}"),"revision":1,"source_ref":reference,
            "title":format!("Private goal {i}"),"status":"active","priority":null,"success_criteria":[],"summary":"Synthetic goal",
        })).unwrap()).collect();
        let work_items = works.iter().enumerate().map(|(i,id)|serde_json::from_value(json!({
            "id":id,"external_key":format!("W{i}"),"revision":1,"source_ref":reference,
            "title":format!("Search component {i}"),"kind":null,"owner":null,"required":true,
            "raw_status":"ready","status":"ready","priority":null,"milestone":null,"score":null,"evidence_level":null,
            "summary":"Search component implementation","next_action":"Implement","blocker":null,"acceptance":[],"tags":[],"paths":[],
        })).unwrap()).collect();
        let rules = vec![serde_json::from_value(json!({
            "id":Id::new(),"external_key":"SHARED","revision":1,"source_ref":reference,
            "text":"Retain shared constraints","severity":"hard","scope":{"type":"project","value":"*"},"unresolved":[],
        })).unwrap()];
        let plans = (0..3).map(|i|serde_json::from_value(json!({
            "id":Id::new(),"external_key":format!("P{i}"),"revision":1,"source_ref":reference,
            "title":format!("Private plan {i}"),"status":"active","scope":if i==2 {vec!["W0","W1"]} else if i==0 {vec!["W0"]} else {vec!["W1"]},"summary":"Plan",
        })).unwrap()).collect();
        let catalog = WorkstreamCatalog {
            version: 1,
            project_id: project.to_string(),
            legacy_default: None,
            workstreams: scopes
                .iter()
                .enumerate()
                .map(|(i, id)| Workstream {
                    id: *id,
                    project_id: project.to_string(),
                    external_key: format!("S{i}"),
                    title: format!("Scope {i}"),
                    state: WorkstreamState::Active,
                    authority_version: 1,
                    goal_keys: vec![format!("G{i}")],
                    acceptance_contracts: vec![],
                })
                .collect(),
        };
        let batch = ProjectionBatch {
            goals,
            work_items,
            rules,
            plans,
            workstream_projection: explicit.then_some(WorkstreamProjection {
                catalog,
                ownership: works
                    .iter()
                    .enumerate()
                    .map(|(i, work)| WorkstreamWorkBinding {
                        project_id: project.to_string(),
                        work_item_id: work.to_string(),
                        workstream_id: scopes[i % 2],
                    })
                    .collect(),
            }),
            ..Default::default()
        };
        let source = store
            .commit_source_projection(&source, "v1", batch.clone())
            .unwrap();
        let scopes = if explicit {
            scopes
        } else {
            let id = store.workstream_catalog(project).unwrap().workstreams[0].id;
            [id, id]
        };
        Self {
            root,
            store,
            project,
            source,
            scopes,
            works,
            batch,
        }
    }
    pub fn rev(&self) -> Revision {
        self.store.project(self.project).unwrap().project_revision
    }
    pub fn access(&self, indexes: &[usize]) -> WorkstreamAccess {
        WorkstreamAccess {
            project_id: self.project.to_string(),
            subject: "reader".into(),
            grants: indexes
                .iter()
                .map(|i| WorkstreamGrant {
                    workstream_id: self.scopes[*i],
                    authority_version: 1,
                    read: true,
                    write: false,
                    manage: false,
                })
                .collect(),
        }
    }
    pub fn read(&self, i: usize) -> WorkstreamRead {
        self.store
            .read_workstream(
                self.project,
                &self.access(&[i]),
                &WorkstreamReadSelection::default(),
                SNAPSHOT,
            )
            .unwrap()
    }
    pub fn start(&mut self, i: usize) -> Session {
        self.store
            .start_session(
                self.project,
                self.rev(),
                SessionDraft {
                    work_item_key: Some(format!("W{i}")),
                    agent_id: format!("agent-{i}"),
                    provider: "fixture".into(),
                    model: "fixture".into(),
                    branch_id: None,
                    claim: false,
                    claim_ttl_ms: None,
                },
            )
            .unwrap()
            .0
            .session
    }
    pub fn event(&mut self, i: usize, session: Option<Id>) -> Event {
        let mut draft = EventDraft::new("report.observed", format!("Search event {i}"));
        draft.work_item_id = Some(self.works[i]);
        draft.session_id = session;
        self.store
            .append_event(self.project, self.rev(), draft)
            .unwrap()
    }
    pub fn checkpoint(&mut self, session: Id) -> Checkpoint {
        self.store
            .create_checkpoint(
                self.project,
                self.rev(),
                session,
                CheckpointDraft {
                    context_hash: "a".repeat(64),
                    digest: "Retained scope".into(),
                    next_action: "Continue".into(),
                    open_loops: vec![],
                    changed_entities: vec![],
                },
            )
            .unwrap()
            .0
    }
    pub fn evidence(&mut self, i: usize) -> Evidence {
        self.store
            .record_evidence(
                self.project,
                self.rev(),
                EvidenceDraft {
                    work_item_key: Some(format!("W{i}")),
                    external_key: format!("E{i}"),
                    evidence_type: "report".into(),
                    level: EvidenceLevel::Designed,
                    summary: format!("Private evidence {i}"),
                    locator: format!("report-{i}.txt"),
                    sha256: None,
                    source_sha: None,
                    command: None,
                    scope: vec![format!("W{i}")],
                    branch_id: None,
                    verified_at: None,
                },
            )
            .unwrap()
            .0
    }
    pub fn dependency(&mut self, from: &str, to: &str, required: bool) -> Id {
        let id = Id::new();
        self.batch.edges.push(Edge {
            id,
            project_id: self.project,
            from_kind: EntityKind::WorkItem,
            from_key: from.into(),
            relation: "depends_on".into(),
            to_kind: EntityKind::WorkItem,
            to_key: to.into(),
            required,
            revision: 1,
            source_ref: self.batch.work_items[0].meta.source_ref.clone(),
        });
        id
    }
    pub fn reproject(&mut self) {
        self.source = self
            .store
            .mark_source_freshness(&self.source, Freshness::Stale)
            .unwrap();
        let fingerprint = format!("fixture-r{}", self.source.revision + 1);
        let mut value = serde_json::to_value(&self.batch).unwrap();
        fn references(value: &mut serde_json::Value, revision: Revision, fingerprint: &str) {
            match value {
                serde_json::Value::Object(map) => {
                    if map.contains_key("source_id") && map.contains_key("source_revision") {
                        map.insert("source_revision".into(), revision.into());
                        map.insert("source_fingerprint".into(), fingerprint.into());
                    }
                    for v in map.values_mut() {
                        references(v, revision, fingerprint);
                    }
                }
                serde_json::Value::Array(items) => {
                    for v in items {
                        references(v, revision, fingerprint);
                    }
                }
                _ => (),
            }
        }
        references(&mut value, self.source.revision + 1, &fingerprint);
        self.batch = serde_json::from_value(value).unwrap();
        self.source = self
            .store
            .commit_source_projection(&self.source, &fingerprint, self.batch.clone())
            .unwrap();
    }
    pub fn move_work(&mut self, i: usize, to: usize) {
        let ownership = self
            .store
            .workstream_ownership(self.project, self.works[i])
            .unwrap();
        self.source = self
            .store
            .mark_source_freshness(&self.source, Freshness::Stale)
            .unwrap();
        let mut batch = self.batch.clone();
        for work in &mut batch.work_items {
            work.meta.source_ref.source_revision = self.source.revision + 1;
            work.meta.source_ref.source_fingerprint = "moved".into();
        }
        for goal in &mut batch.goals {
            goal.meta.source_ref.source_revision = self.source.revision + 1;
            goal.meta.source_ref.source_fingerprint = "moved".into();
        }
        for rule in &mut batch.rules {
            rule.meta.source_ref.source_revision = self.source.revision + 1;
            rule.meta.source_ref.source_fingerprint = "moved".into();
        }
        for plan in &mut batch.plans {
            plan.meta.source_ref.source_revision = self.source.revision + 1;
            plan.meta.source_ref.source_fingerprint = "moved".into();
        }
        batch.workstream_projection.as_mut().unwrap().ownership[i].workstream_id = self.scopes[to];
        self.source = self
            .store
            .commit_source_projection_with_moves(
                self.rev(),
                &self.source,
                "moved",
                batch.clone(),
                &[WorkstreamMove {
                    work_item_id: self.works[i].to_string(),
                    from: ownership.binding.workstream_id,
                    to: self.scopes[to],
                    expected_ownership_revision: ownership.revision,
                }],
            )
            .unwrap();
        self.batch = batch;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
