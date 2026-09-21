use awr_context::*;
use awr_core::*;
use awr_source::{Manifest, index_project};
use awr_store::{Store, WorkstreamReadSelection};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

const RULES: &str = "# Shared requirement {#shared severity=hard scope=project value=*}\n\nPreserve approved requirements.\n";
const MANIFEST: &str = "[project]\nname='Scoped compiler'\nexternal_key='scoped-compiler'\n[[sources]]\ndomain='ledger'\nrole='primary'\npath='work.yaml'\nadapter='yaml-workstream-ledger-v1'\n[[sources]]\ndomain='rules'\nrole='primary'\npath='rules.md'\nadapter='markdown-rules-v1'\n";
struct Fixture {
    root: PathBuf,
    store: Store,
    project: Id,
    document: Value,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("awr-scoped-compiler-{}", Id::new()));
        fs::create_dir_all(root.join(".awr")).unwrap();
        let document = json!({
          "workstreams":{"version":1,"definitions":[
            {"id":"01K00000000000000000000001","external_key":"api","title":"API","state":"active","authority_version":1,"goal_keys":["G-A"],"acceptance_contracts":[]},
            {"id":"01K00000000000000000000002","external_key":"client","title":"Client","state":"active","authority_version":1,"goal_keys":["G-B"],"acceptance_contracts":[]}]},
          "goals":[{"id":"G-A","title":"API scope goal","status":"active","success_criteria":["Ship the API"]},
            {"id":"G-B","title":"PRIVATE_CLIENT_GOAL","status":"active","success_criteria":["PRIVATE_CLIENT_CRITERIA"]}],
          "work_items":[
            {"id":"API-1","title":"Deliver the API","status":"in_progress","workstream":"api","next_action":"Implement the API","acceptance":["API contract is verified"]},
            {"id":"CLIENT-1","title":"PRIVATE_CLIENT_TASK","status":"in_progress","workstream":"client","next_action":"PRIVATE_CLIENT_NEXT","acceptance":["PRIVATE_CLIENT_ACCEPTANCE"]},
            {"id":"API-2","title":"UNRELATED_API_TASK","status":"planned","workstream":"api","next_action":"Independent API task","acceptance":["Independent result"]}]
        });
        fs::write(
            root.join("work.yaml"),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
        fs::write(root.join("rules.md"), RULES).unwrap();
        fs::write(root.join(".awr/project.toml"), MANIFEST).unwrap();
        let mut store = Store::open(&root.join(".awr/state.db")).unwrap();
        let r = index_project(&mut store, &root, &Manifest::load(&root).unwrap(), false).unwrap();
        assert!(r.ok, "{r:?}");
        Self {
            root,
            store,
            project: r.project_id,
            document,
        }
    }
    fn save(&self) {
        fs::write(
            self.root.join("work.yaml"),
            serde_json::to_vec_pretty(&self.document).unwrap(),
        )
        .unwrap();
    }
    fn rev(&self) -> Revision {
        self.store.project(self.project).unwrap().project_revision
    }
    fn request() -> ContextRequest {
        ContextRequest {
            work_item_key: Some("API-1".into()),
            token_budget: 8000,
            ..Default::default()
        }
    }
    fn compile(&mut self) -> WorkContextReport {
        compile_context(&mut self.store, &self.root, &Self::request()).unwrap()
    }
    fn bootstrap(&mut self) -> BootstrapPack {
        bootstrap(
            &mut self.store,
            &self.root,
            &BootstrapRequest {
                work_item_key: Some("API-1".into()),
                token_budget: 8000,
                ..Default::default()
            },
        )
        .unwrap()
    }
    fn start(&mut self, key: &str) -> Session {
        self.store
            .start_session(
                self.project,
                self.rev(),
                SessionDraft {
                    work_item_key: Some(key.into()),
                    agent_id: format!("agent-{key}"),
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
    fn access(&self, scope: usize) -> WorkstreamAccess {
        let catalog = self.store.workstream_catalog(self.project).unwrap();
        let s = &catalog.workstreams[scope];
        WorkstreamAccess {
            project_id: self.project.to_string(),
            subject: "scoped-test-reader".into(),
            grants: vec![WorkstreamGrant {
                workstream_id: s.id,
                authority_version: s.authority_version,
                read: true,
                write: false,
                manage: false,
            }],
        }
    }
    fn completeness_request() -> CompletenessRequest {
        CompletenessRequest {
            work_item_key: "API-1".into(),
            branch_id: None,
            scope: RuleScopeInput::default(),
            source_sha: None,
        }
    }
    fn move_api(&mut self, destination: &str) {
        let work = self.store.work_item(self.project, "API-1").unwrap();
        let ownership = self
            .store
            .workstream_ownership(self.project, work.item.meta.id)
            .unwrap();
        let target = self
            .store
            .workstream_catalog(self.project)
            .unwrap()
            .workstreams
            .into_iter()
            .find(|s| s.external_key == destination)
            .unwrap()
            .id;
        self.document["work_items"][0]["workstream"] = json!(destination);
        self.save();
        let source = self
            .store
            .mark_source_freshness(&work.source, Freshness::Stale)
            .unwrap();
        let bytes = fs::read(self.root.join("work.yaml")).unwrap();
        let snapshot = awr_source::SourceSnapshot {
            locator: source.locator.clone(),
            fingerprint: awr_source::fingerprint(&bytes),
            bytes,
        };
        let adapter = awr_source::source_adapter(&source.adapter).unwrap();
        let manifest = Manifest::load(&self.root).unwrap();
        let batch = adapter
            .parse(
                &snapshot,
                &awr_source::ParseContext {
                    source: &source,
                    existing_ids: self.store.projection_ids(&source).unwrap(),
                },
                &manifest.sources[0],
            )
            .unwrap();
        self.store
            .commit_source_projection_with_moves(
                self.rev(),
                &source,
                &snapshot.fingerprint,
                batch,
                &[WorkstreamMove {
                    work_item_id: work.item.meta.id.to_string(),
                    from: ownership.binding.workstream_id,
                    to: target,
                    expected_ownership_revision: ownership.revision,
                }],
            )
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn default_compiler_selects_current_scope_and_retains_shared_hard_facts() {
    let mut f = Fixture::new();
    let own = f.start("API-1");
    let other = f.start("CLIENT-1");
    f.store
        .create_checkpoint(
            f.project,
            f.rev(),
            other.id,
            CheckpointDraft {
                context_hash: "a".repeat(64),
                digest: "PRIVATE_CLIENT_DIGEST".into(),
                next_action: "PRIVATE_CLIENT_RECOVERY".into(),
                open_loops: vec!["PRIVATE_CLIENT_LOOP".into()],
                changed_entities: vec![],
            },
        )
        .unwrap();
    let report = f.compile();
    assert!(
        report.completeness.complete,
        "{:?}",
        report.completeness.issues
    );
    assert_eq!(report.session_id, Some(own.id));
    assert_eq!(report.goal_selection_basis, "workstream_goal_references");
    let pack = report.work_context.unwrap();
    assert_eq!(pack.policy, WORKSTREAM_BUDGET_POLICY);
    assert_eq!(pack.token_estimate, token_count(&pack.rendered_context));
    assert!(pack.token_estimate <= pack.token_budget);
    for required in [
        "API scope goal",
        "Implement the API",
        "API contract is verified",
        "Preserve approved requirements.",
    ] {
        assert!(
            pack.rendered_context.contains(required),
            "missing {required}"
        );
    }
    assert!(!pack.rendered_context.contains("PRIVATE_CLIENT"));
    assert!(!pack.rendered_context.contains("UNRELATED_API_TASK"));
    assert!(matches!(
        compile_context(&mut f.store, &f.root, &ContextRequest::default()),
        Err(Error::Workstream(WorkstreamError::ScopeRequired))
    ));
}

#[test]
fn unrelated_source_and_runtime_changes_leave_semantic_text_and_hash_identical() {
    let mut f = Fixture::new();
    f.start("API-1");
    let other = f.start("CLIENT-1");
    let before = f.compile().work_context.unwrap();
    let orientation = f.bootstrap();
    f.document["work_items"][1]["next_action"] = json!("PRIVATE_CLIENT_CHANGED");
    f.document["work_items"][2]["next_action"] = json!("UNRELATED_API_CHANGED");
    f.document["goals"][1]["title"] = json!("PRIVATE_CLIENT_CHANGED_GOAL");
    f.save();
    for _ in 0..20 {
        let mut e = EventDraft::new("report.observed", "PRIVATE_CLIENT_EVENT");
        e.session_id = Some(other.id);
        e.importance = "critical".into();
        f.store.append_event(f.project, f.rev(), e).unwrap();
    }
    let after = f.compile().work_context.unwrap();
    assert!(after.identity.project_revision > before.identity.project_revision);
    assert_ne!(
        serde_json::to_value(&after.identity.source_versions).unwrap(),
        serde_json::to_value(&before.identity.source_versions).unwrap()
    );
    assert_eq!(after.rendered_context, before.rendered_context);
    assert_eq!(after.context_hash, before.context_hash);
    let current_orientation = f.bootstrap();
    assert_eq!(
        orientation.rendered_context,
        current_orientation.rendered_context
    );
    assert_eq!(orientation.context_hash, current_orientation.context_hash);
    f.document["work_items"][0]["next_action"] = json!("Review the implemented API");
    f.save();
    let relevant = f.compile().work_context.unwrap();
    assert_ne!(after.context_hash, relevant.context_hash);
    assert!(
        relevant
            .rendered_context
            .contains("Review the implemented API")
    );
}

#[test]
fn hidden_completed_dependency_blocks_without_disclosing_its_body_or_descendants() {
    let mut f = Fixture::new();
    f.document["work_items"][0]["depends_on"] = json!(["CLIENT-1"]);
    f.document["work_items"][1]["status"] = json!("completed");
    f.document["work_items"][1]["depends_on"] = json!(["API-2"]);
    f.save();
    let report = f.compile();
    assert!(!report.completeness.complete);
    assert!(!report.completeness.dependencies_complete);
    assert!(
        report
            .completeness
            .issues
            .iter()
            .any(|i| i.code == "required_dependency_unavailable")
    );
    assert!(
        report
            .rendered_context()
            .contains("do not treat it as satisfied")
    );
    for secret in ["PRIVATE_CLIENT", "CLIENT-1", "UNRELATED_API_TASK", "API-2"] {
        assert!(
            !report.rendered_context().contains(secret),
            "leaked {secret}"
        );
    }
}

#[test]
fn trusted_scope_selection_does_not_turn_selectors_into_permissions() {
    let mut f = Fixture::new();
    let access = f.access(0);
    let own = f.start("API-1");
    f.start("CLIENT-1");
    let implicit = compile_workstream_context(
        &mut f.store,
        &f.root,
        &ContextRequest::default(),
        &access,
        &WorkstreamReadSelection::default(),
    )
    .unwrap();
    assert_eq!(implicit.session_id, Some(own.id));
    for goal in ["G-B", "missing-goal"] {
        let req = ContextRequest {
            goal_keys: vec![goal.into()],
            ..Fixture::request()
        };
        assert!(matches!(
            compile_workstream_context(&mut f.store, &f.root, &req, &access, &Default::default()),
            Err(Error::Workstream(WorkstreamError::AccessDenied))
        ));
    }
    let foreign = ContextRequest {
        work_item_key: Some("CLIENT-1".into()),
        ..Default::default()
    };
    assert!(matches!(
        compile_workstream_context(
            &mut f.store,
            &f.root,
            &foreign,
            &access,
            &Default::default()
        ),
        Err(Error::Workstream(WorkstreamError::AccessDenied))
    ));
    let mut stale = access.clone();
    stale.grants[0].authority_version += 1;
    assert!(matches!(
        compile_workstream_context(
            &mut f.store,
            &f.root,
            &Fixture::request(),
            &stale,
            &Default::default()
        ),
        Err(Error::Workstream(WorkstreamError::StaleAuthority))
    ));
}

#[test]
fn shared_policy_changes_invalidate_and_unknown_hard_rules_remain_mandatory() {
    let mut f = Fixture::new();
    let before = f.compile().work_context.unwrap();
    fs::write(
        f.root.join("rules.md"),
        RULES.replace(
            "Preserve approved requirements.",
            "Preserve approved requirements and review receipts.",
        ),
    )
    .unwrap();
    let after = f.compile().work_context.unwrap();
    assert_ne!(before.context_hash, after.context_hash);
    assert!(after.rendered_context.contains("review receipts"));
    fs::write(
        f.root.join("rules.md"),
        format!("{RULES}\n# Unknown obligation\n\nDo not silently drop this obligation.\n"),
    )
    .unwrap();
    let unknown = f.compile();
    assert!(!unknown.completeness.complete);
    assert!(!unknown.completeness.rules_complete);
    assert!(
        unknown
            .rendered_context()
            .contains("Do not silently drop this obligation.")
    );
}

#[test]
fn legacy_compilation_keeps_its_versioned_layout_and_global_audit_binding() {
    let mut f = Fixture::new();
    // A new independent legacy project is initialized before any scope history.
    let root = f.root.join("legacy");
    fs::create_dir_all(root.join(".awr")).unwrap();
    f.document.as_object_mut().unwrap().remove("workstreams");
    for work in f.document["work_items"].as_array_mut().unwrap() {
        work.as_object_mut().unwrap().remove("workstream");
    }
    fs::write(
        root.join("work.yaml"),
        serde_json::to_vec_pretty(&f.document).unwrap(),
    )
    .unwrap();
    fs::write(root.join("rules.md"), RULES).unwrap();
    fs::write(
        root.join(".awr/project.toml"),
        MANIFEST.replace("yaml-workstream-ledger-v1", "yaml-ledger-v1"),
    )
    .unwrap();
    let mut store = Store::open(&root.join(".awr/state.db")).unwrap();
    let report = compile_context(&mut store, &root, &Fixture::request()).unwrap();
    let pack = report.work_context.unwrap();
    assert_eq!(pack.policy, BUDGET_POLICY);
    assert!(pack.workstream_identity.is_none());
    assert!(pack.rendered_context.contains("PRIVATE_CLIENT_GOAL"));
    assert!(pack.rendered_context.contains("through"));
    assert!(
        !serde_json::to_value(&pack)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("workstream_identity")
    );
}

#[test]
fn standalone_delta_filters_before_source_and_event_limits() {
    let mut f = Fixture::new();
    let own = f.start("API-1");
    let other = f.start("CLIENT-1");
    let baseline = f.rev();
    let mut event = EventDraft::new("report.observed", "Review the API result");
    event.session_id = Some(own.id);
    event.importance = "high".into();
    f.store.append_event(f.project, f.rev(), event).unwrap();
    for _ in 0..20 {
        let mut event = EventDraft::new("report.observed", "PRIVATE_CLIENT_EVENT");
        event.session_id = Some(other.id);
        event.importance = "critical".into();
        f.store.append_event(f.project, f.rev(), event).unwrap();
    }
    let mut global = EventDraft::new("report.observed", "UNATTRIBUTED_EVENT");
    global.importance = "critical".into();
    f.store.append_event(f.project, f.rev(), global).unwrap();
    f.document["work_items"][0]["next_action"] = json!("Review API result");
    f.document["work_items"][1]["next_action"] = json!("PRIVATE_CLIENT_CHANGED");
    f.document["work_items"][2]["next_action"] = json!("UNRELATED_API_CHANGED");
    f.document["goals"][1]["title"] = json!("PRIVATE_CLIENT_CHANGED_GOAL");
    f.save();
    let report = context_delta(
        &mut f.store,
        &f.root,
        &DeltaContextRequest {
            work_item_key: Some("API-1".into()),
            delta: DeltaRequest {
                baseline: DeltaBaseline::Revision { revision: baseline },
                event_limit: 1,
                entity_limit_per_source: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let events = &report.delta.events;
    assert_eq!(events.important_event_count, 1);
    assert_eq!(events.omitted_important_events, 0);
    assert_eq!(events.important_events[0].summary, "Review the API result");
    assert_eq!(events.source_changes.len(), 1);
    let source = &events.source_changes[0];
    assert_eq!(source.changed_entity_count, 1);
    assert_eq!(source.omitted_entities, 0);
    assert_eq!(source.changed_entities[0].external_key, "API-1");
    assert!(report.delta.gaps.is_empty());
    let text = serde_json::to_string(&report).unwrap();
    for secret in ["PRIVATE_CLIENT", "UNRELATED_API", "UNATTRIBUTED_EVENT"] {
        assert!(!text.contains(secret), "leaked {secret}");
    }
}

#[test]
fn removing_a_hidden_dependency_retains_an_opaque_change_and_rechecks_completeness() {
    let mut f = Fixture::new();
    f.document["work_items"][0]["depends_on"] = json!(["CLIENT-1"]);
    f.save();
    assert!(!f.compile().completeness.dependencies_complete);
    let baseline = f.rev();
    f.document["work_items"][0]["depends_on"] = json!([]);
    f.save();
    let report = context_delta(
        &mut f.store,
        &f.root,
        &DeltaContextRequest {
            work_item_key: Some("API-1".into()),
            delta: DeltaRequest {
                baseline: DeltaBaseline::Revision { revision: baseline },
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let changes = report
        .delta
        .events
        .source_changes
        .iter()
        .flat_map(|s| &s.changed_entities)
        .collect::<Vec<_>>();
    assert!(changes.iter().any(|c| c.kind == "edges"
        && c.external_key.starts_with("dependency:")
        && c.after_revision.is_none()));
    assert!(!serde_json::to_string(&report).unwrap().contains("CLIENT-1"));
    let current =
        check_completeness(&mut f.store, &f.root, &Fixture::completeness_request()).unwrap();
    assert!(current.complete);
}

#[test]
fn completeness_uses_the_same_opaque_dependency_and_inactive_scope_rules() {
    let mut f = Fixture::new();
    f.document["work_items"][0]["depends_on"] = json!(["CLIENT-1"]);
    f.document["work_items"][1]["status"] = json!("completed");
    f.document["work_items"][1]["depends_on"] = json!(["API-2"]);
    f.save();
    let report =
        check_completeness(&mut f.store, &f.root, &Fixture::completeness_request()).unwrap();
    assert!(!report.complete && !report.dependencies_complete);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.code == "required_dependency_unavailable")
    );
    let text = serde_json::to_string(&report).unwrap();
    for secret in ["CLIENT-1", "PRIVATE_CLIENT", "API-2"] {
        assert!(!text.contains(secret), "leaked {secret}");
    }
    f.document["work_items"][0]["depends_on"] = json!([]);
    f.document["workstreams"]["definitions"][0]["state"] = json!("paused");
    f.document["workstreams"]["definitions"][0]["authority_version"] = json!(2);
    f.save();
    let report =
        check_completeness(&mut f.store, &f.root, &Fixture::completeness_request()).unwrap();
    assert!(!report.complete && !report.work_state_complete);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.code == "workstream_inactive")
    );
    assert!(!f.compile().completeness.work_state_complete);
}

#[test]
fn global_branch_switch_cannot_redirect_workstream_defaults() {
    let mut f = Fixture::new();
    let own = f.start("API-1");
    let before = f.compile().work_context.unwrap();
    let foreign = f
        .store
        .create_branch(
            f.project,
            f.rev(),
            BranchDraft {
                name: "private-client-branch".into(),
                parent_branch_id: None,
                git_binding: None,
                actor: "fixture".into(),
                reason: "Explore client work".into(),
            },
        )
        .unwrap()
        .0;
    f.store
        .start_session(
            f.project,
            f.rev(),
            SessionDraft {
                work_item_key: Some("CLIENT-1".into()),
                agent_id: "client-explorer".into(),
                provider: "fixture".into(),
                model: "fixture".into(),
                branch_id: Some(foreign.id),
                claim: false,
                claim_ttl_ms: None,
            },
        )
        .unwrap();
    f.store
        .switch_branch(
            f.project,
            f.rev(),
            Some(foreign.id),
            "fixture",
            "Select client exploration",
        )
        .unwrap();
    let after = f.compile();
    assert_eq!(after.session_id, Some(own.id));
    let after = after.work_context.unwrap();
    assert_eq!(before.context_hash, after.context_hash);
    assert_eq!(before.rendered_context, after.rendered_context);
    assert!(matches!(
        compile_branch_context(&mut f.store, &f.root, &foreign.name, &Fixture::request()),
        Err(Error::Workstream(WorkstreamError::AccessDenied))
    ));
    assert!(matches!(
        check_completeness(
            &mut f.store,
            &f.root,
            &CompletenessRequest {
                branch_id: Some(foreign.id),
                ..Fixture::completeness_request()
            }
        ),
        Err(Error::Workstream(WorkstreamError::AccessDenied))
    ));
    let own_branch = f
        .store
        .create_branch(
            f.project,
            f.rev(),
            BranchDraft {
                name: "api-experiment".into(),
                parent_branch_id: None,
                git_binding: None,
                actor: "fixture".into(),
                reason: "Explore API work".into(),
            },
        )
        .unwrap()
        .0;
    let session = f
        .store
        .start_session(
            f.project,
            f.rev(),
            SessionDraft {
                work_item_key: Some("API-1".into()),
                agent_id: "api-explorer".into(),
                provider: "fixture".into(),
                model: "fixture".into(),
                branch_id: Some(own_branch.id),
                claim: false,
                claim_ttl_ms: None,
            },
        )
        .unwrap()
        .0
        .session;
    let scoped = compile_context(
        &mut f.store,
        &f.root,
        &ContextRequest {
            session_id: Some(session.id),
            ..Fixture::request()
        },
    )
    .unwrap();
    assert_eq!(scoped.session_id, Some(session.id));
    assert_eq!(
        scoped.work_context.unwrap().identity.branch_id,
        Some(own_branch.id)
    );
    assert_eq!(
        f.store.project(f.project).unwrap().current_branch_id,
        Some(foreign.id)
    );
}

#[test]
fn authority_and_reader_identity_invalidate_scoped_semantics() {
    let mut f = Fixture::new();
    let access = f.access(0);
    let first = compile_workstream_context(
        &mut f.store,
        &f.root,
        &Fixture::request(),
        &access,
        &Default::default(),
    )
    .unwrap()
    .work_context
    .unwrap();
    let mut other = access.clone();
    other.subject = "another-authorized-reader".into();
    let next = compile_workstream_context(
        &mut f.store,
        &f.root,
        &Fixture::request(),
        &other,
        &Default::default(),
    )
    .unwrap()
    .work_context
    .unwrap();
    assert_ne!(first.context_hash, next.context_hash);
    f.document["workstreams"]["definitions"][0]["authority_version"] = json!(2);
    f.save();
    assert!(matches!(
        compile_workstream_context(
            &mut f.store,
            &f.root,
            &Fixture::request(),
            &access,
            &Default::default()
        ),
        Err(Error::Workstream(WorkstreamError::StaleAuthority))
    ));
    let access = f.access(0);
    let current = compile_workstream_context(
        &mut f.store,
        &f.root,
        &Fixture::request(),
        &access,
        &Default::default(),
    )
    .unwrap()
    .work_context
    .unwrap();
    assert_ne!(first.context_hash, current.context_hash);
    assert_eq!(current.workstream_identity.unwrap().authority_version, 2);
}

#[test]
fn source_refresh_failure_blocks_all_scoped_context_entrypoints_without_private_diagnostics() {
    let mut f = Fixture::new();
    fs::write(f.root.join(".awr/project.toml"), format!("{MANIFEST}\n[[sources]]\ndomain='decisions'\nrole='supporting'\npath='PRIVATE_CLIENT_MISSING'\nadapter='markdown-directory-v1'\n")).unwrap();
    let compile = compile_context(&mut f.store, &f.root, &Fixture::request()).unwrap_err();
    let delta = context_delta(
        &mut f.store,
        &f.root,
        &DeltaContextRequest {
            work_item_key: Some("API-1".into()),
            ..Default::default()
        },
    )
    .unwrap_err();
    let completeness =
        check_completeness(&mut f.store, &f.root, &Fixture::completeness_request()).unwrap_err();
    let orientation = bootstrap(
        &mut f.store,
        &f.root,
        &BootstrapRequest {
            work_item_key: Some("API-1".into()),
            ..Default::default()
        },
    )
    .unwrap_err();
    for error in [compile, delta, completeness, orientation] {
        assert!(matches!(error, Error::ContextIncomplete(_)), "{error}");
        assert!(!error.to_string().contains("PRIVATE_CLIENT_MISSING"));
    }
}

#[test]
fn bootstrap_uses_scope_selection_and_retains_unknown_hard_obligations() {
    let mut f = Fixture::new();
    let own = f.start("API-1");
    let foreign = f.start("CLIENT-1");
    f.store
        .create_checkpoint(
            f.project,
            f.rev(),
            foreign.id,
            CheckpointDraft {
                context_hash: "b".repeat(64),
                digest: "PRIVATE_CLIENT_DIGEST".into(),
                next_action: "PRIVATE_CLIENT_NEXT".into(),
                open_loops: vec![],
                changed_entities: vec![],
            },
        )
        .unwrap();
    fs::write(
        f.root.join("rules.md"),
        format!("{RULES}\n# Unclassified obligation\n\nKeep the full unresolved obligation.\n"),
    )
    .unwrap();
    let pack = f.bootstrap();
    assert!(!pack.context.complete);
    assert!(!pack.context.execution_context_complete);
    assert_eq!(pack.context.session.unwrap().id, own.id);
    assert!(pack.workstream_identity.is_some());
    assert!(
        pack.rendered_context
            .contains("Keep the full unresolved obligation.")
    );
    assert!(!pack.rendered_context.contains("PRIVATE_CLIENT"));
    assert!(pack.context.checkpoint.is_none());
    assert!(matches!(
        bootstrap(&mut f.store, &f.root, &BootstrapRequest::default()),
        Err(Error::Workstream(WorkstreamError::ScopeRequired))
    ));
}

#[test]
fn moved_back_work_cannot_reuse_prior_context_sessions_checkpoints_or_events() {
    let mut f = Fixture::new();
    let session = f.start("API-1");
    let before = f.compile().work_context.unwrap();
    let checkpoint = f
        .store
        .create_checkpoint(
            f.project,
            f.rev(),
            session.id,
            CheckpointDraft {
                context_hash: before.context_hash.clone(),
                digest: "OLD_OWNERSHIP_DIGEST".into(),
                next_action: "OLD_OWNERSHIP_NEXT".into(),
                open_loops: vec![],
                changed_entities: vec![],
            },
        )
        .unwrap()
        .0;
    let mut event = EventDraft::new("report.observed", "OLD_OWNERSHIP_EVENT");
    event.session_id = Some(session.id);
    event.importance = "critical".into();
    f.store.append_event(f.project, f.rev(), event).unwrap();
    f.store
        .end_session(f.project, f.rev(), session.id, SessionOutcome::Ended)
        .unwrap();
    f.move_api("client");
    f.document["work_items"][0]["next_action"] = json!("Current source action");
    f.document["work_items"][0]["depends_on"] = json!(["CLIENT-1"]);
    f.save();
    index_project(
        &mut f.store,
        &f.root,
        &Manifest::load(&f.root).unwrap(),
        false,
    )
    .unwrap();
    f.document["work_items"][0]["depends_on"] = json!([]);
    f.save();
    index_project(
        &mut f.store,
        &f.root,
        &Manifest::load(&f.root).unwrap(),
        false,
    )
    .unwrap();
    f.move_api("api");
    let delta = context_delta(
        &mut f.store,
        &f.root,
        &DeltaContextRequest {
            work_item_key: Some("API-1".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        !delta
            .delta
            .events
            .source_changes
            .iter()
            .flat_map(|s| &s.changed_entities)
            .any(|c| c.kind == "edges" || (c.kind == "work_items" && c.external_key == "API-1"))
    );
    let current = f.compile();
    assert!(current.checkpoint_id.is_none());
    assert!(current.session_id.is_none());
    assert!(!current.rendered_context().contains("OLD_OWNERSHIP"));
    let pack = current.work_context.unwrap();
    assert_ne!(pack.context_hash, before.context_hash);
    assert_eq!(
        pack.workstream_identity.unwrap().ownership_revision,
        before.workstream_identity.unwrap().ownership_revision + 2
    );
    let orientation = f.bootstrap();
    assert!(orientation.context.checkpoint.is_none());
    assert!(!orientation.rendered_context.contains("OLD_OWNERSHIP"));
    assert!(matches!(
        compile_context(
            &mut f.store,
            &f.root,
            &ContextRequest {
                session_id: Some(session.id),
                ..Fixture::request()
            }
        ),
        Err(Error::Workstream(WorkstreamError::BindingMismatch))
    ));
    assert!(matches!(
        compile_context(
            &mut f.store,
            &f.root,
            &ContextRequest {
                delta_baseline: DeltaBaseline::Checkpoint { id: checkpoint.id },
                ..Fixture::request()
            }
        ),
        Err(Error::Workstream(WorkstreamError::BindingMismatch))
    ));
}

#[test]
fn scoped_budget_preserves_required_facts_at_the_exact_boundary() {
    let mut f = Fixture::new();
    let full = f.compile().work_context.unwrap();
    let required = full.required_tokens;
    let exact = compile_context(
        &mut f.store,
        &f.root,
        &ContextRequest {
            token_budget: required,
            ..Fixture::request()
        },
    )
    .unwrap()
    .work_context
    .unwrap();
    assert_eq!(exact.token_estimate, token_count(&exact.rendered_context));
    assert!(exact.token_estimate <= required);
    for fact in [
        "Preserve approved requirements.",
        "API contract is verified",
        "Implement the API",
    ] {
        assert!(exact.rendered_context.contains(fact));
    }
    assert!(matches!(
        compile_context(
            &mut f.store,
            &f.root,
            &ContextRequest {
                token_budget: required - 1,
                ..Fixture::request()
            }
        ),
        Err(Error::BudgetExceeded { .. })
    ));
}
