//! One host action orchestrates existing source writers; provenance confers no authority.
use crate::mutation_apply::{directory, named_lock, new_file, recovery_root};
use crate::{DocumentRequest, ReviewProposalAction, ReviewProposalRequest};
use awr_core::*;
use awr_source::*;
use awr_store::Store;
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostSaveRequest {
    pub version: u32,
    pub request_key: String,
    pub actor: HostActor,
    pub reason: String,
    pub change: HostChange,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostChange {
    Fields {
        kind: EntityKind,
        target: String,
        source_fingerprint: String,
        fields: Value,
    },
    ActivateDraft {
        work: String,
        source_fingerprint: String,
    },
    ConfirmOrdinary {
        work: String,
        source_fingerprint: String,
        policy_fingerprint: String,
        kind: OrdinaryCompletionKind,
        basis: String,
        confirmed_at: i64,
        artifacts: Vec<OrdinaryArtifact>,
    },
    Document {
        change: DocumentAction,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
enum Backend {
    Fields {
        patch: MutationPatch,
        before_text: String,
        after_text: String,
    },
    Document {
        input: DocumentRequest,
        preview: Value,
    },
    NoChange,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    version: u32,
    project_id: Id,
    root: PathBuf,
    request: HostSaveRequest,
    request_hash: String,
    project_revision: Revision,
    backend: Backend,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    plan: Plan,
    preview_fingerprint: String,
    phase: String,
    project_revision: Revision,
    proposal_id: Option<Id>,
    outcome: Option<Value>,
}
pub struct HostSaveReport {
    pub value: Value,
    pub failure: Option<Error>,
}
fn hash(value: &impl Serialize) -> Result<String> {
    Ok(fingerprint(&serde_json::to_vec(value)?)
        .trim_start_matches("sha256:")
        .into())
}
fn name(project: Id, key: &str) -> Result<String> {
    if key.trim().is_empty() || key.len() > 512 || key.chars().any(char::is_control) {
        return Err(Error::InvalidInput(
            "host request key requires 1..512 bytes without controls".into(),
        ));
    }
    ensure_public_text(key)?;
    Ok(format!("host-save-{}", hash(&(project, key))?))
}
fn validate(r: &HostSaveRequest) -> Result<()> {
    ensure_public_data(r)?;
    r.actor.validate()?;
    if r.version != 1
        || r.request_key.trim().is_empty()
        || r.request_key.len() > 512
        || r.request_key.chars().any(char::is_control)
        || r.reason.trim().is_empty()
        || r.reason.len() > 4096
        || serde_json::to_vec(r)?.len() > MARKDOWN_READ_CAP as usize
    {
        return Err(Error::InvalidInput(
            "host save needs version 1, bounded request identity, reason and one source change"
                .into(),
        ));
    }
    Ok(())
}
fn load(root: &Path, project: Id, key: &str) -> Result<Option<Receipt>> {
    let base = root.join(".awr/mutations").join(name(project, key)?);
    match std::fs::symlink_metadata(&base) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
        Ok(m) if !m.is_dir() || m.file_type().is_symlink() => {
            return Err(Error::RuleViolation(
                "host save receipt directory is invalid".into(),
            ));
        }
        _ => (),
    }
    let r: Receipt = serde_json::from_slice(&read_capped(
        &base.join("receipt.json"),
        MARKDOWN_READ_CAP * 5,
    )?)?;
    if r.plan.version != 1
        || r.plan.project_id != project
        || r.plan.root != root
        || r.plan.request.request_key != key
        || r.plan.request_hash != hash(&(project, &r.plan.request))?
        || r.preview_fingerprint != hash(&r.plan)?
        || !["prepared", "completed", "no_change"].contains(&r.phase.as_str())
    {
        return Err(Error::SourceConflict(
            "host save receipt has a different project, request or plan".into(),
        ));
    }
    validate(&r.plan.request)?;
    Ok(Some(r))
}
fn save(dir: &Dir, r: &Receipt) -> Result<()> {
    let name = format!("receipt-{}.tmp", Id::new());
    let mut f = new_file(dir, OsStr::new(&name))?;
    f.write_all(&serde_json::to_vec_pretty(r)?)?;
    f.sync_all()?;
    drop(f);
    dir.rename(&name, dir, "receipt.json")?;
    crate::fs_sync::sync_directory(dir)
}
fn summary(r: &Receipt, replay: bool) -> Result<Value> {
    let stopped = r.phase == "prepared"
        && r.outcome.as_ref().is_some_and(|v| {
            matches!(
                v["proposal"]["status"].as_str(),
                Some("failed" | "conflict" | "rejected")
            )
        });
    Ok(
        json!({"ok":r.phase!="prepared","status":if stopped{"requires_review"}else if r.phase=="prepared"{"pending_recovery"}else{r.phase.as_str()},"project_id":r.plan.project_id,
        "project_revision":r.project_revision,"request_key":r.plan.request.request_key,"actor":r.plan.request.actor,"provenance_is_authentication":false,
        "proposal_id":r.proposal_id,"preview_fingerprint":r.preview_fingerprint,"already_recorded":replay,"historical_outcome":replay,
        "source_write_performed":false,"runtime_write_performed":false,"write_outcome":if stopped{"stopped"}else if r.phase=="prepared"{"pending_recovery"}else if r.phase=="no_change"{"no_change"}else{"applied"},
        "outcome":r.outcome,"recovery_directory":format!(".awr/mutations/{}",name(r.plan.project_id,&r.plan.request.request_key)?)}),
    )
}
fn preview_fields(
    store: &Store,
    root: &Path,
    project: Id,
    request: &HostSaveRequest,
    kind: EntityKind,
    target: &str,
    source_fp: &str,
    changes: Value,
    action: HostEditAction,
) -> Result<Backend> {
    let target = store.mutation_target(project, kind, target)?;
    if target.source.fingerprint != source_fp {
        return Err(Error::SourceConflict(
            "host editor source fingerprint is obsolete".into(),
        ));
    }
    let patch = MutationPatch {
        version: 1,
        target: MutationTarget {
            kind,
            meta: serde_json::from_value(target.item)?,
        },
        source_config: target.source.config.clone(),
        intent: request.reason.clone(),
        changes,
        work_action: None,
        host_edit: Some(HostEditBinding {
            version: 1,
            request_key: request.request_key.clone(),
            request_hash: hash(&(project, request))?,
            actor: request.actor.clone(),
            action,
        }),
    };
    patch.validate()?;
    verify_mutation_source(root, &target.source, &patch)?;
    if action == HostEditAction::Fields {
        if patch
            .changes
            .as_object()
            .unwrap()
            .keys()
            .any(|k| !yaml_field_writable(kind, k))
        {
            return Err(Error::MutationUnsupported(
                "host field saves retain the existing field and lifecycle restrictions".into(),
            ));
        }
        let record = read_yaml_mutation_record(root, &target.source, &patch)?;
        if patch
            .changes
            .as_object()
            .unwrap()
            .iter()
            .all(|(k, v)| record.get(k) == Some(v))
        {
            return Ok(Backend::NoChange);
        }
    }
    let proposal = MutationProposal {
        id: Id::new(),
        project_id: project,
        work_item_id: (kind == EntityKind::WorkItem).then_some(patch.target.meta.id),
        source_id: target.source.id,
        base_fingerprint: source_fp.into(),
        expected_revision: store.project(project)?.project_revision,
        mutation_type: patch.mutation_type().into(),
        patch: serde_json::to_value(&patch)?,
        status: ProposalStatus::Draft,
        created_by_session: None,
        revision: 1,
    };
    store.check_work_proposal(project, &proposal)?;
    crate::work_action::verify_work_dependencies(store, root, project, &patch)?;
    let prepared = prepare_yaml_mutation(
        root,
        &target.source,
        &proposal,
        store.projection_ids(&target.source)?,
    )?;
    if open_file_exact(&prepared.path)?
        .metadata()?
        .permissions()
        .readonly()
    {
        return Err(Error::RuleViolation("host source is read-only".into()));
    }
    Ok(Backend::Fields {
        patch,
        before_text: prepared.before.text()?.into(),
        after_text: prepared.after.text()?.into(),
    })
}
fn prepare(store: &mut Store, root: &Path, request: HostSaveRequest) -> Result<Plan> {
    validate(&request)?;
    let project = store.project_by_root(root)?;
    let backend = match &request.change {
        HostChange::Fields {
            kind,
            target,
            source_fingerprint,
            fields,
        } => preview_fields(
            store,
            root,
            project.id,
            &request,
            *kind,
            target,
            source_fingerprint,
            fields.clone(),
            HostEditAction::Fields,
        )?,
        HostChange::ActivateDraft {
            work,
            source_fingerprint,
        } => preview_fields(
            store,
            root,
            project.id,
            &request,
            EntityKind::WorkItem,
            work,
            source_fingerprint,
            json!({"status":"planned"}),
            HostEditAction::ActivateDraft,
        )?,
        HostChange::ConfirmOrdinary {
            work,
            source_fingerprint,
            policy_fingerprint,
            kind,
            basis,
            confirmed_at,
            artifacts,
        } => {
            let current = store.work_item(project.id, work)?;
            let policy =
                OrdinaryWorkPolicy::from_config(&current.source.config)?.ok_or_else(|| {
                    Error::RuleViolation(
                        "this work retains the strict engineering completion policy".into(),
                    )
                })?;
            let receipt = OrdinaryCompletion {
                version: 1,
                request_key: request.request_key.clone(),
                policy,
                policy_fingerprint: policy_fingerprint.clone(),
                kind: kind.clone(),
                actor: request.actor.clone(),
                basis: basis.clone(),
                confirmed_at: *confirmed_at,
                acceptance: current.item.acceptance.clone(),
                artifacts: artifacts.clone(),
            };
            preview_fields(
                store,
                root,
                project.id,
                &request,
                EntityKind::WorkItem,
                work,
                source_fingerprint,
                json!({"status":"completed","ordinary_completion":receipt}),
                HostEditAction::ConfirmOrdinary,
            )?
        }
        HostChange::Document { change } => {
            let unchanged = if let DocumentAction::Edit {
                source_id,
                source_fingerprint,
                edit,
            } = change
            {
                let source = store.source(project.id, *source_id)?;
                let p = prepare_document_edit(
                    root,
                    &source,
                    source_fingerprint,
                    edit,
                    store.projection_ids(&source)?,
                )?;
                p.before
                    .as_ref()
                    .is_some_and(|s| s.fingerprint == p.after.fingerprint)
            } else {
                false
            };
            if unchanged {
                Backend::NoChange
            } else {
                let input = DocumentRequest {
                    version: 1,
                    request_key: format!("host/{}", hash(&(project.id, &request.request_key))?),
                    change: change.clone(),
                };
                let preview =
                    crate::change_document(store, root, input.clone(), false, None, None)?;
                if let Some(e) = preview.failure {
                    return Err(e);
                }
                Backend::Document {
                    input,
                    preview: preview.value,
                }
            }
        }
    };
    Ok(Plan {
        version: 1,
        project_id: project.id,
        root: root.into(),
        request_hash: hash(&(project.id, &request))?,
        request,
        project_revision: store.project(project.id)?.project_revision,
        backend,
    })
}
pub fn host_preview(
    store: &mut Store,
    root: &Path,
    request: HostSaveRequest,
) -> Result<HostSaveReport> {
    let root = root.canonicalize()?;
    let plan = prepare(store, &root, request)?;
    Ok(HostSaveReport {
        value: json!({"ok":true,"status":"preview","project_revision":plan.project_revision,"preview":{"fingerprint":hash(&plan)?,"plan":plan},"source_write_performed":false}),
        failure: None,
    })
}
pub fn host_status(store: &Store, root: &Path, key: &str) -> Result<HostSaveReport> {
    let root = root.canonicalize()?;
    let project = store.project_by_root(&root)?;
    let value = if let Some(r) = load(&root, project.id, key)? {
        let mut value = summary(&r, true)?;
        value["found"] = json!(true);
        value["read_only"] = json!(true);
        if let Some(p) = store.host_proposal(project.id, key)? {
            if r.phase == "prepared"
                && matches!(
                    p.status,
                    ProposalStatus::Failed | ProposalStatus::Conflict | ProposalStatus::Rejected
                )
            {
                value["status"] = json!("requires_review");
                value["write_outcome"] = json!("stopped");
            }
            value["current_proposal"] = json!(p)
        }
        if let Backend::Fields {
            patch,
            before_text,
            after_text,
        } = &r.plan.backend
        {
            let observed = (|| -> Result<String> {
                let source = store.source(project.id, patch.target.meta.source_ref.source_id)?;
                if source.config != patch.source_config {
                    return Err(Error::SourceConflict("source mapping changed".into()));
                }
                Ok(inspect_registered_source(&root, &source)?.2.fingerprint)
            })();
            value["current_source"] = json!(match observed {
                Ok(fp) if fp == fingerprint(after_text.as_bytes()) => "after",
                Ok(fp) if fp == fingerprint(before_text.as_bytes()) => "before",
                Ok(_) => "externally_changed",
                Err(_) => "unavailable_or_registration_changed",
            });
        }
        if let Backend::Document { input, .. } = &r.plan.backend {
            value["current_document"] =
                crate::document_status(store, &root, &input.request_key)?.value
        }
        value
    } else {
        json!({"ok":true,"found":false,"read_only":true,"source_write_performed":false,"runtime_write_performed":false})
    };
    Ok(HostSaveReport {
        value,
        failure: None,
    })
}
/// A human Save already selects exact fields/source bytes. AI application additionally
/// requires the fingerprint of the complete preview actually shown by the host.
pub fn host_save(
    store: &mut Store,
    root: &Path,
    request: HostSaveRequest,
    expected: Revision,
    reviewed: Option<&str>,
) -> Result<HostSaveReport> {
    let root = root.canonicalize()?;
    validate(&request)?;
    let project = store.project_by_root(&root)?;
    let name = name(project.id, &request.request_key)?;
    let _lock = named_lock(&root, &format!("{name}.lock"))?;
    if let Some(r) = load(&root, project.id, &request.request_key)? {
        if r.plan.request_hash != hash(&(project.id, &request))? {
            return Err(Error::SourceConflict(
                "host request identity belongs to a different edit or actor".into(),
            ));
        }
        return Ok(HostSaveReport {
            value: summary(&r, true)?,
            failure: (r.phase == "prepared").then(|| {
                Error::MutationConflict(
                    "inspect the existing host request and explicitly recover it".into(),
                )
            }),
        });
    }
    if project.project_revision != expected {
        return Err(Error::RevisionConflict {
            expected,
            actual: project.project_revision,
        });
    }
    let plan = prepare(store, &root, request)?;
    let fingerprint = hash(&plan)?;
    if plan.project_revision != expected {
        return Err(Error::RevisionConflict {
            expected,
            actual: plan.project_revision,
        });
    }
    if (plan.request.actor.origin != HostEditOrigin::Human || reviewed.is_some())
        && reviewed != Some(fingerprint.as_str())
    {
        return Err(Error::SourceConflict(
            "AI application requires the exact reviewed host preview".into(),
        ));
    }
    let mutations = recovery_root(&root)?;
    mutations.create_dir(&name)?;
    let dir = directory(&mutations, &name, &root.join(".awr/mutations").join(&name))?;
    let mut r = Receipt {
        plan,
        preview_fingerprint: fingerprint,
        phase: "prepared".into(),
        project_revision: expected,
        proposal_id: None,
        outcome: None,
    };
    save(&dir, &r)?;
    crate::fs_sync::sync_directory(&mutations)?;
    finish(store, &root, &dir, &mut r, expected, false)
}
pub fn host_recover(
    store: &mut Store,
    root: &Path,
    key: &str,
    expected: Revision,
) -> Result<HostSaveReport> {
    let root = root.canonicalize()?;
    let project = store.project_by_root(&root)?;
    let name = name(project.id, key)?;
    let _lock = named_lock(&root, &format!("{name}.lock"))?;
    let mut r =
        load(&root, project.id, key)?.ok_or_else(|| Error::NotFound("host save request".into()))?;
    if r.phase != "prepared" {
        return Ok(HostSaveReport {
            value: summary(&r, true)?,
            failure: None,
        });
    }
    let dir = open_dir_exact(&root.join(".awr/mutations").join(&name))?;
    finish(store, &root, &dir, &mut r, expected, true)
}
fn finish(
    store: &mut Store,
    root: &Path,
    dir: &Dir,
    r: &mut Receipt,
    expected: Revision,
    recovery: bool,
) -> Result<HostSaveReport> {
    let mut wrote = false;
    let result = (|| -> Result<()> {
        let actual = store.project(r.plan.project_id)?.project_revision;
        if actual != expected {
            return Err(Error::RevisionConflict { expected, actual });
        }
        match r.plan.backend.clone() {
            Backend::NoChange => {
                let current = prepare(store, root, r.plan.request.clone())?;
                if !matches!(current.backend, Backend::NoChange) {
                    return Err(Error::SourceConflict(
                        "unchanged save no longer matches current source facts".into(),
                    ));
                }
                r.phase = "no_change".into();
                save(dir, r)?;
            }
            Backend::Document { input, preview } => {
                let status = crate::document_status(store, root, &input.request_key)?;
                let report = if status.value["found"] == true {
                    if !recovery {
                        return Err(Error::MutationConflict(
                            "document request already exists; explicitly recover the host save"
                                .into(),
                        ));
                    }
                    crate::recover_document(store, root, &input.request_key, expected)?
                } else {
                    crate::change_document(
                        store,
                        root,
                        input,
                        true,
                        preview["preview"]["fingerprint"].as_str(),
                        Some(expected),
                    )?
                };
                wrote = report.value["source_write_performed"] == true;
                r.project_revision = store.project(r.plan.project_id)?.project_revision;
                r.outcome = Some(report.value);
                if let Some(e) = report.failure {
                    save(dir, r)?;
                    return Err(e);
                }
                r.phase = "completed".into();
                save(dir, r)?;
            }
            Backend::Fields { patch, .. } => {
                let project = r.plan.project_id;
                let mut proposal = if let Some(p) =
                    store.host_proposal(project, &r.plan.request.request_key)?
                {
                    if serde_json::to_value(p.bound_patch()?)? != serde_json::to_value(&patch)? {
                        return Err(Error::SourceConflict(
                            "saved host proposal differs from the reviewed patch".into(),
                        ));
                    }
                    p
                } else {
                    let source = store.source(project, patch.target.meta.source_ref.source_id)?;
                    verify_mutation_source(root, &source, &patch)?;
                    crate::work_action::verify_work_dependencies(store, root, project, &patch)?;
                    store
                        .create_proposal(
                            project,
                            expected,
                            MutationDraft {
                                source_id: source.id,
                                base_fingerprint: source.fingerprint,
                                mutation_type: patch.mutation_type().into(),
                                patch: patch.clone(),
                                created_by_session: None,
                            },
                        )?
                        .0
                };
                r.proposal_id = Some(proposal.id);
                r.project_revision = store.project(project)?.project_revision;
                save(dir, r)?;
                loop {
                    let action=match proposal.status {
                        ProposalStatus::Draft=>ReviewProposalAction::Submit,
                        ProposalStatus::Ready=>ReviewProposalAction::Approve,
                        ProposalStatus::Approved=>if store.proposal_apply_attempt(project,proposal.id)?.is_some(){ReviewProposalAction::Recover}else{ReviewProposalAction::Apply},
                        ProposalStatus::Applied=>break,
                        _=>return Err(Error::SourceConflict("host proposal is terminal without application; preserve it and review a new request".into())),
                    };
                    let report = crate::review_proposal(
                        store,
                        root,
                        &ReviewProposalRequest {
                            proposal_id: proposal.id,
                            expected_revision: r.project_revision,
                            action,
                            actor: format!(
                                "{}:{}",
                                r.plan.request.actor.host, r.plan.request.actor.subject
                            ),
                            reason: r.plan.request.reason.clone(),
                        },
                    )?;
                    wrote |= report.source_write_performed == Some(true);
                    r.project_revision = report.project_revision;
                    r.outcome = Some(serde_json::to_value(&report)?);
                    let failure = report.failure;
                    proposal = report.proposal;
                    save(dir, r)?;
                    if let Some(e) = failure {
                        return Err(e);
                    }
                }
                r.phase = "completed".into();
                save(dir, r)?;
            }
        }
        Ok(())
    })();
    let mut value = summary(r, false)?;
    value["source_write_performed"] = json!(wrote);
    value["runtime_write_performed"] = json!(true);
    let failure = result.err();
    if let Some(e) = &failure {
        value["ok"] = json!(false);
        if value["status"] != "requires_review" {
            value["status"] = json!("pending_recovery");
            value["write_outcome"] = json!("pending_recovery");
        }
        value["error"] = json!(e.report())
    }
    Ok(HostSaveReport { value, failure })
}
