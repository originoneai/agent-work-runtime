use awr_core::*;
use serde_json::json;

fn id(value: u128) -> Id {
    Id::from(value)
}

fn opaque(value: u128) -> String {
    id(value).to_string()
}

fn stream(number: u128, key: &str) -> Workstream {
    Workstream {
        id: id(number),
        project_id: opaque(1),
        external_key: key.into(),
        title: key.into(),
        state: WorkstreamState::Active,
        authority_version: 1,
        goal_keys: vec!["deliver".into()],
        acceptance_contracts: vec!["contract-v1".into()],
    }
}

fn catalog() -> WorkstreamCatalog {
    WorkstreamCatalog {
        version: WORKSTREAM_CATALOG_VERSION,
        project_id: opaque(1),
        legacy_default: Some(id(10)),
        workstreams: vec![stream(10, "api"), stream(20, "client")],
    }
}

fn grant(number: u128) -> WorkstreamGrant {
    WorkstreamGrant {
        workstream_id: id(number),
        authority_version: 1,
        read: true,
        write: true,
        manage: false,
    }
}

fn access() -> WorkstreamAccess {
    WorkstreamAccess {
        project_id: opaque(1),
        subject: "builder".into(),
        grants: vec![grant(10), grant(20)],
    }
}

fn work(number: u128, workstream: u128) -> WorkstreamWorkBinding {
    WorkstreamWorkBinding {
        project_id: opaque(1),
        workstream_id: id(workstream),
        work_item_id: opaque(number),
    }
}

fn read(selection: WorkstreamSelection) -> WorkstreamResult<ResolvedWorkstream> {
    resolve_workstream(&catalog(), &access(), &selection, WorkstreamAction::Read)
}

#[test]
fn legacy_identity_is_stable_project_scoped_and_confers_no_access() {
    let legacy = WorkstreamCatalog::legacy(opaque(1)).unwrap();
    assert_eq!(legacy, WorkstreamCatalog::legacy(opaque(1)).unwrap());
    assert_ne!(
        legacy.legacy_default,
        WorkstreamCatalog::legacy(opaque(2)).unwrap().legacy_default
    );
    assert_eq!(legacy.workstreams[0].external_key, "main");
    assert!(WorkstreamCatalog::legacy("").is_err());
    let mut permission = access();
    permission.grants.clear();
    assert_eq!(
        resolve_workstream(
            &legacy,
            &permission,
            &WorkstreamSelection::default(),
            WorkstreamAction::Write
        ),
        Err(WorkstreamError::AccessDenied)
    );
    permission.grants.push(WorkstreamGrant {
        workstream_id: legacy.legacy_default.unwrap(),
        ..grant(10)
    });
    let selected = resolve_workstream(
        &legacy,
        &permission,
        &WorkstreamSelection::default(),
        WorkstreamAction::Write,
    )
    .unwrap();
    assert_eq!(Some(selected.workstream_id), legacy.legacy_default);
    assert_eq!(selected.basis, WorkstreamSelectionBasis::UniqueAuthorized);
}

#[test]
fn unknown_versions_and_fields_cannot_silently_drop_scope_constraints() {
    let mut changed = catalog();
    changed.version += 1;
    assert_eq!(
        changed.validate(),
        Err(WorkstreamError::UnsupportedVersion(2))
    );
    let mut value = serde_json::to_value(catalog()).unwrap();
    value["permit_unknown_scopes"] = json!(true);
    assert!(serde_json::from_value::<WorkstreamCatalog>(value).is_err());
    let mut value = serde_json::to_value(stream(10, "api")).unwrap();
    value["grants"] = json!(["*"]);
    assert!(serde_json::from_value::<Workstream>(value).is_err());
}

#[test]
fn catalog_rejects_aliases_missing_defaults_and_cross_project_identity() {
    for bad in ["", " api", "../api", "api/client", "api*", "api\n", "主线"] {
        assert!(stream(10, bad).validate().is_err(), "{bad:?}");
    }
    let mut changed = catalog();
    changed.workstreams[1].id = id(10);
    assert!(changed.validate().is_err());
    let mut changed = catalog();
    changed.workstreams[1].external_key = "api".into();
    assert!(changed.validate().is_err());
    let mut changed = catalog();
    changed.legacy_default = Some(id(30));
    assert!(changed.validate().is_err());
    let mut changed = catalog();
    changed.workstreams[1].project_id = opaque(2);
    assert_eq!(changed.validate(), Err(WorkstreamError::BindingMismatch));
}

