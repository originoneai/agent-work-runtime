//! Matched, same-root runtime snapshots. Source files are never restore targets.
use awr_core::{Error, Freshness, Id, Result, Revision, Source};
use awr_source::{Manifest, fingerprint};
use awr_store::{RuntimeLease, Store};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const FILE_CAP: u64 = 256 * 1024 * 1024;
const TOTAL_CAP: u64 = 1024 * 1024 * 1024;
const FILE_COUNT: usize = 10000;
const CLI_PAYLOAD: &str = if cfg!(windows) {
    "program/awr.exe"
} else {
    "program/awr"
};
const MCP_PAYLOAD: &str = if cfg!(windows) {
    "program/awr-mcp.exe"
} else {
    "program/awr-mcp"
};
const DATABASE_FILES: [&str; 4] = [
    "state.db",
    "state.db-wal",
    "state.db-shm",
    "state.db-journal",
];

#[derive(Debug, Subcommand)]
pub enum RuntimeCommand {
    /// Inspect the executing program, schema, configuration and current source/history binding.
    Binding,
    /// Capture a coherent database, registered sources, runtime files and executable copies.
    Backup {
        #[arg(long)]
        output: PathBuf,
        /// Optional matching MCP executable to retain alongside the CLI.
        #[arg(long)]
        companion: Option<PathBuf>,
        /// Optional caller assertion; executable bytes remain the verified program identity.
        #[arg(long)]
        source_sha: Option<String>,
    },
    /// Verify a sealed backup without opening or changing the target project database.
    Check {
        #[arg(long)]
        backup: PathBuf,
    },
    /// Preview a same-root history restore; sources and known runtime files must still match.
    RestorePreview {
        #[arg(long)]
        backup: PathBuf,
    },
    /// Restore only the database after an exact preview and explicit offline acknowledgement.
    Restore {
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        expected_preview: String,
        /// Assert all clients, including older versions without runtime leases, are stopped.
        #[arg(long, required = true)]
        offline: bool,
    },
    /// Inspect an interrupted restore without opening SQLite. Omit ID for the pending restore.
    RestoreStatus {
        #[arg(long)]
        id: Option<Id>,
    },
    /// Resume the pending restore only while its saved inputs and database bytes still match.
    RestoreRecover {
        #[arg(long, required = true)]
        offline: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Blob {
    path: String,
    sha256: String,
    bytes: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Program {
    original_path: PathBuf,
    version: String,
    binary: Blob,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceCopy {
    source: Source,
    observed_locator: String,
    content: Blob,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    id: Id,
    created_at: i64,
    project_root: PathBuf,
    project_id: Id,
    project_revision: Revision,
    schema_version: i64,
    program: Program,
    companion: Option<Program>,
    caller_source_sha: Option<String>,
    source_state_fingerprint: String,
    sources: Vec<SourceCopy>,
    runtime_files: BTreeMap<String, Blob>,
    database: Blob,
    fingerprint: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreJournal {
    version: u32,
    id: Id,
    phase: String,
    created_at: i64,
    project_root: PathBuf,
    project_id: Id,
    backup: PathBuf,
    backup_fingerprint: String,
    preview: Value,
    before_database: BTreeMap<String, Option<Blob>>,
    rollback: BTreeMap<String, Blob>,
    restored_revision: Revision,
}

fn conflict(message: impl Into<String>) -> Error {
    Error::SourceConflict(message.into())
}
fn relative(path: &str) -> Result<&Path> {
    let p = Path::new(path);
    if path.is_empty() || p.components().any(|p| !matches!(p, Component::Normal(_))) {
        return Err(Error::RuleViolation(
            "snapshot paths must be relative without traversal".into(),
        ));
    }
    Ok(p)
}
fn exact_read(path: &Path, cap: u64) -> Result<Vec<u8>> {
    let file = awr_source::open_file_exact(path)?;
    let m = file.metadata()?;
    if !m.is_file() || m.len() > cap {
        return Err(Error::InvalidInput(
            "snapshot needs a bounded regular file".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(Error::InvalidInput(
            "snapshot file grew past its read cap".into(),
        ));
    }
    Ok(bytes)
}
fn describe(path: String, bytes: &[u8]) -> Blob {
    Blob {
        path,
        sha256: fingerprint(bytes),
        bytes: bytes.len() as u64,
    }
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn private_dir(path: &Path) -> Result<()> {
    if exists(path)? {
        awr_source::open_dir_exact(path)?;
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidInput("directory needs parent".into()))?;
    awr_source::open_dir_exact(parent)?;
    let mut b = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(path)?;
    sync_dir(parent)
}
fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8], executable: bool) -> Result<()> {
    awr_source::open_dir_exact(
        path.parent()
            .ok_or_else(|| Error::InvalidInput("file needs parent".into()))?,
    )?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if executable { 0o700 } else { 0o600 });
    }
    #[cfg(not(unix))]
    {
        let _ = executable;
    }
    let mut f = options.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    sync_dir(path.parent().unwrap())
}
fn write_payload(root: &Path, path: &str, bytes: &[u8], executable: bool) -> Result<Blob> {
    let target = root.join(relative(path)?);
    let mut parent = root.to_path_buf();
    for p in relative(path)?.parent().unwrap().components() {
        parent.push(p.as_os_str());
        private_dir(&parent)?;
    }
    write_new(&target, bytes, executable)?;
    Ok(describe(path.into(), bytes))
}
fn blob_bytes(root: &Path, blob: &Blob) -> Result<Vec<u8>> {
    let bytes = exact_read(&root.join(relative(&blob.path)?), FILE_CAP)?;
    if describe(blob.path.clone(), &bytes) != *blob {
        return Err(conflict(format!("backup payload differs: {}", blob.path)));
    }
    Ok(bytes)
}
fn seal<T: Serialize>(value: &T) -> Result<String> {
    // Canonical object ordering permits independent manifest verification by hosts.
    Ok(fingerprint(&serde_json::to_vec(&serde_json::to_value(
        value,
    )?)?))
}
fn snapshot_fingerprint(s: &Snapshot) -> Result<String> {
    let mut copy = s.clone();
    copy.fingerprint.clear();
    seal(&copy)
}
fn program() -> Result<(Program, Vec<u8>)> {
    let path = std::env::current_exe()?.canonicalize()?;
    let bytes = exact_read(&path, FILE_CAP)?;
    Ok((
        Program {
            original_path: path,
            version: env!("CARGO_PKG_VERSION").into(),
            binary: describe(CLI_PAYLOAD.into(), &bytes),
        },
        bytes,
    ))
}
fn excluded_runtime(path: &str, restore_id: Option<Id>) -> bool {
    DATABASE_FILES.contains(&path)
        || path == "state.db-restore.pending"
        || path.ends_with(".lock")
        || restore_id.is_some_and(|id| path.starts_with(&format!("restores/{id}/")))
}
fn inventory(runtime: &Path, restore_id: Option<Id>) -> Result<BTreeMap<String, Blob>> {
    let mut files = BTreeMap::new();
    let mut total = 0;
    fn walk(
        root: &Path,
        dir: &Path,
        id: Option<Id>,
        files: &mut BTreeMap<String, Blob>,
        total: &mut u64,
    ) -> Result<()> {
        awr_source::open_dir_exact(dir)?;
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| conflict("runtime inventory escaped root"))?
                .to_str()
                .ok_or_else(|| Error::InvalidInput("runtime names must be UTF-8".into()))?
                .replace('\\', "/");
            if excluded_runtime(&relative, id) {
                continue;
            }
            let meta = fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() {
                return Err(conflict("runtime snapshots refuse symlinked entries"));
            }
            if meta.is_dir() {
                walk(root, &path, id, files, total)?;
            } else {
                let bytes = exact_read(&path, FILE_CAP)?;
                *total += bytes.len() as u64;
                if *total > TOTAL_CAP || files.len() >= FILE_COUNT {
                    return Err(Error::InvalidInput(
                        "runtime snapshot exceeds total size/file cap".into(),
                    ));
                }
                files.insert(
                    relative.clone(),
                    describe(format!("runtime/{relative}"), &bytes),
                );
            }
        }
        Ok(())
    }
    walk(runtime, runtime, restore_id, &mut files, &mut total)?;
    Ok(files)
}
fn require_clear(runtime: &Path) -> Result<()> {
    if exists(&awr_store::restore_pending_path(&runtime.join("state.db")))? {
        return Err(conflict(
            "runtime restore is pending; inspect restore-status and recover",
        ));
    }
    Ok(())
}
fn sources(
    store: &Store,
    root: &Path,
) -> Result<(String, Vec<(Source, awr_source::SourceSnapshot)>)> {
    let project = store.project_by_root(root)?;
    let state = awr_source::source_state_fingerprint(store, project.id)?;
    let mut check = store.memory_snapshot(FILE_CAP)?;
    let report = awr_source::index_project(&mut check, root, &Manifest::load(root)?, false)?;
    if !report.ok || awr_source::source_state_fingerprint(&check, project.id)? != state {
        return Err(Error::SourceStale("configuration or source inventory differs from recorded state; refresh explicitly before taking a matching snapshot".into()));
    }
    let mut content = Vec::new();
    let mut total = 0;
    for source in store.sources(project.id)? {
        if source.freshness != Freshness::Fresh {
            return Err(Error::SourceStale(
                "snapshot requires fresh registered sources".into(),
            ));
        }
        let (_, _, snapshot) = awr_source::inspect_registered_source(root, &source)?;
        if snapshot.fingerprint != source.fingerprint {
            return Err(conflict("source changed while capturing runtime snapshot"));
        }
        total += snapshot.bytes.len() as u64;
        if total > TOTAL_CAP || content.len() >= FILE_COUNT {
            return Err(Error::InvalidInput(
                "source snapshot exceeds total size/file cap".into(),
            ));
        }
        content.push((source, snapshot));
    }
    Ok((state, content))
}
fn binding(root: &Path, runtime: &Path) -> Result<Value> {
    require_clear(runtime)?;
    let store = Store::read_snapshot(&runtime.join("state.db"), FILE_CAP)?;
    let project = store.project_by_root(root)?;
    let doctor = store.doctor()?;
    if !doctor.ok {
        return Err(Error::Storage(
            "snapshot database integrity check failed".into(),
        ));
    }
    let (state, source_bytes) = sources(&store, root)?;
    let (program, _) = program()?;
    Ok(
        json!({"version":1,"read_only":true,"program":program,"schema_version":doctor.schema_version,"project":project,"configuration_fingerprint":fingerprint(&exact_read(&runtime.join("project.toml"),65536)?),"source_state_fingerprint":state,"sources":source_bytes.into_iter().map(|(s,_)|s).collect::<Vec<_>>(),"source_currentness_verified":true,"management_binding_rewritten":false}),
    )
}
fn backup(
    root: &Path,
    runtime: &Path,
    output: &Path,
    companion: Option<&Path>,
    source_sha: Option<&str>,
) -> Result<Value> {
    require_clear(runtime)?;
    if source_sha.is_some_and(|s| s.len() != 40 || !s.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err(Error::InvalidInput(
            "source-sha must be a full Git SHA; this remains a caller assertion".into(),
        ));
    }
    let output = if output.is_absolute() {
        output.to_owned()
    } else {
        root.join(output)
    };
    let parent = output
        .parent()
        .ok_or_else(|| Error::InvalidInput("backup output needs a parent".into()))?
        .canonicalize()?;
    let output = parent.join(
        output
            .file_name()
            .ok_or_else(|| Error::InvalidInput("backup output needs a name".into()))?,
    );
    if output.starts_with(runtime) || exists(&output)? {
        return Err(conflict("backup needs a new directory outside .awr"));
    }
    let before = inventory(runtime, None)?;
    let store = Store::read_snapshot(&runtime.join("state.db"), FILE_CAP)?;
    let project = store.project_by_root(root)?;
    let doctor = store.doctor()?;
    if !doctor.ok {
        return Err(Error::Storage(
            "backup requires an intact current-schema database".into(),
        ));
    }
    let (state, content) = sources(&store, root)?;
    let (program, program_bytes) = program()?;
    let companion = companion
        .map(|p| -> Result<(Program, Vec<u8>)> {
            let path = if p.is_absolute() {
                p.to_owned()
            } else {
                root.join(p)
            }
            .canonicalize()?;
            let bytes = exact_read(&path, FILE_CAP)?;
            Ok((
                Program {
                    original_path: path,
                    version: "caller-selected; bytes verified".into(),
                    binary: describe(MCP_PAYLOAD.into(), &bytes),
                },
                bytes,
            ))
        })
        .transpose()?;
    private_dir(&output)?; // New, private staging directory. Only a final manifest seals it.
    write_payload(&output, CLI_PAYLOAD, &program_bytes, true)?;
    if let Some((_, bytes)) = &companion {
        write_payload(&output, MCP_PAYLOAD, bytes, true)?;
    }
    private_dir(&output.join("runtime"))?;
    store.export_snapshot(&output.join("runtime/state.db"))?;
    let database = describe(
        "runtime/state.db".into(),
        &exact_read(&output.join("runtime/state.db"), FILE_CAP)?,
    );
    for (path, expected) in &before {
        let bytes = exact_read(&runtime.join(relative(path)?), FILE_CAP)?;
        if describe(expected.path.clone(), &bytes) != *expected {
            return Err(conflict(
                "runtime files changed during backup; discard the unsealed directory and retry",
            ));
        }
        write_payload(&output, &expected.path, &bytes, false)?;
    }
    let mut copies = Vec::new();
    for (source, observed) in content {
        let payload = write_payload(
            &output,
            &format!("sources/{}.bin", source.id),
            &observed.bytes,
            false,
        )?;
        copies.push(SourceCopy {
            source,
            observed_locator: observed.locator,
            content: payload,
        });
    }
    if inventory(runtime, None)? != before || sources(&store, root)?.0 != state {
        return Err(conflict(
            "snapshot inputs changed; output remains unsealed, retry with a new directory",
        ));
    }
    let mut snapshot = Snapshot {
        version: 1,
        id: Id::new(),
        created_at: awr_core::now_millis()?,
        project_root: root.to_owned(),
        project_id: project.id,
        project_revision: project.project_revision,
        schema_version: doctor.schema_version,
        program,
        companion: companion.map(|(p, _)| p),
        caller_source_sha: source_sha.map(str::to_owned),
        source_state_fingerprint: state,
        sources: copies,
        runtime_files: before,
        database,
        fingerprint: String::new(),
    };
    let total = snapshot.database.bytes
        + snapshot.program.binary.bytes
        + snapshot.companion.as_ref().map_or(0, |p| p.binary.bytes)
        + snapshot
            .runtime_files
            .values()
            .map(|b| b.bytes)
            .sum::<u64>()
        + snapshot
            .sources
            .iter()
            .map(|s| s.content.bytes)
            .sum::<u64>();
    if total > TOTAL_CAP || snapshot.runtime_files.len() + snapshot.sources.len() + 3 > FILE_COUNT {
        return Err(Error::InvalidInput(
            "combined backup exceeds total size/file cap; output remains unsealed".into(),
        ));
    }
    snapshot.fingerprint = snapshot_fingerprint(&snapshot)?;
    write_new(
        &output.join("snapshot.json"),
        &serde_json::to_vec_pretty(&snapshot)?,
        false,
    )?;
    Ok(
        json!({"phase":"completed","backup":output,"snapshot":snapshot,"scope":"registered_sources_and_persistent_awr_runtime","excluded":"external artifacts, host-owned state outside .awr, unregistered business files, transient locks","management_binding_rewritten":false,"source_sha_provenance":"caller_assertion_only"}),
    )
}
fn checked(backup: &Path) -> Result<(PathBuf, Snapshot, Store)> {
    let root = backup.canonicalize()?;
    awr_source::open_dir_exact(&root)?;
    let snapshot: Snapshot =
        serde_json::from_slice(&exact_read(&root.join("snapshot.json"), 16 * 1024 * 1024)?)?;
    if snapshot.version != 1
        || snapshot.schema_version != awr_store::SCHEMA_VERSION
        || snapshot.fingerprint != snapshot_fingerprint(&snapshot)?
    {
        return Err(conflict("unsupported or altered snapshot manifest"));
    }
    if snapshot.database.path != "runtime/state.db"
        || snapshot.program.binary.path != CLI_PAYLOAD
        || snapshot
            .companion
            .as_ref()
            .is_some_and(|p| p.binary.path != MCP_PAYLOAD)
    {
        return Err(conflict("invalid snapshot payload layout"));
    }
    let mut payloads = vec![&snapshot.database, &snapshot.program.binary];
    if let Some(c) = &snapshot.companion {
        payloads.push(&c.binary);
    }
    let mut names = BTreeSet::new();
    let mut total = 0;
    for (name, blob) in &snapshot.runtime_files {
        relative(name)?;
        if excluded_runtime(name, None) || blob.path != format!("runtime/{name}") {
            return Err(conflict("invalid runtime file binding"));
        }
        payloads.push(blob);
    }
    if !snapshot.runtime_files.contains_key("project.toml") {
        return Err(conflict("snapshot has no project configuration"));
    }
    for copy in &snapshot.sources {
        if copy.content.path != format!("sources/{}.bin", copy.source.id) {
            return Err(conflict("source payload binding differs"));
        }
        payloads.push(&copy.content);
    }
    for blob in payloads {
        if blob.bytes > FILE_CAP {
            return Err(Error::InvalidInput(
                "snapshot payload exceeds file cap".into(),
            ));
        }
        if !names.insert(&blob.path) {
            return Err(conflict("duplicate snapshot payload"));
        }
        total += blob.bytes;
        if total > TOTAL_CAP || names.len() > FILE_COUNT {
            return Err(Error::InvalidInput(
                "snapshot exceeds total size/file cap".into(),
            ));
        }
        blob_bytes(&root, blob)?;
    }
    for copy in &snapshot.sources {
        let observed = awr_source::SourceSnapshot {
            locator: copy.observed_locator.clone(),
            fingerprint: copy.source.fingerprint.clone(),
            bytes: blob_bytes(&root, &copy.content)?,
        };
        let location_matches = if let Some(original) = copy.source.locator.strip_prefix("git://") {
            let original_path = original.split_once(':').map(|(_, p)| p);
            let observed_path = observed
                .locator
                .strip_prefix("git://")
                .and_then(|s| s.split_once(':'))
                .map(|(_, p)| p);
            original_path.is_some() && original_path == observed_path
        } else {
            copy.source.locator == observed.locator
        };
        if !location_matches || !observed.verify_fingerprint() {
            return Err(conflict(
                "source bytes or resolved locator differ from recorded fingerprint",
            ));
        }
    }
    for suffix in ["-wal", "-shm", "-journal", "-restore.pending"] {
        if exists(&root.join(format!("runtime/state.db{suffix}")))? {
            return Err(conflict(
                "sealed database must not have sidecars or a pending restore",
            ));
        }
    }
    let store = Store::read_snapshot(&root.join("runtime/state.db"), FILE_CAP)?;
    let project = store.project_by_root(&snapshot.project_root)?;
    if !store.doctor()?.ok
        || project.id != snapshot.project_id
        || project.project_revision != snapshot.project_revision
        || awr_source::source_state_fingerprint(&store, project.id)?
            != snapshot.source_state_fingerprint
        || serde_json::to_value(store.sources(project.id)?)?
            != serde_json::to_value(
                snapshot
                    .sources
                    .iter()
                    .map(|s| &s.source)
                    .collect::<Vec<_>>(),
            )?
    {
        return Err(conflict(
            "database identity, history or source bindings differ from snapshot",
        ));
    }
    Ok((root, snapshot, store))
}
fn database_files(runtime: &Path) -> Result<BTreeMap<String, Option<Blob>>> {
    DATABASE_FILES
        .into_iter()
        .map(|name| {
            let path = runtime.join(name);
            Ok((
                name.into(),
                if exists(&path)? {
                    Some(describe(name.into(), &exact_read(&path, FILE_CAP)?))
                } else {
                    None
                },
            ))
        })
        .collect()
}
fn matches_target(
    root: &Path,
    runtime: &Path,
    snapshot: &Snapshot,
    store: &Store,
    id: Option<Id>,
) -> Result<BTreeMap<String, Blob>> {
    if snapshot.project_root != root {
        return Err(conflict(
            "restore is bound to the original canonical project root",
        ));
    }
    if program()?.0.binary.sha256 != snapshot.program.binary.sha256
        || snapshot.program.version != env!("CARGO_PKG_VERSION")
    {
        return Err(conflict(
            "executing CLI differs from backup; explicitly run the retained matching executable",
        ));
    }
    if let Some(p) = &snapshot.companion {
        if fingerprint(&exact_read(&p.original_path, FILE_CAP)?) != p.binary.sha256 {
            return Err(conflict(
                "companion executable no longer matches its backup binding",
            ));
        }
    }
    let inventory = inventory(runtime, id)?;
    for (name, expected) in &snapshot.runtime_files {
        if inventory.get(name) != Some(expected) {
            return Err(conflict(format!(
                "known runtime/configuration file changed or is missing: {name}; restore never overwrites it"
            )));
        }
    }
    if sources(store, root)?.0 != snapshot.source_state_fingerprint {
        return Err(conflict("authoritative sources no longer match backup"));
    }
    Ok(inventory
        .into_iter()
        .filter(|(name, _)| !snapshot.runtime_files.contains_key(name))
        .collect())
}
fn preview(
    root: &Path,
    runtime: &Path,
    backup: &Path,
    lease: Option<&RuntimeLease>,
) -> Result<Value> {
    require_clear(runtime)?;
    let (backup, snapshot, store) = checked(backup)?;
    let added = matches_target(root, runtime, &snapshot, &store, None)?;
    let before = database_files(runtime)?;
    let current = if let Some(lease) = lease {
        Store::read_snapshot_with_lease(&runtime.join("state.db"), FILE_CAP, lease)
    } else {
        Store::read_snapshot(&runtime.join("state.db"), FILE_CAP)
    };
    let (revision, currentness) = match current {
        Ok(current) => {
            let p = current.project_by_root(root)?;
            if p.id != snapshot.project_id {
                return Err(conflict("target database belongs to another project"));
            }
            if awr_source::source_state_fingerprint(&current, p.id)?
                != snapshot.source_state_fingerprint
            {
                return Err(conflict("current database has different source bindings"));
            }
            (Some(p.project_revision), "owned_current_schema")
        }
        Err(error) => {
            if before.values().any(Option::is_some) {
                return Err(conflict(format!(
                    "target database ownership cannot be verified; retain and inspect it before recovery: {error}"
                )));
            }
            (
                None,
                "missing; ownership bound by matching backup/configuration/sources",
            )
        }
    };
    if database_files(runtime)? != before {
        return Err(conflict(
            "target database changed during restore preview; retry offline",
        ));
    }
    let mut value = json!({"version":1,"phase":"preview","read_only":true,"project_root":root,"project_id":snapshot.project_id,"backup":backup,"backup_fingerprint":snapshot.fingerprint,"program_sha256":snapshot.program.binary.sha256,"current_revision":revision,"current_database":currentness,"restore_revision":snapshot.project_revision,"before_database":before,"preserved_additional_runtime_files":added,"source_files_overwritten":0,"known_runtime_files_overwritten":0,"offline_required":true,"history_after_restore_revision_will_be_removed":revision.is_some_and(|r|r>snapshot.project_revision),"rollback_retained":true});
    value["fingerprint"] = json!(seal(&value)?);
    Ok(value)
}
fn journal_path(runtime: &Path, id: Id) -> PathBuf {
    runtime
        .join("restores")
        .join(id.to_string())
        .join("receipt.json")
}
fn save_journal(runtime: &Path, journal: &RestoreJournal) -> Result<()> {
    let path = journal_path(runtime, journal.id);
    let parent = path.parent().unwrap();
    let tmp = parent.join(format!("receipt-{}.tmp", Id::new()));
    write_new(&tmp, &serde_json::to_vec_pretty(journal)?, false)?;
    let dir = awr_source::open_dir_exact(parent)?;
    dir.rename(tmp.file_name().unwrap(), &dir, path.file_name().unwrap())?;
    sync_dir(parent)
}
fn pending_id(runtime: &Path) -> Result<Id> {
    let bytes = exact_read(
        &awr_store::restore_pending_path(&runtime.join("state.db")),
        128,
    )?;
    std::str::from_utf8(&bytes)
        .map_err(|_| conflict("invalid restore marker"))?
        .parse()
        .map_err(|_| conflict("invalid restore marker"))
}
fn read_journal(runtime: &Path, id: Id) -> Result<RestoreJournal> {
    let journal: RestoreJournal =
        serde_json::from_slice(&exact_read(&journal_path(runtime, id), 16 * 1024 * 1024)?)?;
    if journal.version != 1
        || journal.id != id
        || !["prepared", "completed"].contains(&journal.phase.as_str())
    {
        return Err(conflict("restore receipt identity differs"));
    }
    Ok(journal)
}
fn finish_restore(
    root: &Path,
    runtime: &Path,
    mut journal: RestoreJournal,
    lease: &RuntimeLease,
) -> Result<Value> {
    if journal.project_root != root || pending_id(runtime)? != journal.id {
        return Err(conflict("pending restore project identity differs"));
    }
    let (backup, snapshot, store) = checked(&journal.backup)?;
    if snapshot.fingerprint != journal.backup_fingerprint
        || snapshot.project_id != journal.project_id
        || snapshot.project_revision != journal.restored_revision
    {
        return Err(conflict("pending restore backup differs"));
    }
    matches_target(root, runtime, &snapshot, &store, Some(journal.id))?;
    if journal
        .before_database
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != DATABASE_FILES
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    {
        return Err(conflict("invalid recorded database inventory"));
    }
    let journal_dir = journal_path(runtime, journal.id)
        .parent()
        .unwrap()
        .to_owned();
    for (name, blob) in &journal.rollback {
        if !DATABASE_FILES.contains(&name.as_str()) || blob.path != format!("rollback/{name}") {
            return Err(conflict("invalid rollback binding"));
        }
        let bytes = blob_bytes(&journal_dir, blob)?;
        if journal.before_database.get(name) != Some(&Some(describe(name.clone(), &bytes))) {
            return Err(conflict("rollback bytes differ from recorded database"));
        }
    }
    for (name, before) in &journal.before_database {
        if before.is_some() != journal.rollback.contains_key(name) {
            return Err(conflict("incomplete rollback snapshot"));
        }
    }
    let current = database_files(runtime)?;
    let installed = current["state.db"].as_ref().is_some_and(|b| {
        b.sha256 == snapshot.database.sha256 && b.bytes == snapshot.database.bytes
    });
    if installed {
        if DATABASE_FILES[1..].iter().any(|n| current[*n].is_some()) {
            return Err(conflict(
                "restored database acquired new sidecars; recovery refuses external changes",
            ));
        }
    } else {
        if current["state.db"] != journal.before_database["state.db"]
            || DATABASE_FILES[1..]
                .iter()
                .any(|n| current[*n].is_some() && current[*n] != journal.before_database[*n])
        {
            return Err(conflict(
                "database bytes changed outside the recorded restore; retain pending state for inspection",
            ));
        }
        if journal.phase == "completed" {
            return Err(conflict(
                "completed restore database no longer matches; never replay history replacement",
            ));
        }
        let candidate = journal_dir.join("candidate.db");
        let bytes = blob_bytes(&backup, &snapshot.database)?;
        if exists(&candidate)? {
            if exact_read(&candidate, FILE_CAP)? != bytes {
                return Err(conflict("staged restore bytes changed"));
            }
        } else {
            write_new(&candidate, &bytes, false)?;
        }
        // Last compare before the replacement. Pending marker + lease block cooperating clients.
        if database_files(runtime)? != current {
            return Err(conflict("database changed before restore replacement"));
        }
        let dir = awr_source::open_dir_exact(runtime)?;
        for name in &DATABASE_FILES[1..] {
            if current[*name].is_some() {
                dir.remove_file(name)?;
            }
        }
        let from = awr_source::open_dir_exact(&journal_dir)?;
        from.rename("candidate.db", &dir, "state.db")?;
        sync_dir(runtime)?;
    }
    let restored = Store::read_snapshot_with_lease(&runtime.join("state.db"), FILE_CAP, lease)?;
    let p = restored.project_by_root(root)?;
    if !restored.doctor()?.ok
        || p.id != snapshot.project_id
        || p.project_revision != snapshot.project_revision
    {
        return Err(conflict(
            "restored database validation failed; rollback retained and opens remain blocked",
        ));
    }
    journal.phase = "completed".into();
    save_journal(runtime, &journal)?;
    fs::remove_file(awr_store::restore_pending_path(&runtime.join("state.db")))?;
    sync_dir(runtime)?;
    Ok(
        json!({"phase":"completed","restore_id":journal.id,"project_id":p.id,"project_revision":p.project_revision,"receipt":journal_path(runtime,journal.id),"rollback":journal_dir.join("rollback"),"source_files_overwritten":0,"known_runtime_files_overwritten":0,"next_action":"Restart clients, inspect doctor findings and explicitly compile fresh context; do not replay old host requests."}),
    )
}
fn restore(
    root: &Path,
    runtime: &Path,
    backup: &Path,
    expected: &str,
    offline: bool,
) -> Result<Value> {
    if !offline {
        return Err(Error::InvalidInput(
            "restore requires explicit offline acknowledgement".into(),
        ));
    }
    require_clear(runtime)?;
    let lease = RuntimeLease::exclusive(&runtime.join("state.db"))?;
    let plan = preview(root, runtime, backup, Some(&lease))?;
    if plan["fingerprint"].as_str() != Some(expected) {
        return Err(conflict("restore preview is stale; review a new preview"));
    }
    let (backup, snapshot, _) = checked(backup)?;
    let before_database = database_files(runtime)?;
    if serde_json::to_value(&before_database)? != plan["before_database"] {
        return Err(conflict("database changed after reviewed preview"));
    }
    let id = Id::new();
    private_dir(&runtime.join("restores"))?;
    let dir = runtime.join("restores").join(id.to_string());
    private_dir(&dir)?;
    private_dir(&dir.join("rollback"))?;
    let mut rollback = BTreeMap::new();
    for (name, before) in &before_database {
        if let Some(before) = before {
            let bytes = exact_read(&runtime.join(name), FILE_CAP)?;
            if describe(name.clone(), &bytes) != *before {
                return Err(conflict("database changed while retaining rollback"));
            }
            rollback.insert(
                name.clone(),
                write_payload(&dir, &format!("rollback/{name}"), &bytes, false)?,
            );
        }
    }
    let journal = RestoreJournal {
        version: 1,
        id,
        phase: "prepared".into(),
        created_at: awr_core::now_millis()?,
        project_root: root.to_owned(),
        project_id: snapshot.project_id,
        backup,
        backup_fingerprint: snapshot.fingerprint,
        preview: plan,
        before_database,
        rollback,
        restored_revision: snapshot.project_revision,
    };
    save_journal(runtime, &journal)?;
    write_new(
        &awr_store::restore_pending_path(&runtime.join("state.db")),
        id.to_string().as_bytes(),
        false,
    )?;
    finish_restore(root, runtime, journal, &lease)
}

pub fn run(root: &Path, command: &RuntimeCommand, _json_output: bool) -> Result<()> {
    // Backup checking does not require the original project to exist.
    let value = if let RuntimeCommand::Check { backup } = command {
        let backup = if backup.is_absolute() {
            backup.to_owned()
        } else {
            root.join(backup)
        };
        let (path, snapshot, _) = checked(&backup)?;
        json!({"phase":"verified","read_only":true,"backup":path,"snapshot":snapshot,"target_matches_checked":false})
    } else {
        let root = root.canonicalize()?;
        let runtime = crate::source::runtime_dir(&root, false)?;
        awr_source::open_dir_exact(&runtime)?;
        let resolve = |p: &Path| {
            if p.is_absolute() {
                p.to_owned()
            } else {
                root.join(p)
            }
        };
        match command {
            RuntimeCommand::Binding => binding(&root, &runtime)?,
            RuntimeCommand::Backup {
                output,
                companion,
                source_sha,
            } => backup(
                &root,
                &runtime,
                output,
                companion.as_deref(),
                source_sha.as_deref(),
            )?,
            RuntimeCommand::RestorePreview { backup } => {
                preview(&root, &runtime, &resolve(backup), None)?
            }
            RuntimeCommand::Restore {
                backup,
                expected_preview,
                offline,
            } => restore(
                &root,
                &runtime,
                &resolve(backup),
                expected_preview,
                *offline,
            )?,
            RuntimeCommand::RestoreStatus { id } => {
                let pending = exists(&awr_store::restore_pending_path(&runtime.join("state.db")))?;
                let id = if let Some(id) = id {
                    Some(*id)
                } else if pending {
                    Some(pending_id(&runtime)?)
                } else {
                    None
                };
                json!({"read_only":true,"pending":pending,"receipt":id.map(|id|read_journal(&runtime,id)).transpose()?})
            }
            RuntimeCommand::RestoreRecover { offline } => {
                if !offline {
                    return Err(Error::InvalidInput(
                        "recovery requires explicit offline acknowledgement".into(),
                    ));
                }
                let lease = RuntimeLease::exclusive(&runtime.join("state.db"))?;
                let id = pending_id(&runtime)?;
                finish_restore(&root, &runtime, read_journal(&runtime, id)?, &lease)?
            }
            RuntimeCommand::Check { .. } => unreachable!(),
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
