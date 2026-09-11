//! Cooperative process lifetime lease for offline snapshot restore.
use awr_core::{Error, Result};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

pub struct RuntimeLease {
    file: File,
    database: PathBuf,
}
impl Drop for RuntimeLease {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}
pub fn pending_path(path: &Path) -> PathBuf {
    sibling(path, "-restore.pending")
}
fn pending(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(pending_path(path)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
impl RuntimeLease {
    pub(crate) fn shared(path: &Path, create: bool) -> Result<Option<Self>> {
        if pending(path)? {
            return Err(Error::Storage("runtime restore is pending; inspect runtime restore-status and recover before opening AWR".into()));
        }
        let lease = Self::open(path, create, false)?;
        if pending(path)? {
            return Err(Error::Storage(
                "runtime restore started while opening AWR; retry after recovery".into(),
            ));
        }
        Ok(lease)
    }
    pub fn exclusive(path: &Path) -> Result<Self> {
        Self::open(path, true, true)?
            .ok_or_else(|| Error::Storage("runtime lease unavailable".into()))
    }
    fn open(path: &Path, create: bool, exclusive: bool) -> Result<Option<Self>> {
        let lock = sibling(path, "-runtime.lock");
        match std::fs::symlink_metadata(&lock) {
            Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
                return Err(Error::RuleViolation(
                    "runtime lease must be a regular file".into(),
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !create => return Ok(None),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => (),
        }
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(create)
            .create(create)
            .truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&lock)?;
        let result = if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        result.map_err(|_| {
            Error::Storage(
                "runtime is in use or an offline restore is active; close AWR clients and retry"
                    .into(),
            )
        })?;
        Ok(Some(Self {
            file,
            database: path.to_owned(),
        }))
    }
    pub fn check_database(&self, path: &Path) -> Result<()> {
        if self.database == path {
            Ok(())
        } else {
            Err(Error::RuleViolation(
                "runtime lease belongs to another database".into(),
            ))
        }
    }
}
