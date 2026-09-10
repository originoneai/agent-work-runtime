//! Reviewed source batches. A durable journal distinguishes source writes from indexing.
use crate::mutation_apply::{
    SourceReplacement, directory, named_lock, named_lock_for_owner, new_file, recovery_root,
};
use awr_core::*;
use awr_source::*;
use awr_store::Store;
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub version: u32,
    pub request_key: String,
    pub actor: HostActor,
    pub reason: String,
    pub change: BatchChange,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BatchChange {
    Ledger {
        source_id: Id,
        source_fingerprint: String,
        operations: Vec<LedgerBatchOperation>,
    },
    Related {
        changes: Vec<RelatedChange>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum RelatedChange {
    Document {
        source_id: Id,
        source_fingerprint: String,
        edit: DocumentEdit,
    },
    Adopt {
        candidate: DocumentVersion,
        supersedes: Option<DocumentVersion>,
    },
    Ledger {
        source_id: Id,
        source_fingerprint: String,
        operations: Vec<LedgerBatchOperation>,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePlan {
    path: PathBuf,
    source: Source,
    spec: SourceSpec,
    before_fingerprint: String,
    after_fingerprint: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    version: u32,
    project_id: Id,
    root: PathBuf,
    project_revision: Revision,
    manifest_fingerprint: String,
    request: BatchRequest,
    files: Vec<FilePlan>,
    observations: Vec<Source>,
    archive_targets: Vec<Id>,
    outcomes: Vec<Value>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    plan: Plan,
    fingerprint: String,
    phase: String,
    project_revision: Revision,
    applied_files: Vec<usize>,
}
pub struct BatchReport {
    pub value: Value,
    pub failure: Option<Error>,
}
fn name(project: Id, key: &str) -> Result<String> {
    if key.trim().is_empty() || key.len() > 512 || key.chars().any(char::is_control) {
        return Err(Error::InvalidInput(
            "batch requires a bounded stable request key".into(),
        ));
    }
    Ok(format!(
        "batch-{}",
        fingerprint(&serde_json::to_vec(&(project, key))?).trim_start_matches("sha256:")
    ))
}
fn read(path: &Path, cap: u64) -> Result<Vec<u8>> {
    let f = open_file_exact(path)?;
    if !f.metadata()?.is_file() || f.metadata()?.len() > cap {
        return Err(Error::InvalidInput(
            "invalid or oversized batch file".into(),
        ));
    }
    let mut bytes = vec![];
    f.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(Error::InvalidInput("batch file exceeds bound".into()));
    }
    Ok(bytes)
}
fn manifest_hash(root: &Path) -> Result<String> {
    Ok(fingerprint(&read(
        &root.join(".awr/project.toml"),
        64 * 1024,
    )?))
}
fn load(root: &Path, project: Id, key: &str) -> Result<Option<Receipt>> {
    let base = root.join(".awr/mutations").join(name(project, key)?);
    match std::fs::symlink_metadata(&base) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
        Ok(m) if !m.is_dir() || m.file_type().is_symlink() => {
            return Err(Error::RuleViolation(
                "batch receipt directory must be real".into(),
            ));
        }
        _ => (),
    }
    let r: Receipt = serde_json::from_slice(&read(&base.join("receipt.json"), 64 * 1024 * 1024)?)?;
    if r.plan.version != 1
        || r.plan.request.version != 1
        || r.plan.project_id != project
        || r.plan.root != root
        || r.plan.request.request_key != key
        || fingerprint(&serde_json::to_vec(&r.plan)?) != r.fingerprint
        || !["prepared", "applied", "completed", "no_change"].contains(&r.phase.as_str())
        || r.plan.files.is_empty()
        || r.plan.files.len() > 100
    {
        return Err(Error::SourceConflict(
            "batch receipt differs from its bound project, request or version".into(),
        ));
    }
    Ok(Some(r))
}
fn save(dir: &Dir, r: &Receipt) -> Result<()> {
    let temp = format!("receipt-{}.tmp", Id::new());
    let mut f = new_file(dir, OsStr::new(&temp))?;
    f.write_all(&serde_json::to_vec_pretty(r)?)?;
    f.sync_all()?;
    drop(f);
    dir.rename(&temp, dir, "receipt.json")?;
    crate::fs_sync::sync_directory(dir)
}
fn done(r: &Receipt) -> bool {
    ["completed", "no_change"].contains(&r.phase.as_str())
}
fn report(r: &Receipt, replay: bool) -> Result<Value> {
    Ok(
        json!({"ok":done(r),"status":if r.phase=="no_change"{"no_change"}else if done(r){"completed"}else{"pending_recovery"},"phase":r.phase,"project_id":r.plan.project_id,"project_revision":r.project_revision,"request_key":r.plan.request.request_key,"actor":r.plan.request.actor,"reason":r.plan.request.reason,"preview_fingerprint":r.fingerprint,"outcomes":r.plan.outcomes,"applied_files":r.applied_files,"file_total":r.plan.files.len(),"filesystem_atomic":false,"partial_apply":!r.applied_files.is_empty() && !done(r),"already_recorded":replay,"historical_outcome":replay,"source_write_performed":false,"runtime_write_performed":false,"write_outcome":if r.phase=="no_change"{"no_change"}else if done(r){"applied"}else{"pending_recovery"},"recovery_directory":format!(".awr/mutations/{}",name(r.plan.project_id,&r.plan.request.request_key)?)}),
    )
}
fn observe(root: &Path, plan: &Plan) -> Result<Vec<String>> {
    if manifest_hash(root)? != plan.manifest_fingerprint {
        return Err(Error::SourceConflict(
            "batch source registration changed".into(),
        ));
    }
    for source in &plan.observations {
        if inspect_registered_source(root, source)?.2.fingerprint != source.fingerprint {
            return Err(Error::SourceConflict(
                "a dependency source changed since batch review".into(),
            ));
        }
    }
    plan.files
        .iter()
        .map(|f| {
            let (locator, spec, snapshot) = inspect_registered_source(root, &f.source)?;
            if !matches!(locator, Locator::File(ref path) if path == &f.path)
                || serde_json::to_value(spec)? != serde_json::to_value(&f.spec)?
            {
                return Err(Error::SourceConflict("batch source mapping changed".into()));
            }
            Ok(snapshot.fingerprint)
        })
        .collect()
}
fn lock_names(plan: &Plan) -> Vec<String> {
    let mut names = BTreeSet::new();
    for f in &plan.files {
        names.insert(format!("{}.lock", f.source.id));
        names.insert(format!(
            "document-path-{}.lock",
            fingerprint(f.path.to_string_lossy().as_bytes()).trim_start_matches("sha256:")
        ));
    }
    names.into_iter().collect()
}
fn reserve(root: &Path, plan: &Plan, owner: &str) -> Result<()> {
    let dir = recovery_root(root)?;
    for lock in lock_names(plan) {
        let file = lock.trim_end_matches(".lock").to_owned() + ".intent";
        match new_file(&dir, OsStr::new(&file)) {
            Ok(mut f) => {
                f.write_all(owner.as_bytes())?;
                f.sync_all()?;
            }
            Err(Error::Io(_)) if root.join(".awr/mutations").join(&file).exists() => {
                if read(&root.join(".awr/mutations").join(&file), 512)? != owner.as_bytes() {
                    return Err(Error::MutationConflict(
                        "another batch owns the source intent".into(),
                    ));
                }
            }
            Err(e) => return Err(e),
        }
    }
    crate::fs_sync::sync_directory(&dir)
}
fn release(root: &Path, plan: &Plan, owner: &str) -> Result<()> {
    let dir = recovery_root(root)?;
    for lock in lock_names(plan) {
        let file = lock.trim_end_matches(".lock").to_owned() + ".intent";
        let path = root.join(".awr/mutations").join(&file);
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
            Ok(_) => {
                if read(&path, 512)? != owner.as_bytes() {
                    return Err(Error::MutationConflict(
                        "batch cannot release another owner's intent".into(),
                    ));
                }
                dir.remove_file(&file)?;
            }
        }
    }
    crate::fs_sync::sync_directory(&dir)
}
pub fn batch_status(store: &Store, root: &Path, key: &str) -> Result<BatchReport> {
    let root = root.canonicalize()?;
    let p = store.project_by_root(&root)?;
    let value = if let Some(r) = load(&root, p.id, key)? {
        let mut v = report(&r, true)?;
        v["found"] = json!(true);
        v["read_only"] = json!(true);
        v["current_sources"] = match observe(&root, &r.plan) {
            Ok(states) => json!(
                states
                    .iter()
                    .zip(&r.plan.files)
                    .map(|(s, f)| if s == &f.after_fingerprint {
                        "after"
                    } else if s == &f.before_fingerprint {
                        "before"
                    } else {
                        "externally_changed"
                    })
                    .collect::<Vec<_>>()
            ),
            Err(_) => json!("unavailable_or_registration_changed"),
        };
        v
    } else {
        json!({"ok":true,"found":false,"read_only":true,"source_write_performed":false,"runtime_write_performed":false})
    };
    Ok(BatchReport {
        value,
        failure: None,
    })
}
fn validate_archive(
    store: &Store,
    project: Id,
    source: Id,
    prepared: &PreparedLedgerBatch,
) -> Result<()> {
    for id in &prepared.archive_targets {
        store.ensure_work_unoccupied(project, *id)?;
    }
    let mut works: BTreeMap<_, _> = store
        .work_items(project)?
        .into_iter()
        .filter(|w| w.item.meta.source_ref.source_id != source)
        .map(|w| (w.item.meta.external_key.clone(), w.item))
        .collect();
    works.extend(
        prepared
            .projection
            .work_items
            .iter()
            .cloned()
            .map(|w| (w.meta.external_key.clone(), w)),
    );
    let mut edges = store.work_dependency_links(project)?;
    edges.retain(|e| e.source_ref.source_id != source);
    edges.extend(
        prepared
            .projection
            .edges
            .iter()
            .filter(|e| e.relation == "depends_on" && e.to_kind == EntityKind::WorkItem)
            .cloned(),
    );
    for edge in edges {
        if works.get(&edge.from_key).is_some_and(|w| !w.archived)
            && works.get(&edge.to_key).is_some_and(|w| w.archived)
        {
            return Err(Error::RuleViolation(format!(
                "{} still depends on archived {}; remove that dependency or explicitly archive its dependent in the same batch",
                edge.from_key, edge.to_key
            )));
        }
    }
    Ok(())
}
pub fn change_batch(
    store: &mut Store,
    root: &Path,
    request: BatchRequest,
    accept: bool,
    expected_preview: Option<&str>,
    expected_revision: Option<Revision>,
) -> Result<BatchReport> {
    let root = root.canonicalize()?;
    let project = store.project_by_root(&root)?;
    let owner = name(project.id, &request.request_key)?;
    ensure_public_data(&request)?;
    request.actor.validate()?;
    if request.version != 1 || request.reason.trim().is_empty() || request.reason.len() > 4096 {
        return Err(Error::InvalidInput(
            "batch requires version 1 and a bounded reason".into(),
        ));
    }
    let _request_lock = if accept {
        Some(named_lock(&root, &format!("{owner}.lock"))?)
    } else {
        None
    };
    if let Some(r) = load(&root, project.id, &request.request_key)? {
        if r.plan.request != request {
            return Err(Error::SourceConflict(
                "batch key already belongs to different content or actor".into(),
            ));
        }
        return Ok(BatchReport {
            value: report(&r, true)?,
            failure: (!done(&r))
                .then(|| Error::MutationConflict("explicitly recover the pending batch".into())),
        });
    }
    if !index_project(store, &root, &Manifest::load(&root)?, false)?.ok {
        return Err(Error::SourceStale(
            "refresh all registered sources before a batch".into(),
        ));
    }
    let revision = store.project(project.id)?.project_revision;
    if let Some(expected) = expected_revision
        && expected != revision
    {
        return Err(Error::RevisionConflict {
            expected,
            actual: revision,
        });
    }
    let mut documents = Vec::new();
    let mut archive_targets = Vec::new();
    let mut outcomes = Vec::new();
    let mut ledger_count = 0;
    let changes = match &request.change {
        BatchChange::Ledger {
            source_id,
            source_fingerprint,
            operations,
        } => vec![RelatedChange::Ledger {
            source_id: *source_id,
            source_fingerprint: source_fingerprint.clone(),
            operations: operations.clone(),
        }],
        BatchChange::Related { changes } => changes.clone(),
    };
    if changes.is_empty() || changes.len() > 100 {
        return Err(Error::InvalidInput(
            "related batch requires 1..100 changes".into(),
        ));
    }
    for change in changes {
        match change {
            RelatedChange::Ledger {
                source_id,
                source_fingerprint,
                operations,
            } => {
                ledger_count += 1;
                if ledger_count > 1 {
                    return Err(Error::MutationUnsupported(
                        "a related batch supports one ledger plus associated documents".into(),
                    ));
                }
                let source = store.source(project.id, source_id)?;
                if source.fingerprint != source_fingerprint {
                    return Err(Error::SourceConflict(
                        "batch ledger fingerprint changed".into(),
                    ));
                }
                let existing = store.work_items(project.id)?;
                for operation in &operations {
                    if let LedgerBatchOperation::Import { external_key, .. } = operation
                        && existing.iter().any(|w| {
                            w.item.meta.external_key == *external_key
                                && w.item.meta.source_ref.source_id != source.id
                        })
                    {
                        return Err(Error::SourceConflict("import key already belongs to another source in this project; link the existing task instead of creating an ambiguous identity".into()));
                    }
                }
                let prepared = prepare_ledger_batch(
                    &root,
                    &source,
                    &operations,
                    store.projection_ids(&source)?,
                )?;
                validate_archive(store, project.id, source.id, &prepared)?;
                archive_targets.extend(prepared.archive_targets);
                outcomes.extend(prepared.outcomes);
                documents.push(PreparedDocument {
                    path: prepared.path,
                    source,
                    spec: prepared.spec,
                    before: Some(prepared.before),
                    after: prepared.after,
                });
            }
            RelatedChange::Document {
                source_id,
                source_fingerprint,
                edit,
            } => {
                let source = store.source(project.id, source_id)?;
                let prepared = prepare_document_edit(
                    &root,
                    &source,
                    &source_fingerprint,
                    &edit,
                    store.projection_ids(&source)?,
                )?;
                outcomes.push(json!({"source_id":source_id,"operation":"document_edit"}));
                documents.push(prepared);
            }
            RelatedChange::Adopt {
                candidate,
                supersedes,
            } => {
                if supersedes.as_ref().is_some_and(|old| {
                    old.source_id == candidate.source_id
                        || old.external_key == candidate.external_key
                }) {
                    return Err(Error::InvalidInput(
                        "supersession requires distinct old and new documents".into(),
                    ));
                }
                let adoption = DecisionAdoption {
                    version: 1,
                    request_key: request.request_key.clone(),
                    actor: request.actor.clone(),
                    reason: request.reason.clone(),
                    candidate: candidate.clone(),
                    supersedes: supersedes.clone(),
                    content_fingerprint: String::new(),
                };
                // Retire the old accepted document before accepting its replacement. Each step remains recoverable.
                if let Some(old) = &supersedes {
                    let source = store.source(project.id, old.source_id)?;
                    documents.push(prepare_decision_lifecycle(
                        &root,
                        &source,
                        old,
                        &adoption,
                        true,
                        store.projection_ids(&source)?,
                    )?);
                }
                let source = store.source(project.id, candidate.source_id)?;
                documents.push(prepare_decision_lifecycle(
                    &root,
                    &source,
                    &candidate,
                    &adoption,
                    false,
                    store.projection_ids(&source)?,
                )?);
                outcomes.push(
                    json!({"operation":"adopt","candidate":candidate,"supersedes":supersedes}),
                );
            }
        }
    }
    let mut paths = BTreeSet::new();
    let mut source_ids = BTreeSet::new();
    let mut total = 0usize;
    for d in &documents {
        if !paths.insert(d.path.clone()) || !source_ids.insert(d.source.id) {
            return Err(Error::InvalidInput("each source/path may occur only once in a related batch; finish its content edit before reviewing adoption".into()));
        }
        if store
            .source_apply_pending(project.id, d.source.id)?
            .is_some()
        {
            return Err(Error::MutationConflict(
                "finish pending source proposal before a batch".into(),
            ));
        }
        total += d.before.as_ref().unwrap().bytes.len() + d.after.bytes.len();
    }
    if documents.len() > 100 || total > 64 * 1024 * 1024 {
        return Err(Error::BudgetExceeded {
            required: total,
            budget: 64 * 1024 * 1024,
        });
    }
    let observations = store
        .sources(project.id)?
        .into_iter()
        .filter(|s| s.domain == "ledger" && !source_ids.contains(&s.id))
        .collect();
    let files = documents
        .iter()
        .map(|d| FilePlan {
            path: d.path.clone(),
            source: d.source.clone(),
            spec: d.spec.clone(),
            before_fingerprint: d.before.as_ref().unwrap().fingerprint.clone(),
            after_fingerprint: d.after.fingerprint.clone(),
        })
        .collect();
    let plan = Plan {
        version: 1,
        project_id: project.id,
        root: root.clone(),
        project_revision: revision,
        manifest_fingerprint: manifest_hash(&root)?,
        request,
        files,
        observations,
        archive_targets,
        outcomes,
    };
    let file_previews=documents.iter().map(|d|Ok(json!({"path":d.path,"before_text":d.before.as_ref().unwrap().text()?,"after_text":d.after.text()?}))).collect::<Result<Vec<Value>>>()?;
    let preview = fingerprint(&serde_json::to_vec(&plan)?);
    if !accept {
        return Ok(BatchReport {
            value: json!({"ok":true,"status":"preview","requires_accept":true,"project_revision":revision,"preview":{"fingerprint":preview,"plan":plan,"files":file_previews},"source_write_performed":false,"runtime_write_performed":true}),
            failure: None,
        });
    }
    if expected_revision.is_none() || expected_preview != Some(preview.as_str()) {
        return Err(Error::SourceConflict(
            "batch acceptance requires the exact reviewed preview and revision".into(),
        ));
    }
    let _locks = lock_names(&plan)
        .iter()
        .map(|n| named_lock_for_owner(&root, n, Some(&owner)))
        .collect::<Result<Vec<_>>>()?;
    let states = observe(&root, &plan)?;
    if states
        .iter()
        .zip(&plan.files)
        .any(|(s, f)| s != &f.before_fingerprint)
    {
        return Err(Error::SourceConflict(
            "batch source changed after preflight".into(),
        ));
    }
    let mutations = recovery_root(&root)?;
    mutations.create_dir(&owner)?;
    let dir = directory(
        &mutations,
        &owner,
        &root.join(".awr/mutations").join(&owner),
    )?;
    for (i, d) in documents.iter().enumerate() {
        for (suffix, bytes) in [
            ("before", d.before.as_ref().unwrap().bytes.as_slice()),
            ("after", d.after.bytes.as_slice()),
        ] {
            let mut f = new_file(&dir, OsStr::new(&format!("{i}.{suffix}")))?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
    }
    let mut r = Receipt {
        plan,
        fingerprint: preview,
        phase: "prepared".into(),
        project_revision: revision,
        applied_files: vec![],
    };
    save(&dir, &r)?;
    crate::fs_sync::sync_directory(&mutations)?;
    finish(store, &root, &dir, &mut r, revision)
}
pub fn recover_batch(
    store: &mut Store,
    root: &Path,
    key: &str,
    expected: Revision,
) -> Result<BatchReport> {
    let root = root.canonicalize()?;
    let p = store.project_by_root(&root)?;
    let owner = name(p.id, key)?;
    let _request_lock = named_lock(&root, &format!("{owner}.lock"))?;
    let mut r = load(&root, p.id, key)?.ok_or_else(|| Error::NotFound("batch receipt".into()))?;
    let _locks = lock_names(&r.plan)
        .iter()
        .map(|n| named_lock_for_owner(&root, n, Some(&owner)))
        .collect::<Result<Vec<_>>>()?;
    if done(&r) {
        release(&root, &r.plan, &owner)?;
        return Ok(BatchReport {
            value: report(&r, true)?,
            failure: None,
        });
    }
    let dir = open_dir_exact(&root.join(".awr/mutations").join(&owner))?;
    finish(store, &root, &dir, &mut r, expected)
}
fn finish(
    store: &mut Store,
    root: &Path,
    dir: &Dir,
    r: &mut Receipt,
    expected: Revision,
) -> Result<BatchReport> {
    let owner = name(r.plan.project_id, &r.plan.request.request_key)?;
    let base = root.join(".awr/mutations").join(&owner);
    let mut wrote = false;
    let result = (|| -> Result<()> {
        let actual = store.project(r.plan.project_id)?.project_revision;
        if actual != expected {
            return Err(Error::RevisionConflict { expected, actual });
        }
        for id in &r.plan.archive_targets {
            store.ensure_work_unoccupied(r.plan.project_id, *id)?;
        }
        let states = observe(root, &r.plan)?;
        let mut snapshots = vec![];
        for (i, file) in r.plan.files.iter().enumerate() {
            if store
                .source_apply_pending(r.plan.project_id, file.source.id)?
                .is_some()
            {
                return Err(Error::MutationConflict(
                    "source proposal must be recovered first".into(),
                ));
            }
            let before = read(
                &base.join(format!("{i}.before")),
                source_read_cap(&file.source.adapter)?,
            )?;
            let after = read(
                &base.join(format!("{i}.after")),
                source_read_cap(&file.source.adapter)?,
            )?;
            if fingerprint(&before) != file.before_fingerprint
                || fingerprint(&after) != file.after_fingerprint
            {
                return Err(Error::SourceConflict(
                    "batch snapshots differ from the reviewed plan".into(),
                ));
            }
            if states[i] != file.before_fingerprint && states[i] != file.after_fingerprint {
                return Err(Error::SourceConflict(
                    "batch recovery preserves external source changes".into(),
                ));
            }
            let permissions = open_file_exact(&file.path)?.metadata()?.permissions();
            if permissions.readonly() {
                return Err(Error::RuleViolation(
                    "batch destination is read-only".into(),
                ));
            }
            snapshots.push((after, permissions));
        }
        reserve(root, &r.plan, &owner)?;
        for (i, (after, permissions)) in snapshots.into_iter().enumerate() {
            let file = &r.plan.files[i];
            let current = observe(root, &r.plan)?;
            for (s, f) in current.iter().zip(&r.plan.files) {
                if s != &f.before_fingerprint && s != &f.after_fingerprint {
                    return Err(Error::SourceConflict(
                        "source changed while batch was in progress".into(),
                    ));
                }
            }
            if current[i] == file.before_fingerprint
                && file.before_fingerprint != file.after_fingerprint
            {
                let mut replacement = SourceReplacement::prepare(&file.path, &after, permissions)?;
                if observe(root, &r.plan)? != current {
                    return Err(Error::SourceConflict(
                        "batch source changed immediately before writing".into(),
                    ));
                }
                let actual = store.project(r.plan.project_id)?.project_revision;
                if actual != expected {
                    return Err(Error::RevisionConflict { expected, actual });
                }
                replacement.install()?;
                replacement.sync_parent()?;
                wrote = true;
            }
            if file.before_fingerprint != file.after_fingerprint && !r.applied_files.contains(&i) {
                r.applied_files.push(i);
            }
            save(dir, r)?;
        }
        if r.plan
            .files
            .iter()
            .all(|f| f.before_fingerprint == f.after_fingerprint)
        {
            r.phase = "no_change".into();
        } else {
            r.phase = "applied".into();
            save(dir, r)?;
            let indexed = index_project(store, root, &Manifest::load(root)?, false)?;
            r.project_revision = indexed.project_revision;
            if !indexed.ok {
                return Err(Error::SourceStale(
                    "batch files were written but indexing requires recovery".into(),
                ));
            }
            if observe(root, &r.plan)?
                .iter()
                .zip(&r.plan.files)
                .any(|(s, f)| s != &f.after_fingerprint)
            {
                return Err(Error::SourceConflict("indexed batch source changed".into()));
            }
            for f in &r.plan.files {
                if store.source(r.plan.project_id, f.source.id)?.fingerprint != f.after_fingerprint
                {
                    return Err(Error::SourceConflict(
                        "batch projection differs from reviewed content".into(),
                    ));
                }
            }
            r.phase = "completed".into();
        }
        save(dir, r)?;
        release(root, &r.plan, &owner)
    })();
    let mut value = report(r, false)?;
    value["source_write_performed"] = json!(wrote);
    value["runtime_write_performed"] = json!(true);
    let failure = result.err();
    if let Some(e) = &failure {
        value["ok"] = json!(false);
        value["status"] = json!("pending_recovery");
        value["write_outcome"] = json!("pending_recovery");
        value["error"] = serde_json::to_value(e.report())?;
    }
    Ok(BatchReport { value, failure })
}