#[test]
fn source_ownership_has_exact_coverage_without_duplicate_shared_work() {
    let works = [opaque(100), opaque(200)];
    let bindings = [work(100, 10), work(200, 20)];
    validate_workstream_ownership(&catalog(), &works, &bindings).unwrap();
    assert!(validate_workstream_ownership(&catalog(), &works, &bindings[..1]).is_err());
    assert!(validate_workstream_ownership(&catalog(), &works[..1], &bindings).is_err());
    assert!(
        validate_workstream_ownership(&catalog(), &[opaque(100)], &[work(100, 10), work(100, 20)])
            .is_err()
    );
    assert!(validate_workstream_ownership(&catalog(), &[opaque(100)], &[work(100, 99)]).is_err());
    let mut foreign = work(100, 10);
    foreign.project_id = opaque(2);
    assert!(validate_workstream_ownership(&catalog(), &[opaque(100)], &[foreign]).is_err());
}

#[test]
fn multiple_authorized_scopes_require_selection_even_with_a_legacy_default() {
    assert_eq!(
        read(WorkstreamSelection::default()),
        Err(WorkstreamError::ScopeRequired)
    );
    let mut permission = access();
    permission.grants.retain(|g| g.workstream_id == id(20));
    let selected = resolve_workstream(
        &catalog(),
        &permission,
        &WorkstreamSelection::default(),
        WorkstreamAction::Read,
    )
    .unwrap();
    assert_eq!(selected.workstream_id, id(20));
    assert_eq!(selected.basis, WorkstreamSelectionBasis::UniqueAuthorized);
}

