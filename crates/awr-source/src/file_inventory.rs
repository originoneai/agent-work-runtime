//! Read-only file freshness for host adapters with broader source trees than AWR projections.
use crate::open_file_exact;
use awr_core::{Error, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

const SCHEMA_VERSION: u32 = 1;
const MAX_FILES: usize = 20_000;
const MAX_TOTAL_BYTES: usize = 1024 * 1024 * 1024;
const MAX_PATHS: usize = 64;
const MAX_CHANGES: usize = 12;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileInventory {
    pub schema_version: u32,
    pub includes: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub files: BTreeMap<String, String>,
    pub digest: String,
}

impl FileInventory {
    fn computed_digest(&self) -> Result<String> {
        let bytes = serde_json::to_vec(&(
            self.schema_version,
            &self.includes,
            &self.exclude_globs,
            &self.files,
        ))?;
        Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
    }

    pub fn verify(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION || self.digest != self.computed_digest()? {
            return Err(Error::SourceConflict(
                "file inventory has an unsupported schema or invalid digest".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Missing,
    Changed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InventoryChange {
    pub path: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryDiff {
    pub fresh: bool,
    pub total_changes: usize,
    pub changes: Vec<InventoryChange>,
    pub omitted_changes: usize,
}

fn relative(path: &Path) -> Result<String> {
    let parts = path
        .components()
        .map(|part| match part {
            Component::Normal(value) => value
                .to_str()
                .filter(|value| !value.contains('\\'))
                .map(str::to_string)
                .ok_or_else(|| Error::InvalidInput("file inventory path must be UTF-8".into())),
            _ => Err(Error::RuleViolation(
                "file inventory paths must be relative without dot or parent components".into(),
            )),
        })
        .collect::<Result<Vec<_>>>()?;
    if parts.is_empty() {
        return Err(Error::InvalidInput(
            "file inventory requires non-empty relative paths".into(),
        ));
    }
    Ok(parts.join("/"))
}

fn excluded(path: &str, globs: &GlobSet) -> bool {
    path.split('/').any(|part| {
        part.eq_ignore_ascii_case(".DS_Store")
            || part.eq_ignore_ascii_case(".git")
            || part.eq_ignore_ascii_case(".awr")
    }) || globs.is_match(path)
}

fn exclusion_glob(pattern: &str) -> Result<globset::Glob> {
    GlobBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|error| Error::InvalidInput(error.to_string()))
}

fn hash_file(path: &Path, total_bytes: &mut usize) -> Result<String> {
    let mut file = open_file_exact(path)?;
    let before = file.metadata()?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        *total_bytes = total_bytes.saturating_add(count);
        if *total_bytes > MAX_TOTAL_BYTES {
            return Err(Error::BudgetExceeded {
                required: *total_bytes,
                budget: MAX_TOTAL_BYTES,
            });
        }
        digest.update(&buffer[..count]);
    }
    let after = file.metadata()?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(Error::SourceStale(format!(
            "file changed during inventory: {}",
            path.display()
        )));
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn walk(
    root: &Path,
    path: &str,
    globs: &GlobSet,
    files: &mut BTreeMap<String, String>,
    total_bytes: &mut usize,
) -> Result<()> {
    if excluded(path, globs) {
        return Ok(());
    }
    let full = root.join(path);
    let metadata = match fs::symlink_metadata(&full) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Err(Error::RuleViolation(format!(
            "file inventory refuses symbolic link {path}"
        )));
    }
    if metadata.is_dir() {
        let directory = crate::open_dir_exact(&full)?;
        let mut names = directory
            .entries()
            .map_err(Error::Io)?
            .map(|entry| entry.map(|value| value.file_name()).map_err(Error::Io))
            .collect::<Result<Vec<_>>>()?;
        names.sort();
        for name in names {
            let name = name
                .to_str()
                .ok_or_else(|| Error::InvalidInput("file inventory path must be UTF-8".into()))?;
            walk(root, &format!("{path}/{name}"), globs, files, total_bytes)?;
        }
    } else if metadata.is_file() {
        if files.len() >= MAX_FILES && !files.contains_key(path) {
            return Err(Error::BudgetExceeded {
                required: files.len() + 1,
                budget: MAX_FILES,
            });
        }
        files.insert(path.to_string(), hash_file(&full, total_bytes)?);
    } else {
        return Err(Error::RuleViolation(format!(
            "file inventory requires regular files: {path}"
        )));
    }
    Ok(())
}

/// Inventory selected project-relative paths without touching AWR runtime state.
/// Finder metadata and the local Git/AWR runtime directories are never authority.
pub fn inventory_files(
    root: &Path,
    includes: &[PathBuf],
    exclude_globs: &[String],
) -> Result<FileInventory> {
    if includes.is_empty() || includes.len() > MAX_PATHS || exclude_globs.len() > MAX_PATHS {
        return Err(Error::InvalidInput(
            "file inventory requires 1..64 includes and at most 64 excludes".into(),
        ));
    }
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(Error::InvalidInput(
            "file inventory project root must be a directory".into(),
        ));
    }
    let includes = includes
        .iter()
        .map(|path| relative(path))
        .collect::<Result<BTreeSet<_>>>()?
        .into_iter()
        .collect::<Vec<_>>();
    let exclude_globs = exclude_globs
        .iter()
        .map(|pattern| {
            if pattern.is_empty() || pattern.len() > 256 || pattern.starts_with('/') {
                return Err(Error::InvalidInput(
                    "file inventory exclude glob must be a short relative pattern".into(),
                ));
            }
            exclusion_glob(pattern)?;
            Ok(pattern.clone())
        })
        .collect::<Result<BTreeSet<_>>>()?
        .into_iter()
        .collect::<Vec<_>>();
    let mut globs = GlobSetBuilder::new();
    for pattern in &exclude_globs {
        globs.add(exclusion_glob(pattern)?);
    }
    let globs = globs
        .build()
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    let mut files = BTreeMap::new();
    let mut total_bytes = 0;
    for path in &includes {
        walk(&root, path, &globs, &mut files, &mut total_bytes)?;
    }
    let mut snapshot = FileInventory {
        schema_version: SCHEMA_VERSION,
        includes,
        exclude_globs,
        files,
        digest: String::new(),
    };
    snapshot.digest = snapshot.computed_digest()?;
    Ok(snapshot)
}

/// Compare an integrity-checked baseline with the same selection policy.
/// Diagnostics contain only a bounded set of relative paths, never file hashes.
pub fn compare_file_inventories(
    before: &FileInventory,
    after: &FileInventory,
) -> Result<InventoryDiff> {
    before.verify()?;
    after.verify()?;
    if before.includes != after.includes || before.exclude_globs != after.exclude_globs {
        return Err(Error::SourceConflict(
            "file inventory selection policy changed; review and create a new baseline".into(),
        ));
    }
    let mut total_changes = 0;
    let mut changes = Vec::new();
    for path in before
        .files
        .keys()
        .chain(after.files.keys())
        .collect::<BTreeSet<_>>()
    {
        if before.files.get(path) == after.files.get(path) {
            continue;
        }
        total_changes += 1;
        if changes.len() >= MAX_CHANGES {
            continue;
        }
        let kind = if !before.files.contains_key(path) {
            ChangeKind::Added
        } else if !after.files.contains_key(path) {
            ChangeKind::Missing
        } else {
            ChangeKind::Changed
        };
        let display = if path.chars().count() > 160 {
            format!("{}...", path.chars().take(157).collect::<String>())
        } else {
            (*path).clone()
        };
        changes.push(InventoryChange {
            path: display,
            kind,
        });
    }
    Ok(InventoryDiff {
        fresh: total_changes == 0,
        total_changes,
        omitted_changes: total_changes.saturating_sub(changes.len()),
        changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use awr_core::Id;

    #[test]
    fn finder_metadata_does_not_stale_real_sources() {
        let root = std::env::temp_dir().join(format!("awr-file-inventory-{}", Id::new()));
        fs::create_dir_all(root.join("docs/visuals")).unwrap();
        let includes = [PathBuf::from("docs")];
        let excludes = ["docs/generated/**".to_string()];
        let before = inventory_files(&root, &includes, &excludes).unwrap();
        fs::write(root.join("docs/visuals/.DS_Store"), b"Finder metadata").unwrap();
        fs::write(root.join("docs/visuals/real.md"), b"first").unwrap();
        let added = inventory_files(&root, &includes, &excludes).unwrap();
        assert!(!added.files.contains_key("docs/visuals/.DS_Store"));
        assert_eq!(
            compare_file_inventories(&before, &added).unwrap().changes[0].kind,
            ChangeKind::Added
        );
        fs::write(
            root.join("docs/visuals/.DS_Store"),
            b"changed Finder metadata",
        )
        .unwrap();
        assert_eq!(inventory_files(&root, &includes, &excludes).unwrap(), added);
        fs::write(root.join("docs/visuals/real.md"), b"second").unwrap();
        let changed = inventory_files(&root, &includes, &excludes).unwrap();
        assert_eq!(
            compare_file_inventories(&added, &changed).unwrap().changes[0].kind,
            ChangeKind::Changed
        );
        fs::remove_file(root.join("docs/visuals/real.md")).unwrap();
        let missing = inventory_files(&root, &includes, &excludes).unwrap();
        assert_eq!(
            compare_file_inventories(&changed, &missing)
                .unwrap()
                .changes[0]
                .kind,
            ChangeKind::Missing
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn policy_drift_and_tampered_baselines_fail_closed() {
        let root = std::env::temp_dir().join(format!("awr-file-inventory-{}", Id::new()));
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs/source.md"), b"source").unwrap();
        let first = inventory_files(&root, &[PathBuf::from("docs")], &[]).unwrap();
        let other = inventory_files(&root, &[PathBuf::from("docs/source.md")], &[]).unwrap();
        assert!(compare_file_inventories(&first, &other).is_err());
        let mut tampered = first.clone();
        tampered.files.clear();
        assert!(compare_file_inventories(&tampered, &first).is_err());
        assert!(inventory_files(&root, &[PathBuf::from("../outside")], &[]).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exclusions_and_diagnostics_are_bounded() {
        let root = std::env::temp_dir().join(format!("awr-file-inventory-{}", Id::new()));
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs/.ENV"), b"synthetic secret").unwrap();
        fs::write(root.join("docs/KEY.PEM"), b"synthetic key").unwrap();
        let includes = [PathBuf::from("docs")];
        let excludes = ["**/.env".to_string(), "**/*.pem".to_string()];
        let baseline = inventory_files(&root, &includes, &excludes).unwrap();
        assert!(baseline.files.is_empty());
        for index in 0..16 {
            fs::write(root.join(format!("docs/source-{index}.md")), b"source").unwrap();
        }
        let current = inventory_files(&root, &includes, &excludes).unwrap();
        let diff = compare_file_inventories(&baseline, &current).unwrap();
        assert_eq!(diff.total_changes, 16);
        assert_eq!(diff.changes.len(), MAX_CHANGES);
        assert_eq!(diff.omitted_changes, 4);
        assert!(!serde_json::to_string(&diff).unwrap().contains("sha256:"));
        fs::remove_dir_all(root).unwrap();
    }
}
