//! Local, immutable review receipts. No source bodies are stored in the archive.
use awr_core::{
    ContentAssessment, Error, Result, SourceContentReview, VerifiedSourceContentReview,
};
use cap_fs_ext::DirExt;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectContentReview {
    #[serde(flatten)]
    pub review: SourceContentReview,
}
fn source_path(root: &Path, path: &Path) -> Result<(PathBuf, PathBuf, String)> {
    let root = root.canonicalize()?;
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
    .canonicalize()?;
    // Review is intentionally local to this project. External/Git sources retain strict guards.
    if !path.starts_with(&root)
        || (path.starts_with(root.join(".awr")) && !path.starts_with(root.join(".awr/intake")))
    {
        return Err(Error::RuleViolation("content review requires a file inside the project, outside runtime state (authored .awr/intake files are supported)".into()));
    }
    let locator = Url::from_file_path(&path)
        .map_err(|_| Error::InvalidInput("invalid review source path".into()))?
        .to_string();
    Ok((root, path, locator))
}
pub fn scan_content_review(root: &Path, path: &Path) -> Result<ProjectContentReview> {
    let (root, path, locator) = source_path(root, path)?;
    let bytes = crate::read_source_for_review(&path, crate::YAML_READ_CAP)?;
    Ok(ProjectContentReview {
        review: SourceContentReview {
            project_root: root.to_string_lossy().into(),
            version: awr_core::CONTENT_REVIEW_VERSION,
            assessment: ContentAssessment::scan(&bytes, &locator)?,
            reviewer: String::new(),
            reviewed_at: 0,
            decisions: vec![],
        },
    })
}
fn name(locator: &str, bytes: &[u8]) -> String {
    let key = crate::fingerprint(locator.as_bytes());
    let hash = crate::fingerprint(bytes);
    format!(
        "{}-{}-p{}.json",
        &key[7..],
        &hash[7..],
        awr_core::SECRET_POLICY_VERSION
    )
}
pub fn archive_content_review(root: &Path, receipt: &ProjectContentReview) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    if receipt.review.project_root != root.to_string_lossy() {
        return Err(Error::SourceConflict(
            "review belongs to another project root".into(),
        ));
    }
    let path = Url::parse(&receipt.review.assessment.locator)
        .ok()
        .and_then(|u| u.to_file_path().ok())
        .ok_or_else(|| {
            Error::InvalidInput("content review requires a local file locator".into())
        })?;
    let (_, path, locator) = source_path(&root, &path)?;
    let bytes = crate::read_source_for_review(&path, crate::YAML_READ_CAP)?;
    receipt.review.verify(&bytes, &locator)?;
    let archive_name = name(&locator, &bytes);
    let root_dir = crate::open_dir_exact(&root)?;
    match root_dir.create_dir(".awr") {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let runtime = root_dir.open_dir_nofollow(".awr")?;
    match runtime.create_dir("content-reviews") {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let archive = runtime.open_dir_nofollow("content-reviews")?;
    let encoded = serde_json::to_vec_pretty(receipt)?;
    let temporary = format!(".{}.tmp", awr_core::Id::new());
    let mut options = cap_std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let result = (|| -> Result<()> {
        let mut file = archive.open_with(&temporary, &options)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        // Link is atomic and cannot overwrite a prior receipt. Concurrent identical
        // acceptance is idempotent; a competing different receipt is a conflict.
        if let Err(error) = archive.hard_link(&temporary, &archive, &archive_name) {
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error.into());
            }
            if crate::read_source_capped(
                &root.join(".awr/content-reviews").join(&archive_name),
                1024 * 1024,
            )? != encoded
            {
                return Err(Error::SourceConflict(
                    "a different immutable receipt already covers this source version".into(),
                ));
            }
        }
        Ok(())
    })();
    let _ = archive.remove_file(&temporary);
    result?;
    Ok(root.join(".awr/content-reviews").join(archive_name))
}
pub(crate) fn load_review(
    root: &Path,
    locator: &str,
    bytes: &[u8],
) -> Result<Option<VerifiedSourceContentReview>> {
    let root = root.canonicalize()?;
    let path = root.join(".awr/content-reviews").join(name(locator, bytes));
    if !path.try_exists()? {
        return Ok(None);
    }
    let receipt: ProjectContentReview =
        serde_json::from_slice(&crate::read_source_capped(&path, 1024 * 1024)?)
            .map_err(|_| Error::InvalidInput("invalid archived content review".into()))?;
    if receipt.review.project_root != root.to_string_lossy() {
        return Err(Error::SourceConflict(
            "review belongs to another project root".into(),
        ));
    }
    receipt.review.verify(bytes, locator).map(Some)
}
pub fn read_project_document(root: &Path, path: &Path, cap: u64) -> Result<Vec<u8>> {
    let (_, path, _) = source_path(root, path)?;
    Ok(crate::Locator::File(path).read(root, cap)?.bytes)
}
