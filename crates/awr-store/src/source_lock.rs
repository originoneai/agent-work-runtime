use crate::Store;
use awr_core::{Error, Result};
use std::{
    fs::{File, OpenOptions},
    time::{Duration, Instant},
};

pub struct SourceLock {
    database: String,
    _file: Option<File>,
}
impl Store {
    fn database_path(&self) -> Result<String> {
        self.conn
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name='main'",
                [],
                |r| r.get(0),
            )
            .map_err(crate::db_error)
    }
    /// Serialize source/configuration transitions across processes, with a bounded wait.
    pub fn lock_sources(&self) -> Result<SourceLock> {
        let database = self.database_path()?;
        if database.is_empty() {
            return Ok(SourceLock {
                database,
                _file: None,
            });
        }
        let path = format!("{database}-sources.lock");
        if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file()) {
            return Err(Error::RuleViolation(
                "source lock must be a regular file".into(),
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let start = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => {
                    return Ok(SourceLock {
                        database,
                        _file: Some(file),
                    });
                }
                Err(std::fs::TryLockError::WouldBlock)
                    if start.elapsed() < Duration::from_secs(5) =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    return Err(Error::SourceConflict(
                        "source transition is busy; retry after the current operation".into(),
                    ));
                }
                Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
            }
        }
    }
    pub fn check_source_lock(&self, guard: &SourceLock) -> Result<()> {
        if self.database_path()? == guard.database {
            Ok(())
        } else {
            Err(Error::RuleViolation(
                "source lock belongs to another database".into(),
            ))
        }
    }
}
