use crate::execution::OutboxDelivery;
use crate::graph::path_within_scope;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashPoint {
    None,
    BeforeJournal,
    AfterJournalBeforeEffect,
    AfterEffectBeforeReport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerOutcome {
    pub execution_id: String,
    pub effect_key: String,
    pub state: String,
    pub unknown: bool,
    pub started: bool,
    pub observed_paths: Vec<String>,
    pub output_digest: Option<String>,
    pub environment_digest: String,
    pub scope_violation: bool,
    pub exactly_once_supported: bool,
    #[serde(default)]
    pub error: Option<String>,
    /// Files touched but whose final state is uncertain after an I/O error
    /// (may be truncated or partially written). Never silently reported as
    /// "not executed" (CR #58 P2-4).
    #[serde(default)]
    pub partial_paths: Vec<String>,
}

pub struct ReferenceRunner {
    journal_dir: PathBuf,
    worktree_root: PathBuf,
}

enum JournalLoad {
    Missing,
    Owned(RunnerOutcome),
    Corrupt(String),
}

/// RAII holder for an OS-level advisory lock. File locks are released when
/// the process dies or the handle closes — no orphan lock files, unlike
/// create_new marker files (CR #58 r4 P2-2).
struct OsLock {
    _file: fs::File,
}

impl ReferenceRunner {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            journal_dir: root.join("journal"),
            worktree_root: root.join("worktree"),
        }
    }

    fn base_outcome_impl(&self, delivery: &OutboxDelivery, state: &str) -> RunnerOutcome {
        RunnerOutcome {
            execution_id: delivery.execution_id.clone(),
            effect_key: delivery.effect_key.clone(),
            state: state.into(),
            unknown: false,
            started: false,
            observed_paths: vec![],
            output_digest: None,
            environment_digest: env_digest(&self.worktree_root),
            scope_violation: false,
            exactly_once_supported: delivery.fencing_class != "uncontrolled",
            error: None,
            partial_paths: vec![],
        }
    }

    /// Persist a recovery barrier under the same lock held throughout effects.
    /// Waits for a current writer; after this returns, older tokens cannot write.
    /// Call for every barrier returned by restore before clearing recovery state.
    pub fn install_recovery_barrier(&self, barrier: &crate::FencingBarrier) -> Result<(), String> {
        fs::create_dir_all(self.fencing_dir()).map_err(|e| e.to_string())?;
        let delivery = OutboxDelivery {
            coordinator_epoch: barrier.coordinator_epoch.clone(),
            tenant_id: barrier.tenant_id.clone(),
            project_id: barrier.project_id.clone(),
            scope_id: barrier.scope_id.clone(),
            work_id: barrier.work_id.clone(),
            fence: barrier.fence,
            outbox_id: String::new(),
            execution_id: String::new(),
            effect_key: String::new(),
            fencing_class: "hard_fence".into(),
            declared_scope: json!([]),
            payload: json!({}),
            delivery_attempts: 0,
        };
        let _generation = self.acquire_generation(&delivery, true)?;
        let _guard = self.acquire_work_fence(&delivery, true)?;
        Ok(())
    }

    /// Visible to pg-tests for crash-recovery fixtures.
    pub fn base_outcome(&self, delivery: &OutboxDelivery, state: &str) -> RunnerOutcome {
        self.base_outcome_impl(delivery, state)
    }

    fn fencing_dir(&self) -> PathBuf {
        self.journal_dir.join("fencing")
    }
    fn exec_lock_dir(&self) -> PathBuf {
        self.journal_dir.join("locks")
    }

    /// The fencing ledger key is the FULL token identity
    /// (tenant/project/scope/work), percent-encoded into an unambiguous file
    /// name; ledgers and locks live in separate namespaces, so a work id
    /// like "task.lock" cannot collide with another work's lock (CR #58 r4
    /// P2-3).
    fn fence_paths(&self, delivery: &OutboxDelivery) -> (PathBuf, PathBuf) {
        let key = fence_key(
            &delivery.tenant_id,
            &delivery.project_id,
            &delivery.scope_id,
            &delivery.work_id,
        );
        (
            self.fencing_dir().join(format!("ledger-{key}")),
            self.fencing_dir().join(format!("lock-{key}")),
        )
    }

    /// Acquire an OS advisory lock with bounded spin; the lock releases on
    /// process death (CR #58 r4 P2-2).
    fn acquire_os_lock(&self, path: &Path, what: &str) -> Result<OsLock, String> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| format!("{what} lock open failed: {e}"))?;
        let mut attempts = 0;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(OsLock { _file: file }),
                Err(fs::TryLockError::WouldBlock) => {
                    attempts += 1;
                    if attempts > 40 {
                        return Err(format!("{what} busy"));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(error) => return Err(format!("{what} lock failed: {error}")),
            }
        }
    }

    /// One durable generation per PROJECT, not per epoch or work. This also
    /// retires delayed commands for work created after the database backup.
    /// The trusted recovery controller installs barriers; delivery cannot rotate
    /// the generation. Hold this lock throughout each effect and installation.
    fn acquire_generation(
        &self,
        delivery: &OutboxDelivery,
        install: bool,
    ) -> Result<OsLock, String> {
        if delivery.coordinator_epoch.is_empty() {
            return Err("missing coordinator generation; recovery barrier required".into());
        }
        let key = fence_key(&delivery.tenant_id, &delivery.project_id, "", "");
        let path = self.fencing_dir().join(format!("generation-{key}"));
        let lock = self.fencing_dir().join(format!("generation-lock-{key}"));
        let guard = self.acquire_os_lock(&lock, "generation ledger")?;
        let identity = json!([delivery.tenant_id, delivery.project_id]);
        let mut record = match fs::read(&path) {
            Ok(bytes) => {
                let value: Value =
                    serde_json::from_slice(&bytes).map_err(|_| "generation ledger is corrupt")?;
                if value["identity"] != identity
                    || !value["epoch"].is_string()
                    || !value["retired"]
                        .as_array()
                        .is_some_and(|a| a.iter().all(Value::is_string))
                {
                    return Err("generation ledger is corrupt".into());
                }
                value
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                json!({"identity": identity, "epoch": delivery.coordinator_epoch, "retired": []})
            }
            Err(e) => return Err(format!("generation ledger unreadable: {e}")),
        };
        if record["epoch"] != delivery.coordinator_epoch {
            if !install {
                return Err("stale coordinator generation".into());
            }
            if record["retired"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| *v == delivery.coordinator_epoch)
            {
                return Err("retired coordinator generation".into());
            }
            let old = record["epoch"].clone();
            record["retired"].as_array_mut().unwrap().push(old);
            record["epoch"] = json!(delivery.coordinator_epoch);
        }
        // Sync before allowing any effect. A torn record fails closed on restart.
        use std::io::Write;
        let mut file = fs::File::create(&path).map_err(|e| e.to_string())?;
        file.write_all(record.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        Ok(guard)
    }

    fn acquire_fence(&self, delivery: &OutboxDelivery) -> Result<(OsLock, OsLock), String> {
        let generation = self.acquire_generation(delivery, false)?;
        let work = self.acquire_work_fence(delivery, false)?;
        Ok((generation, work))
    }

    /// Resource-end fencing under the per-identity OS lock. The caller keeps
    /// the guard for the WHOLE effect phase, so a stale executor that passed
    /// an earlier check cannot reorder its writes past a newer one
    /// (CR #58 r3/r4).
    fn acquire_work_fence(
        &self,
        delivery: &OutboxDelivery,
        install: bool,
    ) -> Result<OsLock, String> {
        let (ledger, lock) = self.fence_paths(delivery);
        let guard = self.acquire_os_lock(&lock, "fence ledger")?;
        let identity = json!([
            delivery.tenant_id,
            delivery.project_id,
            delivery.scope_id,
            delivery.work_id
        ]);
        let current: Option<i64> = match fs::read_to_string(&ledger) {
            Ok(raw) => {
                // Structured content with the original identity recorded for
                // audit; the digest file name alone is not the proof
                // (CR #58 r5 P2).
                let record: Value = serde_json::from_str(raw.trim())
                    .map_err(|_| "fence ledger is corrupt".to_string())?;
                if record.get("identity") != Some(&identity) {
                    return Err("fence ledger identity mismatch".to_string());
                }
                if !record["epoch"].is_string() && !install {
                    return Err("legacy fence ledger requires recovery barrier".into());
                }
                let fence = record
                    .get("fence")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| "fence ledger is corrupt".to_string())?;
                if record["epoch"] == delivery.coordinator_epoch {
                    Some(fence)
                } else {
                    None
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(format!("fence ledger unreadable: {error}")),
        };
        if let Some(current) = current {
            if delivery.fence < current {
                return Err(format!(
                    "stale fencing token {} (current {current})",
                    delivery.fence
                ));
            }
        }
        if current.map(|c| delivery.fence > c).unwrap_or(true) {
            // Writable handle + sync BEFORE relying on the ledger (Windows
            // cannot flush a read-only handle — CR #58 r4 P2-5).
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&ledger)
                .map_err(|e| format!("fence ledger persist failed: {e}"))?;
            use std::io::Write;
            let record = json!({"identity": identity, "epoch": delivery.coordinator_epoch, "fence": delivery.fence.to_string()});
            file.write_all(record.to_string().as_bytes())
                .map_err(|e| format!("fence ledger persist failed: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("fence ledger sync failed: {e}"))?;
        }
        Ok(guard)
    }

    /// Recovery ownership: a live duplicate returns the in-flight record
    /// unchanged; only once the execution OS lock is FREE (the owner is
    /// provably dead) may the record convert to unknown, and a terminal
    /// record found on re-read always wins (CR #58 r4 P2-4).
    fn recover_or_wait(&self, existing: RunnerOutcome) -> RunnerOutcome {
        let lock_path = self
            .exec_lock_dir()
            .join(format!("{}.lock", existing.execution_id));
        let guard = match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| e.to_string())
            .and_then(|file| match file.try_lock() {
                Ok(()) => Ok(file),
                Err(fs::TryLockError::WouldBlock) => Err("in-flight".to_string()),
                Err(error) => Err(error.to_string()),
            }) {
            Ok(file) => file,
            Err(_) => return existing, // owner provably alive
        };
        // The owner is dead: the OS released the lock.
        let current = match self.load(&existing.execution_id) {
            JournalLoad::Owned(current) => current,
            _ => existing,
        };
        drop(guard);
        if matches!(current.state.as_str(), "succeeded" | "failed") || current.unknown {
            return current;
        }
        let mut recovered = current;
        recovered.state = "unknown".into();
        recovered.unknown = true;
        recovered.error = Some("previous handler died mid-execution; effects uncertain".into());
        let _ = self.persist_result(&recovered);
        recovered
    }

    pub fn handle_delivery(&self, delivery: &OutboxDelivery, crash: CrashPoint) -> RunnerOutcome {
        self.handle_delivery_inner(delivery, crash, None)
    }

    pub(crate) fn handle_scoped_delivery(
        &self,
        delivery: &OutboxDelivery,
        deadline: std::time::Instant,
    ) -> RunnerOutcome {
        self.handle_delivery_inner(delivery, CrashPoint::None, Some(deadline))
    }

    fn handle_delivery_inner(
        &self,
        delivery: &OutboxDelivery,
        crash: CrashPoint,
        deadline: Option<std::time::Instant>,
    ) -> RunnerOutcome {
        if let Err(error) = fs::create_dir_all(&self.journal_dir)
            .and_then(|_| fs::create_dir_all(&self.fencing_dir()))
            .and_then(|_| fs::create_dir_all(&self.exec_lock_dir()))
            .and_then(|_| fs::create_dir_all(&self.worktree_root))
        {
            let mut outcome = self.base_outcome(delivery, "failed");
            outcome.error = Some(format!("runner directories unavailable: {error}"));
            return outcome;
        }
        match self.load(&delivery.execution_id) {
            JournalLoad::Owned(existing) => {
                if matches!(existing.state.as_str(), "succeeded" | "failed") || existing.unknown {
                    return existing;
                }
                return self.recover_or_wait(existing);
            }
            JournalLoad::Corrupt(error) => {
                let mut outcome = self.base_outcome(delivery, "unknown");
                outcome.unknown = true;
                outcome.error = Some(format!("journal unreadable: {error}"));
                return outcome;
            }
            JournalLoad::Missing => {}
        }
        // Execution ownership for the whole run; released by the OS on
        // death, so recovery can provably take over (CR #58 r4 P2-2/P2-4).
        let exec_lock = self
            .exec_lock_dir()
            .join(format!("{}.lock", delivery.execution_id));
        let _exec_guard = match self.acquire_os_lock(&exec_lock, "execution") {
            Ok(guard) => guard,
            Err(error) => {
                let mut outcome = self.base_outcome(delivery, "accepted");
                outcome.error = Some(format!("execution busy: {error}"));
                return outcome;
            }
        };
        if crash == CrashPoint::BeforeJournal {
            return self.base_outcome(delivery, "prepared");
        }
        // The ownership lock is held before touching the journal, so the
        // first handler cannot be shadowed mid-run (CR #58 r4 P2-4).
        if let Err(error) = self.persist_new(&self.base_outcome(delivery, "accepted")) {
            match error.kind() {
                ErrorKind::AlreadyExists => match self.load(&delivery.execution_id) {
                    JournalLoad::Owned(existing) => return existing,
                    JournalLoad::Corrupt(error) => {
                        let mut outcome = self.base_outcome(delivery, "unknown");
                        outcome.unknown = true;
                        outcome.error =
                            Some(format!("journal unreadable after admission race: {error}"));
                        return outcome;
                    }
                    JournalLoad::Missing => {
                        let mut outcome = self.base_outcome(delivery, "unknown");
                        outcome.unknown = true;
                        outcome.error = Some("journal vanished after admission race".into());
                        return outcome;
                    }
                },
                _ => {
                    let mut outcome = self.base_outcome(delivery, "failed");
                    outcome.error = Some(format!("journal persist failed: {error}"));
                    return outcome;
                }
            }
        }
        if crash == CrashPoint::AfterJournalBeforeEffect {
            let mut outcome = self.base_outcome(delivery, "unknown");
            outcome.unknown = true;
            let _ = self.persist_result(&outcome);
            return outcome;
        }
        // Validate the COMPLETE write plan — including the destination files
        // themselves — before any side effect (CR #41 P1-1, CR #58 r4 P1).
        let root_canon = match self.worktree_root.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                let mut outcome = self.base_outcome(delivery, "failed");
                outcome.error = Some(format!("worktree unavailable: {error}"));
                let _ = self.persist_result(&outcome);
                return outcome;
            }
        };
        let writes = delivery
            .payload
            .get("writes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let declared: Vec<String> = delivery
            .declared_scope
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
        let mut plan: Vec<(String, PathBuf, String)> = Vec::new();
        for item in &writes {
            let raw = item.get("path").and_then(Value::as_str).unwrap_or_default();
            let content = item
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let Some(rel) = normalize_write_path(raw) else {
                return self.reject_plan(delivery, format!("unsafe write path: {raw}"));
            };
            if !declared.iter().any(|item| path_within_scope(item, &rel)) {
                let mut outcome = self.base_outcome(delivery, "failed");
                outcome.scope_violation = true;
                outcome.observed_paths = vec![rel.clone()];
                outcome.error = Some(format!("path outside declared scope: {rel}"));
                let _ = self.persist_result(&outcome);
                return outcome;
            }
            let dest = self.worktree_root.join(&rel);
            // Pre-check the FULL chain including the final component, so a
            // known-illegal plan fails before ANY write (CR #58 r4 P1).
            if let Err(error) = self.verify_chain(&root_canon, &dest) {
                return self.reject_plan(delivery, error);
            }
            plan.push((rel, dest, content));
        }
        let _fence_guard = match self.acquire_fence(delivery) {
            Ok(guard) => guard,
            Err(error) => {
                let mut outcome = self.base_outcome(delivery, "failed");
                outcome.error = Some(error);
                let _ = self.persist_result(&outcome);
                return outcome;
            }
        };
        let mut observed = Vec::new();
        let mut partial = Vec::new();
        for (rel, dest, content) in &plan {
            if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                let mut outcome = self.base_outcome(delivery, "unknown");
                outcome.unknown = true;
                outcome.started = !observed.is_empty();
                outcome.observed_paths = observed;
                outcome.error = Some("admission lease elapsed; no further writes attempted".into());
                let _ = self.persist_result(&outcome);
                return outcome;
            }
            let result = self
                .verify_chain(&root_canon, dest)
                .map_err(|e| std::io::Error::new(ErrorKind::PermissionDenied, e))
                .and_then(|_| confined_write(&root_canon, rel, content));
            if let Err(error) = result {
                // A failed write does NOT mean the file is unchanged (CR #58 P2-4).
                partial.push(rel.clone());
                let mut outcome = self.base_outcome(delivery, "failed");
                outcome.started = true;
                outcome.observed_paths = observed.clone();
                outcome.partial_paths = partial.clone();
                outcome.output_digest = Some(output_digest(&self.worktree_root, &observed));
                outcome.error = Some(format!("write failed for {rel}: {error}"));
                let _ = self.persist_result(&outcome);
                return outcome;
            }
            observed.push(rel.clone());
        }
        let digest = output_digest(&self.worktree_root, &observed);
        if crash == CrashPoint::AfterEffectBeforeReport {
            let mut outcome = self.base_outcome(delivery, "unknown");
            outcome.unknown = true;
            outcome.started = true;
            outcome.observed_paths = observed;
            outcome.output_digest = Some(digest);
            let _ = self.persist_result(&outcome);
            return outcome;
        }
        let mut outcome = self.base_outcome(delivery, "succeeded");
        outcome.started = true;
        outcome.observed_paths = observed;
        outcome.output_digest = Some(digest);
        if let Err(error) = self.persist_result(&outcome) {
            let mut failed = outcome.clone();
            failed.state = "unknown".into();
            failed.unknown = true;
            failed.error = Some(format!("journal final persist failed: {error}"));
            return failed;
        }
        outcome
    }

    fn reject_plan(&self, delivery: &OutboxDelivery, error: String) -> RunnerOutcome {
        let mut outcome = self.base_outcome(delivery, "failed");
        outcome.scope_violation = true;
        outcome.error = Some(error);
        let _ = self.persist_result(&outcome);
        outcome
    }

    /// Every component from the canonical root down to the final file must
    /// be present-or-creatable and NOT a symlink (CR #41 P1-1, CR #58 r4 P1).
    fn verify_chain(&self, root_canon: &Path, dest: &Path) -> Result<(), String> {
        if let Ok(meta) = fs::symlink_metadata(dest) {
            if meta.file_type().is_symlink() {
                return Err(format!("write target is a symlink: {}", dest.display()));
            }
        }
        let mut ancestor = dest.parent().map(Path::to_path_buf);
        while let Some(dir) = ancestor.clone() {
            if dir.exists() {
                if fs::symlink_metadata(&dir)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false)
                {
                    return Err(format!("path contains a symlink: {}", dir.display()));
                }
                let canon = dir
                    .canonicalize()
                    .map_err(|e| format!("cannot resolve {}: {e}", dir.display()))?;
                if !canon.starts_with(root_canon) {
                    return Err(format!(
                        "write target escapes the worktree: {}",
                        dest.display()
                    ));
                }
                return Ok(());
            }
            ancestor = dir.parent().map(Path::to_path_buf);
        }
        Err(format!(
            "write target has no existing ancestor: {}",
            dest.display()
        ))
    }

    fn journal_path(&self, execution_id: &str) -> PathBuf {
        self.journal_dir.join(format!("{execution_id}.json"))
    }

    fn load(&self, execution_id: &str) -> JournalLoad {
        match fs::read(self.journal_path(execution_id)) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(outcome) => JournalLoad::Owned(outcome),
                Err(error) => JournalLoad::Corrupt(error.to_string()),
            },
            Err(error) if error.kind() == ErrorKind::NotFound => JournalLoad::Missing,
            Err(error) => JournalLoad::Corrupt(error.to_string()),
        }
    }

    fn persist_new(&self, outcome: &RunnerOutcome) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(outcome)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.journal_path(&outcome.execution_id))?;
        use std::io::Write;
        file.write_all(&bytes)?;
        file.sync_all()
    }

    fn persist_result(&self, outcome: &RunnerOutcome) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(outcome)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;
        let path = self.journal_path(&outcome.execution_id);
        // Unique temp name (no cross-writer tmp collisions), writable handle
        // synced BEFORE rename (read-only handles cannot flush on Windows —
        // CR #58 r4 P2-5), then an atomic replace.
        let tmp = path.with_extension(format!(
            "json.tmp.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        {
            let mut file = fs::File::create(&tmp)?;
            use std::io::Write;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        #[cfg(unix)]
        {
            if let Some(dir) = path.parent() {
                if let Ok(dir) = fs::File::open(dir) {
                    let _ = dir.sync_all();
                }
            }
        }
        Ok(())
    }
}

