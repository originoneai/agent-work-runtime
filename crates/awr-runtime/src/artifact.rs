use crate::Runtime;
use awr_core::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

pub struct ArtifactFile {
    pub path: PathBuf,
    pub artifact_type: String,
    pub mime: String,
    pub source_event_id: Id,
    pub max_bytes: u64,
}
struct PendingFile {
    path: PathBuf,
    output: Option<File>,
    registered: bool,
}
impl Drop for PendingFile {
    fn drop(&mut self) {
        drop(self.output.take());
        if !self.registered {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Runtime<'_> {
    /// Copy a bounded file into immutable managed storage, then register only its metadata.
    pub fn import_artifact(
        &mut self,
        expected: Revision,
        request: ArtifactFile,
    ) -> Result<(Artifact, Event)> {
        let project = self.store.project(self.project)?;
        if project.project_revision != expected {
            return Err(Error::RevisionConflict {
                expected,
                actual: project.project_revision,
            });
        }
        self.store.event(self.project, request.source_event_id)?;
        if request.max_bytes == 0 {
            return Err(Error::InvalidInput(
                "artifact byte limit must be positive".into(),
            ));
        }
        let root = project.root.canonicalize()?;
        if root != project.root {
            return Err(Error::SourceConflict(
                "project root moved since registration".into(),
            ));
        }
        let path = if request.path.is_absolute() {
            request.path.clone()
        } else {
            root.join(&request.path)
        }
        .canonicalize()?;
        let allowed = path.starts_with(&root)
            || (root.join(".awr/project.toml").exists()
                && awr_source::Manifest::load(&root)?
                    .project
                    .authorized_roots
                    .iter()
                    .any(|p| {
                        root.join(p)
                            .canonicalize()
                            .is_ok_and(|r| path.starts_with(r))
                    }));
        if !allowed {
            return Err(Error::RuleViolation(
                "artifact source is outside project and authorized roots".into(),
            ));
        }
        let mut source = File::open(&path)?;
        let before = source.metadata()?;
        if !before.is_file() || before.len() > request.max_bytes {
            return Err(Error::InvalidInput(
                "artifact source must be a regular file within the byte limit".into(),
            ));
        }
        let runtime = root.join(".awr");
        if !runtime.exists() {
            fs::create_dir(&runtime)?;
        }
        let runtime = runtime.canonicalize()?;
        if !runtime.starts_with(&root) || !runtime.is_dir() {
            return Err(Error::RuleViolation(
                "runtime directory escapes project root".into(),
            ));
        }
        let directory = runtime.join("artifacts");
        if !directory.exists() {
            fs::create_dir(&directory)?;
        }
        let directory = directory.canonicalize()?;
        if !directory.starts_with(&runtime) || !directory.is_dir() {
            return Err(Error::RuleViolation(
                "artifact directory escapes runtime storage".into(),
            ));
        }
        let storage_id = Id::new();
        let destination = directory.join(storage_id.to_string());
        let output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&destination)?;
        let mut pending = PendingFile {
            path: destination.clone(),
            output: Some(output),
            registered: false,
        };
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 32 * 1024];
        loop {
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            size = size
                .checked_add(n as u64)
                .ok_or_else(|| Error::InvalidInput("artifact size overflow".into()))?;
            if size > request.max_bytes {
                return Err(Error::InvalidInput(
                    "artifact grew beyond the byte limit".into(),
                ));
            }
            pending.output.as_mut().unwrap().write_all(&buffer[..n])?;
            hasher.update(&buffer[..n]);
        }
        pending.output.as_ref().unwrap().sync_all()?;
        drop(pending.output.take());
        let after = source.metadata()?;
        if size != before.len()
            || after.len() != before.len()
            || after.modified()? != before.modified()?
        {
            return Err(Error::SourceConflict(
                "artifact source changed during copying".into(),
            ));
        }
        let locator = destination
            .strip_prefix(&root)
            .map_err(|_| Error::RuleViolation("artifact locator escapes project".into()))?
            .to_str()
            .ok_or_else(|| Error::InvalidInput("artifact path must be UTF-8".into()))?
            .to_string();
        let result = self.store.record_artifact(
            self.project,
            expected,
            ArtifactDraft {
                artifact_type: request.artifact_type,
                locator,
                sha256: format!("{:x}", hasher.finalize()),
                size,
                mime: request.mime,
                source_event_id: request.source_event_id,
            },
        )?;
        pending.registered = true;
        Ok(result)
    }
}
