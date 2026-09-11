//! An optimistic, bounded capture; SQLite never opens the originating files.
use awr_core::{Error, Id, Result};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: SystemTime,
    identity: (u64, u64),
}
fn stamp(path: &Path) -> Result<Option<Stamp>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if !metadata.is_file() {
        return Err(Error::RuleViolation(
            "snapshot requires regular database files".into(),
        ));
    }
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        (metadata.dev(), metadata.ino())
    };
    #[cfg(not(unix))]
    let identity = (0, 0);
    Ok(Some(Stamp {
        length: metadata.len(),
        modified: metadata.modified()?,
        identity,
    }))
}
pub(super) struct Files {
    pub database: PathBuf,
    directory: PathBuf,
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
impl Files {
    pub fn capture(database: &Path, max_bytes: u64) -> Result<Self> {
        let sidecar = |suffix: &str| {
            let mut name = database.as_os_str().to_owned();
            name.push(suffix);
            PathBuf::from(name)
        };
        // A journal requires recovery and is not silently interpreted as a settled WAL view.
        if stamp(&sidecar("-journal"))?.is_some() {
            return Err(Error::SourceConflict(
                "database has a rollback journal; retry after its writer finishes".into(),
            ));
        }
        let paths = [database.to_path_buf(), sidecar("-wal")];
        let before = paths.iter().map(|p| stamp(p)).collect::<Result<Vec<_>>>()?;
        if before[0].is_none() {
            return Err(Error::NotFound("AWR database".into()));
        }
        let size: u64 = before.iter().flatten().map(|s| s.length).sum();
        if max_bytes == 0 || size > max_bytes {
            return Err(Error::InvalidInput(format!(
                "database and WAL exceed the {max_bytes} byte snapshot limit"
            )));
        }
        let mut images = Vec::new();
        for (path, metadata) in paths.iter().zip(&before) {
            if let Some(metadata) = metadata {
                let mut bytes = Vec::new();
                fs::File::open(path)?
                    .take(metadata.length + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() as u64 != metadata.length {
                    return Err(changed());
                }
                images.push(Some(bytes));
            } else {
                images.push(None);
            }
        }
        // All input files must remain unchanged across both complete reads. An active
        // writer is a retryable conflict, not a corrupt or permanently failed source.
        for (path, image) in paths.iter().zip(&images) {
            if let Some(image) = image {
                let mut second = Vec::new();
                fs::File::open(path)?
                    .take(image.len() as u64 + 1)
                    .read_to_end(&mut second)?;
                if &second != image {
                    return Err(changed());
                }
            }
        }
        let after = paths.iter().map(|p| stamp(p)).collect::<Result<Vec<_>>>()?;
        if before != after || stamp(&sidecar("-journal"))?.is_some() {
            return Err(changed());
        }
        let directory = std::env::temp_dir().join(format!("awr-preview-{}", Id::new()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory)?;
        let result = Self {
            database: directory.join("state.db"),
            directory,
        };
        for (name, image) in ["state.db", "state.db-wal"].iter().zip(images) {
            if let Some(image) = image {
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                options
                    .open(result.directory.join(name))?
                    .write_all(&image)?;
            }
        }
        Ok(result)
    }
}
fn changed() -> Error {
    Error::SourceConflict("database changed during read-only capture; retry the preview".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    #[test]
    fn live_wal_is_included_without_touching_original_files() {
        let directory = std::env::temp_dir().join(format!("awr-preview-test-{}", Id::new()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("state.db");
        let store = Store::open(&path).unwrap();
        store.conn.execute_batch("PRAGMA wal_autocheckpoint=0; CREATE TABLE fixture (value TEXT); INSERT INTO fixture VALUES('committed in WAL');").unwrap();
        let files = || {
            fs::read_dir(&directory)
                .unwrap()
                .map(|p| {
                    let p = p.unwrap().path();
                    (p.clone(), fs::read(p).unwrap())
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        assert!(store.require_memory().is_err());
        let before = files();
        let copy = Store::preview_snapshot(&path, 8 * 1024 * 1024).unwrap();
        assert_eq!(
            copy.conn
                .query_row("SELECT value FROM fixture", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "committed in WAL"
        );
        assert!(copy.require_memory().is_ok());
        assert!(files() == before, "original database and sidecars changed");
        assert!(Store::preview_snapshot(&path, 1).is_err());
        drop(copy);
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }
}