/// Fixed-length digest of the STRUCTURED token identity — field boundaries
/// are serialized before hashing, so identities containing separators never
/// collide, and the file name never exceeds the filesystem limit
/// (CR #58 r5 P2). Exposed to pg-tests for fixtures.
#[doc(hidden)]
pub fn fence_key(tenant: &str, project: &str, scope: &str, work: &str) -> String {
    let structured = json!([tenant, project, scope, work]).to_string();
    format!("{:x}", Sha256::digest(structured.as_bytes()))
}

/// Normalize a write path to a safe relative form; rejects absolute paths,
/// parent components, prefixes and NUL (CR #41 P1-1).
fn normalize_write_path(path: &str) -> Option<String> {
    if path.is_empty() || path.contains('\0') {
        return None;
    }
    let mut parts = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?.to_string()),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// Confined write: resolve every component relative to the worktree dir fd
/// with O_NOFOLLOW, so neither the final file NOR any intermediate directory
/// can be a symlink, and path resolution cannot escape the root (CR #58 r4
/// P1). Non-unix keeps the caller's lexical checks (weaker, documented).
#[cfg(unix)]
fn confined_write(root: &Path, rel: &str, content: &str) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::FromRawFd;

    fn cstr(bytes: &[u8]) -> std::io::Result<CString> {
        CString::new(bytes).map_err(|_| std::io::Error::new(ErrorKind::InvalidInput, "NUL"))
    }

    let root_fd = unsafe {
        libc::open(
            cstr(root.as_os_str().as_bytes())?.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut fd = root_fd;
    let components: Vec<&str> = rel.split('/').collect();
    for directory in &components[..components.len() - 1] {
        let name = cstr(directory.as_bytes())?;
        let next = unsafe {
            libc::openat(
                fd,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if next < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
            if unsafe { libc::mkdirat(fd, name.as_ptr(), 0o755) } < 0 {
                let error = std::io::Error::last_os_error();
                // A concurrent handler may have created the directory between
                // our openat and mkdirat: EEXIST is fine, reopen it with the
                // same confined flags (symlinks still refused). Anything else
                // is an error (CR #58 r5 P2).
                if error.raw_os_error() != Some(libc::EEXIST) {
                    unsafe { libc::close(fd) };
                    return Err(error);
                }
            }
            let opened = unsafe {
                libc::openat(
                    fd,
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            unsafe { libc::close(fd) };
            if opened < 0 {
                return Err(std::io::Error::last_os_error());
            }
            fd = opened;
        } else {
            unsafe { libc::close(fd) };
            if next < 0 {
                return Err(std::io::Error::last_os_error());
            }
            fd = next;
        }
    }
    let name = cstr(components[components.len() - 1].as_bytes())?;
    let file_fd = unsafe {
        libc::openat(
            fd,
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o644,
        )
    };
    unsafe { libc::close(fd) };
    if file_fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut file = unsafe { fs::File::from_raw_fd(file_fd) };
    file.write_all(content.as_bytes())?;
    file.sync_all()
}

#[cfg(not(unix))]
fn confined_write(_root: &Path, _rel: &str, _content: &str) -> std::io::Result<()> {
    // Protected writes are only implemented where the runner can open with
    // confined, no-follow resolution (unix). Degrading silently to a plain
    // check-then-write on other platforms would leave the reported TOCTOU
    // window open (CR #58 r5 P1).
    Err(std::io::Error::new(
        ErrorKind::Unsupported,
        "confined writes are only supported on unix platforms",
    ))
}

fn env_digest(worktree: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(worktree.to_string_lossy().as_bytes())
    )
}

fn output_digest(worktree: &Path, paths: &[String]) -> String {
    let mut hasher = Sha256::new();
    let mut ordered = paths.to_vec();
    ordered.sort();
    for path in ordered {
        hasher.update(path.as_bytes());
        if let Ok(bytes) = fs::read(worktree.join(&path)) {
            hasher.update(&bytes);
        }
    }
    format!("{:x}", hasher.finalize())
}
