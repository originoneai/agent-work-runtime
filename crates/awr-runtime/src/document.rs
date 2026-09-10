//! Durable single-file document operations for local hosts.
use crate::mutation_apply::{
    SourceReplacement, directory, named_lock, new_file, recovery_root, source_lock,
};
use awr_core::*;
use awr_source::*;
use awr_store::Store;
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentRequest {
    pub version: u32,
    pub request_key: String,
    pub change: DocumentAction,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    version: u32,
    project_id: Id,
    root: PathBuf,
    project_revision: Revision,
    request: DocumentRequest,
    manifest_fingerprint: String,
    path: PathBuf,
    source: Option<Source>,
    spec: SourceSpec,
    before_fingerprint: Option<String>,
    after_fingerprint: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    plan: Plan,
    fingerprint: String,
    phase: String,
    project_revision: Revision,
    source_id: Option<Id>,
}
pub struct DocumentReport {
    pub value: Value,
    pub failure: Option<Error>,
}
fn identity(project: Id, key: &str) -> Result<String> {
    if key.trim().is_empty() || key.len() > 512 || key.chars().any(char::is_control) {
        return Err(Error::InvalidInput(
            "request key requires 1..512 bytes without controls".into(),
        ));
    }
    ensure_public_text(key)?;
    Ok(fingerprint(&serde_json::to_vec(&(project, key))?)
        .trim_start_matches("sha256:")
        .into())
}
fn name(project: Id, key: &str) -> Result<String> {
    Ok(format!("document-{}", identity(project, key)?))
}
fn read(path: &Path, cap: u64) -> Result<Vec<u8>> {
    let file = open_file_exact(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > cap {
        return Err(Error::InvalidInput(
            "invalid or oversized document recovery file".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(Error::InvalidInput(
            "document recovery file exceeded its bound".into(),
        ));
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
                "invalid document receipt directory".into(),
            ));
        }
        _ => (),
    }
    let r: Receipt =
        serde_json::from_slice(&read(&base.join("receipt.json"), MARKDOWN_READ_CAP * 3)?)?;
    if r.plan.version != 1
        || r.plan.request.version != 1
        || r.plan.project_id != project
        || r.plan.root != root
        || r.plan.request.request_key != key
        || fingerprint(&serde_json::to_vec(&r.plan)?) != r.fingerprint
        || !["prepared", "applied", "completed", "no_change"].contains(&r.phase.as_str())
    {
        return Err(Error::SourceConflict(
            "document receipt differs from its bound project, version or plan".into(),
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
        json!({"ok":done(r),"status":if r.phase=="no_change"{"no_change"}else if done(r){"completed"}else{"pending_recovery"},
        "request_key":r.plan.request.request_key,"project_id":r.plan.project_id,"project_revision":r.project_revision,"source_id":r.source_id,
        "preview_fingerprint":r.fingerprint,"phase":r.phase,"already_recorded":replay,"historical_outcome":replay,
        "source_write_performed":false,"runtime_write_performed":false,"write_outcome":if r.phase=="no_change"{"no_change"}else if done(r){"applied"}else{"pending_recovery"},
        "recovery_directory":format!(".awr/mutations/{}",name(r.plan.project_id,&r.plan.request.request_key)?)}),
    )
}
fn current(root: &Path, plan: &Plan) -> Result<Option<String>> {
    if manifest_hash(root)? != plan.manifest_fingerprint {
        return Err(Error::SourceConflict(
            "document source registration changed".into(),
        ));
    }
    let spec = document_path_registration(root, &plan.path)?;
    if serde_json::to_value(spec)? != serde_json::to_value(&plan.spec)? {
        return Err(Error::SourceConflict("document mapping changed".into()));
    }
    if let Some(source) = &plan.source {
        let (_, _, snapshot) = inspect_registered_source(root, source)?;
        return Ok(Some(snapshot.fingerprint));
    }
    match std::fs::symlink_metadata(&plan.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(_) => Ok(Some(fingerprint(&read(&plan.path, MARKDOWN_READ_CAP)?))),
    }
}
pub fn document_status(store: &Store, root: &Path, key: &str) -> Result<DocumentReport> {
    let root = root.canonicalize()?;
    let p = store.project_by_root(&root)?;
    let value = if let Some(r) = load(&root, p.id, key)? {
        let mut v = report(&r, true)?;
        v["found"] = json!(true);
        v["read_only"] = json!(true);
        v["current_source"] = json!(match current(&root, &r.plan) {
            Ok(Some(s)) if s == r.plan.after_fingerprint => "after",
            Ok(s) if s == r.plan.before_fingerprint => "before",
            Ok(_) => "externally_changed",
            Err(_) => "unavailable_or_registration_changed",
        });
        v
    } else {
        json!({"ok":true,"found":false,"read_only":true,"source_write_performed":false,"runtime_write_performed":false})
    };
    Ok(DocumentReport {
        value,
        failure: None,
    })
}
pub fn change_document(
    store: &mut Store,
    root: &Path,
    request: DocumentRequest,
    accept: bool,
    expected_preview: Option<&str>,
    expected_revision: Option<Revision>,
) -> Result<DocumentReport> {
    let root = root.canonicalize()?;
    let project = store.project_by_root(&root)?;
    let key = identity(project.id, &request.request_key)?;
    ensure_public_data(&request)?;
    if request.version != 1 {
        return Err(Error::InvalidInput(
            "document request requires version 1".into(),
        ));
    }
    let _lock = if accept {
        Some(named_lock(&root, &format!("document-{key}.lock"))?)
    } else {
        None
    };
    if let Some(r) = load(&root, project.id, &request.request_key)? {
        if r.plan.request != request {
            return Err(Error::SourceConflict(
                "document request key already belongs to different content".into(),
            ));
        }
        return Ok(DocumentReport {
            value: report(&r, true)?,
            failure: (!done(&r)).then(|| {
                Error::MutationConflict(
                    "inspect and explicitly recover the pending document operation".into(),
                )
            }),
        });
    }
    let indexed = index_project(store, &root, &Manifest::load(&root)?, false)?;
    if !indexed.ok {
        return Err(Error::SourceStale(
            "refresh registered sources before editing a document".into(),
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
    let prepared = match &request.change {
        DocumentAction::Edit {
            source_id,
            source_fingerprint,
            edit,
        } => {
            let source = store.source(project.id, *source_id)?;
            prepare_document_edit(
                &root,
                &source,
                source_fingerprint,
                edit,
                store.projection_ids(&source)?,
            )?
        }
        DocumentAction::CreateDraft { path, title, body } => prepare_document_draft(
            &root,
            project.id,
            path,
            title,
            body,
            &format!("DOC-{}", &key[..32]),
        )?,
    };
    let _path_lock = if accept {
        Some(named_lock(
            &root,
            &format!(
                "document-path-{}.lock",
                fingerprint(prepared.path.to_string_lossy().as_bytes())
                    .trim_start_matches("sha256:")
            ),
        )?)
    } else {
        None
    };
    let _source_lock = if accept && prepared.before.is_some() {
        Some(source_lock(&root, prepared.source.id)?)
    } else {
        None
    };
    let plan = Plan {
        version: 1,
        project_id: project.id,
        root: root.clone(),
        project_revision: revision,
        request,
        manifest_fingerprint: manifest_hash(&root)?,
        path: prepared.path,
        source: prepared.before.as_ref().map(|_| prepared.source),
        spec: prepared.spec,
        before_fingerprint: prepared.before.as_ref().map(|s| s.fingerprint.clone()),
        after_fingerprint: prepared.after.fingerprint.clone(),
    };
    let preview = fingerprint(&serde_json::to_vec(&plan)?);
    if !accept {
        return Ok(DocumentReport {
            value: json!({"ok":true,"status":"preview","requires_accept":true,"project_revision":revision,
        "preview":{"fingerprint":preview,"plan":plan,"before_text":prepared.before.as_ref().map(|s|s.text()).transpose()?,"after_text":prepared.after.text()?},
        "source_write_performed":false,"runtime_write_performed":true}),
            failure: None,
        });
    }
    if expected_preview != Some(preview.as_str()) || expected_revision.is_none() {
        return Err(Error::SourceConflict(
            "document acceptance requires exact reviewed preview and project revision".into(),
        ));
    }
    let mutations = recovery_root(&root)?;
    let name = name(project.id, &plan.request.request_key)?;
    mutations.create_dir(&name)?;
    let dir = directory(&mutations, &name, &root.join(".awr/mutations").join(&name))?;
    for (name, bytes) in [
        (
            "before.md",
            prepared
                .before
                .as_ref()
                .map(|s| s.bytes.as_slice())
                .unwrap_or(&[]),
        ),
        ("after.md", prepared.after.bytes.as_slice()),
    ] {
        let mut f = new_file(&dir, OsStr::new(name))?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    let mut receipt = Receipt {
        source_id: plan.source.as_ref().map(|s| s.id),
        plan,
        fingerprint: preview,
        phase: "prepared".into(),
        project_revision: revision,
    };
    save(&dir, &receipt)?;
    crate::fs_sync::sync_directory(&mutations)?;
    finish(store, &root, &dir, &mut receipt, revision)
}
pub fn recover_document(
    store: &mut Store,
    root: &Path,
    key: &str,
    expected: Revision,
) -> Result<DocumentReport> {
    let root = root.canonicalize()?;
    let project = store.project_by_root(&root)?;
    let _lock = named_lock(
        &root,
        &format!("document-{}.lock", identity(project.id, key)?),
    )?;
    let mut r =
        load(&root, project.id, key)?.ok_or_else(|| Error::NotFound("document receipt".into()))?;
    if done(&r) {
        return Ok(DocumentReport {
            value: report(&r, true)?,
            failure: None,
        });
    }
    let _path_lock = named_lock(
        &root,
        &format!(
            "document-path-{}.lock",
            fingerprint(r.plan.path.to_string_lossy().as_bytes()).trim_start_matches("sha256:")
        ),
    )?;
    let _source_lock = r
        .plan
        .source
        .as_ref()
        .map(|s| source_lock(&root, s.id))
        .transpose()?;
    let dir = open_dir_exact(&root.join(".awr/mutations").join(name(project.id, key)?))?;
    finish(store, &root, &dir, &mut r, expected)
}
fn finish(
    store: &mut Store,
    root: &Path,
    dir: &Dir,
    r: &mut Receipt,
    expected: Revision,
) -> Result<DocumentReport> {
    let mut wrote = false;
    let mut indexed = None;
    let result = (|| -> Result<()> {
        let actual = store.project(r.plan.project_id)?.project_revision;
        if actual != expected {
            return Err(Error::RevisionConflict { expected, actual });
        }
        let plan = &r.plan;
        let base = root
            .join(".awr/mutations")
            .join(name(plan.project_id, &plan.request.request_key)?);
        let before = read(&base.join("before.md"), MARKDOWN_READ_CAP)?;
        let after = read(&base.join("after.md"), MARKDOWN_READ_CAP)?;
        if plan
            .before_fingerprint
            .as_ref()
            .is_some_and(|s| *s != fingerprint(&before))
            || fingerprint(&after) != plan.after_fingerprint
        {
            return Err(Error::SourceConflict(
                "document recovery snapshots differ from the reviewed plan".into(),
            ));
        }
        let observed = current(root, plan)?;
        if observed != plan.before_fingerprint && observed != Some(plan.after_fingerprint.clone()) {
            return Err(Error::SourceConflict(
                "document recovery preserves externally changed files".into(),
            ));
        }
        if observed == plan.before_fingerprint && observed != Some(plan.after_fingerprint.clone()) {
            let permissions = if plan.before_fingerprint.is_some() {
                open_file_exact(&plan.path)?.metadata()?.permissions()
            } else {
                if open_dir_exact(plan.path.parent().unwrap())?
                    .dir_metadata()?
                    .permissions()
                    .readonly()
                {
                    return Err(Error::RuleViolation(
                        "document directory is read-only".into(),
                    ));
                }
                new_permissions()?
            };
            if permissions.readonly() {
                return Err(Error::RuleViolation(
                    "document destination is read-only".into(),
                ));
            }
            let mut replacement = SourceReplacement::prepare(&plan.path, &after, permissions)?;
            if current(root, plan)? != observed {
                return Err(Error::SourceConflict(
                    "document changed immediately before writing".into(),
                ));
            }
            let actual = store.project(plan.project_id)?.project_revision;
            if actual != expected {
                return Err(Error::RevisionConflict { expected, actual });
            }
            if plan.before_fingerprint.is_some() {
                replacement.install()?;
                replacement.sync_parent()?
            } else {
                replacement.install_new()?
            }
            wrote = true;
        }
        if plan.before_fingerprint.as_ref() == Some(&plan.after_fingerprint) {
            r.phase = "no_change".into();
            return save(dir, r);
        }
        r.phase = "applied".into();
        save(dir, r)?;
        let result = index_project(store, root, &Manifest::load(root)?, false)?;
        r.project_revision = result.project_revision;
        indexed = Some(serde_json::to_value(&result)?);
        if !result.ok {
            return Err(Error::SourceStale(
                "document was applied but projections require recovery".into(),
            ));
        }
        let locator = Locator::File(plan.path.clone()).identity()?;
        let sources = store.sources(plan.project_id)?;
        let matching: Vec<_> = sources
            .iter()
            .filter(|s| {
                s.locator == locator
                    && s.adapter == plan.spec.adapter
                    && s.domain == plan.spec.domain
            })
            .collect();
        if matching.len() != 1
            || matching[0].fingerprint != plan.after_fingerprint
            || current(root, plan)? != Some(plan.after_fingerprint.clone())
        {
            return Err(Error::SourceConflict(
                "document projection differs from the applied source".into(),
            ));
        }
        r.source_id = Some(matching[0].id);
        r.phase = "completed".into();
        save(dir, r)
    })();
    let mut value = report(r, false)?;
    value["source_write_performed"] = json!(wrote);
    value["runtime_write_performed"] = json!(true);
    if let Some(index) = indexed {
        value["index"] = index
    }
    let failure = result.err();
    if let Some(e) = &failure {
        value["ok"] = json!(false);
        value["status"] = json!("pending_recovery");
        value["write_outcome"] = json!("pending_recovery");
        value["error"] = serde_json::to_value(e.report())?
    }
    Ok(DocumentReport { value, failure })
}
fn new_permissions() -> Result<std::fs::Permissions> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let file = std::env::current_exe()?;
        let mut p = std::fs::metadata(file)?.permissions();
        p.set_readonly(false);
        Ok(p)
    }
}
