use crate::{REGISTERED_CONTENT_READ_CAP, Runtime};
use awr_core::*;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

impl Runtime<'_> {
    /// Explicit artifact body read. No content is returned until size and digest match its record.
    pub fn read_artifact(&self, id: Id, max_bytes: u64) -> Result<(Artifact, Vec<u8>)> {
        let artifact = self.store.artifact(self.project, id)?;
        ensure_public_data(&artifact)?;
        let bytes = read_registered_file(
            self.store,
            self.project,
            &artifact.locator,
            Some(artifact.size),
            Some(&artifact.sha256),
            max_bytes,
        )?;
        Ok((artifact, bytes))
    }

    /// Read a registered local report on demand. Absence of a recorded digest is not verification.
    pub fn read_evidence_report(
        &self,
        key: &str,
        max_bytes: u64,
    ) -> Result<(EvidenceRecord, Vec<u8>)> {
        let record = self.store.evidence(self.project, key)?;
        ensure_public_data(&record)?;
        let bytes = read_registered_file(
            self.store,
            self.project,
            &record.item.locator,
            None,
            record.item.sha256.as_deref(),
            max_bytes,
        )?;
        Ok((record, bytes))
    }
}

pub(crate) fn read_registered_file(
    store: &awr_store::Store,
    project: Id,
    locator: &str,
    expected_size: Option<u64>,
    expected_sha: Option<&str>,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    if max_bytes == 0 || max_bytes > REGISTERED_CONTENT_READ_CAP {
        return Err(Error::InvalidInput(
            "read limit must be 1..16777216 bytes".into(),
        ));
    }
    if expected_size.is_some_and(|size| size > max_bytes) {
        return Err(Error::InvalidInput(format!(
            "artifact exceeds {max_bytes} byte read limit"
        )));
    }
    if locator.contains("://") {
        return Err(Error::Unsupported("body reads currently support registered local paths; use the report locator for remote content".into()));
    }
    let project = store.project(project)?;
    let root = project.root.canonicalize()?;
    if root != project.root {
        return Err(Error::SourceConflict(
            "project root moved since registration".into(),
        ));
    }
    let path = if Path::new(locator).is_absolute() {
        Path::new(locator).to_path_buf()
    } else {
        root.join(locator)
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
            "registered file is outside project and authorized roots".into(),
        ));
    }
    if !path.metadata()?.is_file() {
        return Err(Error::InvalidInput(
            "registered content is not a regular file".into(),
        ));
    }
    let file = awr_source::open_file_exact(&path)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(Error::InvalidInput(
            "registered content is not a regular file".into(),
        ));
    }
    if before.len() > max_bytes {
        return Err(Error::InvalidInput(format!(
            "content has {} bytes; limit is {max_bytes}",
            before.len()
        )));
    }
    if expected_size.is_some_and(|size| size != before.len()) {
        return Err(Error::SourceConflict(
            "artifact size differs from its registered metadata".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&file).take(max_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(Error::InvalidInput("content grew beyond read limit".into()));
    }
    let after = file.metadata()?;
    if bytes.len() as u64 != before.len()
        || after.len() != before.len()
        || after.modified()? != before.modified()?
    {
        return Err(Error::SourceConflict(
            "registered content changed during reading".into(),
        ));
    }
    if let Some(expected) = expected_sha {
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(Error::SourceConflict(
                "registered content SHA256 does not match".into(),
            ));
        }
    }
    ensure_public_bytes(&bytes)?;
    Ok(bytes)
}