#[test]
fn conversation_defaults_do_not_rebind_persistent_sessions() {
    let selected = read(WorkstreamSelection {
        session: Some(WorkstreamSessionBinding {
            work: work(100, 10),
            session_id: opaque(1000),
        }),
        conversation_default: Some(id(20)),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(selected.workstream_id, id(10));
    assert_eq!(selected.work_item_id, Some(opaque(100)));
    assert_eq!(selected.session_id, Some(opaque(1000)));
    assert_eq!(selected.basis, WorkstreamSelectionBasis::Session);
    let other = read(WorkstreamSelection {
        conversation_default: Some(id(20)),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(other.workstream_id, id(20));
    assert_eq!(selected.workstream_id, id(10));
}

#[test]
fn explicit_work_session_and_project_disagreements_are_rejected() {
    for selection in [
        WorkstreamSelection {
            explicit: Some(id(20)),
            work: Some(work(100, 10)),
            ..Default::default()
        },
        WorkstreamSelection {
            work: Some(work(101, 10)),
            session: Some(WorkstreamSessionBinding {
                work: work(100, 10),
                session_id: opaque(1000),
            }),
            ..Default::default()
        },
        WorkstreamSelection {
            work: Some(work(100, 20)),
            session: Some(WorkstreamSessionBinding {
                work: work(100, 10),
                session_id: opaque(1000),
            }),
            ..Default::default()
        },
        WorkstreamSelection {
            session: Some(WorkstreamSessionBinding {
                work: work(100, 10),
                session_id: String::new(),
            }),
            ..Default::default()
        },
        WorkstreamSelection {
            work: Some(WorkstreamWorkBinding {
                project_id: opaque(2),
                ..work(100, 10)
            }),
            ..Default::default()
        },
    ] {
        assert_eq!(read(selection), Err(WorkstreamError::BindingMismatch));
    }
}

#[test]
fn denied_known_and_unknown_scopes_have_the_same_non_disclosing_error() {
    let mut permission = access();
    permission.grants = vec![grant(10)];
    for number in [20, 30] {
        assert_eq!(
            resolve_workstream(
                &catalog(),
                &permission,
                &WorkstreamSelection {
                    explicit: Some(id(number)),
                    ..Default::default()
                },
                WorkstreamAction::Read
            ),
            Err(WorkstreamError::AccessDenied)
        );
    }
    permission.project_id = opaque(2);
    assert_eq!(
        resolve_workstream(
            &catalog(),
            &permission,
            &WorkstreamSelection {
                explicit: Some(id(10)),
                ..Default::default()
            },
            WorkstreamAction::Read
        ),
        Err(WorkstreamError::AccessDenied)
    );
}

#[test]
fn read_grants_do_not_allow_mutation_or_scope_administration() {
    let mut permission = access();
    permission.grants[0].write = false;
    permission
        .authorize(&catalog(), id(10), WorkstreamAction::Read)
        .unwrap();
    for action in [WorkstreamAction::Write, WorkstreamAction::Manage] {
        assert_eq!(
            permission.authorize(&catalog(), id(10), action),
            Err(WorkstreamError::AccessDenied)
        );
    }
    permission.grants[0].read = false;
    permission.grants[0].manage = true;
    assert!(permission.validate().is_err());
    permission = access();
    permission.grants.push(grant(10));
    assert!(permission.validate().is_err());
}

#[test]
fn lifecycle_and_stale_permissions_never_silently_select_another_scope() {
    let mut changed = catalog();
    changed.workstreams[0].state = WorkstreamState::Paused;
    changed.workstreams[0].authority_version = 2;
    assert_eq!(
        resolve_workstream(
            &changed,
            &access(),
            &WorkstreamSelection::default(),
            WorkstreamAction::Write
        ),
        Err(WorkstreamError::ScopeRequired)
    );
    assert_eq!(
        access().authorize(&changed, id(10), WorkstreamAction::Read),
        Err(WorkstreamError::StaleAuthority)
    );
    // The unrelated scope's valid permission remains usable.
    access()
        .authorize(&changed, id(20), WorkstreamAction::Write)
        .unwrap();
    let mut fresh = access();
    fresh.grants[0].authority_version = 2;
    fresh
        .authorize(&changed, id(10), WorkstreamAction::Read)
        .unwrap();
    assert_eq!(
        fresh.authorize(&changed, id(10), WorkstreamAction::Write),
        Err(WorkstreamError::Inactive)
    );
    fresh.grants[0].manage = true;
    fresh
        .authorize(&changed, id(10), WorkstreamAction::Manage)
        .unwrap();
}

#[test]
fn display_changes_preserve_identity_but_authority_changes_require_new_versions() {
    let previous = stream(10, "api");
    let mut changed = previous.clone();
    changed.title = "API contract delivery".into();
    changed.validate_successor(&previous).unwrap();
    changed.state = WorkstreamState::Paused;
    assert_eq!(
        changed.validate_successor(&previous),
        Err(WorkstreamError::StaleAuthority)
    );
    changed.authority_version += 1;
    changed.validate_successor(&previous).unwrap();
    changed.external_key = "renamed-key".into();
    assert_eq!(
        changed.validate_successor(&previous),
        Err(WorkstreamError::BindingMismatch)
    );
    let mut previous = previous;
    previous.goal_keys.push("integrate".into());
    let mut reordered = previous.clone();
    reordered.goal_keys.reverse();
    reordered.validate_successor(&previous).unwrap();
}

#[test]
fn domain_errors_keep_specific_codes_at_existing_error_boundaries() {
    let error: Error = WorkstreamError::ScopeRequired.into();
    assert_eq!(error.code(), "WorkstreamScopeRequired");
    assert!(!error.to_string().contains("api"));
}

#[test]
fn legacy_team_project_work_and_session_ids_round_trip_without_remapping() {
    let catalog = WorkstreamCatalog::legacy("team/project:研发 ").unwrap();
    let scope = catalog.legacy_default.unwrap();
    let binding = WorkstreamSessionBinding {
        work: WorkstreamWorkBinding {
            project_id: catalog.project_id.clone(),
            workstream_id: scope,
            work_item_id: "release/api-23 ".into(),
        },
        session_id: "client:session-7".into(),
    };
    let encoded = serde_json::to_string(&binding).unwrap();
    let decoded: WorkstreamSessionBinding = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, binding);
    let encoded_catalog = serde_json::to_value(&catalog).unwrap();
    assert_eq!(
        serde_json::from_value::<WorkstreamCatalog>(encoded_catalog).unwrap(),
        catalog
    );
    validate_workstream_ownership(
        &catalog,
        &[binding.work.work_item_id.clone()],
        std::slice::from_ref(&binding.work),
    )
    .unwrap();
    let selected = resolve_workstream(
        &catalog,
        &WorkstreamAccess {
            project_id: catalog.project_id.clone(),
            subject: "builder".into(),
            grants: vec![WorkstreamGrant {
                workstream_id: scope,
                ..grant(10)
            }],
        },
        &WorkstreamSelection {
            session: Some(decoded),
            ..Default::default()
        },
        WorkstreamAction::Write,
    )
    .unwrap();
    assert_eq!(selected.project_id, "team/project:研发 ");
    assert_eq!(selected.work_item_id.as_deref(), Some("release/api-23 "));
    assert_eq!(selected.session_id.as_deref(), Some("client:session-7"));
    assert_ne!(
        scope,
        WorkstreamCatalog::legacy("team/project:研发")
            .unwrap()
            .legacy_default
            .unwrap()
    );
}

#[test]
fn invalid_opaque_identities_are_rejected_at_resolution_and_import() {
    for bad in [String::new(), "bad\nidentity".into(), "x".repeat(129)] {
        assert!(WorkstreamCatalog::legacy(bad.clone()).is_err());
        let mut binding = work(100, 10);
        binding.work_item_id = bad.clone();
        assert!(
            validate_workstream_ownership(&catalog(), &[bad.clone()], &[binding.clone()]).is_err()
        );
        assert_eq!(
            read(WorkstreamSelection {
                work: Some(binding),
                ..Default::default()
            }),
            Err(WorkstreamError::BindingMismatch)
        );
        assert_eq!(
            read(WorkstreamSelection {
                session: Some(WorkstreamSessionBinding {
                    work: work(100, 10),
                    session_id: bad,
                }),
                ..Default::default()
            }),
            Err(WorkstreamError::BindingMismatch)
        );
    }
}
