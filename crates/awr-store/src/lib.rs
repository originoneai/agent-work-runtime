//! SQLite persistence. Callers use domain operations, never an exposed SQL handle.
mod catalog;
mod checkpoint;
mod checkpoint_save;
mod delta;
mod events;
mod evidence;
mod handoff;
mod mutation;
mod mutation_apply;
mod projection;
mod query;
mod reconcile;
mod resume;
mod search;
mod session;
mod source_changes;
mod transaction;
mod work;
use awr_core::{Error, Result, now_millis};
pub use catalog::SourceRegistration;
pub use checkpoint_save::{
    CheckpointAttempt, CheckpointAttempts, SessionDeltaSnapshot, SourceObservation,
};
pub use delta::{
    DeltaEvents, EntityDelta, EventReference, HistoryCount, ImportantEvent, SourceDelta,
};
pub use events::{BranchFilter, EventCursor, EventPage, EventQuery};
pub use reconcile::{ReconcileAction, ReconcileReceipt, RuntimeFinding, RuntimeInspection};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
pub use search::{SearchHit, SearchQuery, SearchReport};
use serde::Serialize;
pub use source_changes::{ProjectionChange, SourceState};
use std::{path::Path, time::Duration};

const APPLICATION_ID: i64 = 0x41575231;
const SCHEMA_VERSION: i64 = 3;
const CATALOG_SQL: &str = include_str!("../migrations/001_catalog.sql");
const DOMAIN_SQL: &str = include_str!("../migrations/002_domain.sql");
const SEARCH_SQL: &str = include_str!("../migrations/003_search.sql");

pub struct Store {
    pub(crate) conn: Connection,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub application_id: i64,
    pub schema_version: i64,
    pub sqlite_version: String,
    pub journal_mode: String,
    pub foreign_keys: bool,
    pub busy_timeout_ms: i64,
    pub migrations: Vec<MigrationInfo>,
    pub integrity: Vec<String>,
    pub foreign_key_violations: usize,
}

#[derive(Debug, Serialize)]
pub struct MigrationInfo {
    pub version: i64,
    pub name: String,
    pub applied_at: i64,
}

pub(crate) fn db_error(error: rusqlite::Error) -> Error {
    Error::Storage(error.to_string())
}

impl Store {
    /// Open the current schema for queries without creating or upgrading database state.
    pub fn open_readonly(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(Error::NotFound(format!("AWR database: {}", path.display())));
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(db_error)?;
        Self::configure(&conn)?;
        let owner: i64 = conn
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(db_error)?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(db_error)?;
        if owner != APPLICATION_ID || version != SCHEMA_VERSION {
            return Err(Error::Storage(
                "read queries require a current AWR database; use doctor to inspect it".into(),
            ));
        }
        let store = Self { conn };
        store.verify_catalog()?;
        Ok(store)
    }

    /// Open an existing current-schema database for domain writes without creating or migrating it.
    pub fn open_existing(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(Error::NotFound(format!("AWR database: {}", path.display())));
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(db_error)?;
        Self::configure(&conn)?;
        let owner: i64 = conn
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(db_error)?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(db_error)?;
        if owner != APPLICATION_ID || version != SCHEMA_VERSION {
            return Err(Error::Storage(
                "writes require an existing current AWR database; inspect it before repair".into(),
            ));
        }
        let store = Self { conn };
        store.verify_catalog()?;
        Ok(store)
    }

    /// Create or reopen an AWR database. Refuse unrelated databases.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path).map_err(db_error)?;
        Self::configure(&conn)?;
        let application_id: i64 = conn
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(db_error)?;
        let current: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(db_error)?;
        if application_id == 0 {
            let objects: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            if objects != 0 || current != 0 {
                return Err(Error::Storage(
                    "database is not empty or owned by AWR".into(),
                ));
            }
        } else if application_id != APPLICATION_ID {
            return Err(Error::Storage(
                "database belongs to a different application".into(),
            ));
        }
        if current > SCHEMA_VERSION {
            return Err(Error::Storage(format!(
                "schema {current} is newer than supported {SCHEMA_VERSION}"
            )));
        }
        let mode: String = conn
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .map_err(db_error)?;
        if mode != "wal" {
            let actual: String = conn
                .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
                .map_err(db_error)?;
            if actual != "wal" {
                return Err(Error::Storage(format!(
                    "WAL could not be enabled: {actual}"
                )));
            }
        }
        if current < SCHEMA_VERSION {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_error)?;
            let version: i64 = tx
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .map_err(db_error)?;
            if version == 0 {
                tx.execute_batch(CATALOG_SQL).map_err(db_error)?;
                tx.execute(
                    "INSERT INTO schema_migrations(version,name,applied_at) VALUES(1,'catalog',?1)",
                    [now_millis()?],
                )
                .map_err(db_error)?;
                tx.pragma_update(None, "application_id", APPLICATION_ID)
                    .map_err(db_error)?;
            }
            if version < 2 {
                tx.execute_batch(DOMAIN_SQL).map_err(db_error)?;
                tx.execute(
                    "INSERT INTO schema_migrations(version,name,applied_at) VALUES(2,'domain',?1)",
                    [now_millis()?],
                )
                .map_err(db_error)?;
            }
            if version < 3 {
                tx.execute_batch(SEARCH_SQL).map_err(db_error)?;
                tx.execute(
                    "INSERT INTO schema_migrations(version,name,applied_at) VALUES(3,'search',?1)",
                    [now_millis()?],
                )
                .map_err(db_error)?;
            }
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(db_error)?;
            tx.commit().map_err(db_error)?;
        }
        let store = Self { conn };
        store.verify_catalog()?;
        Ok(store)
    }

    fn configure(conn: &Connection) -> Result<()> {
        conn.busy_timeout(Duration::from_millis(5000))
            .map_err(db_error)?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(db_error)
    }

    fn verify_catalog(&self) -> Result<()> {
        let versions: Vec<i64> = self
            .conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .map_err(db_error)?
            .query_map([], |r| r.get(0))
            .map_err(db_error)?
            .collect::<rusqlite::Result<_>>()
            .map_err(db_error)?;
        if versions != (1..=SCHEMA_VERSION).collect::<Vec<_>>() {
            return Err(Error::Storage(format!(
                "migration catalog does not match schema: {versions:?}"
            )));
        }
        Ok(())
    }

    /// Inspect without creating, upgrading, or repairing the database.
    pub fn inspect(path: &Path) -> Result<DoctorReport> {
        if !path.is_file() {
            return Err(Error::NotFound(format!("AWR database: {}", path.display())));
        }
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(db_error)?;
        Self::configure(&conn)?;
        Self { conn }.doctor()
    }

    pub fn doctor(&self) -> Result<DoctorReport> {
        let application_id: i64 = self
            .conn
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(db_error)?;
        if application_id != APPLICATION_ID {
            return Err(Error::Storage("not an AWR database".into()));
        }
        let schema_version = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(db_error)?;
        let integrity = self
            .conn
            .prepare("PRAGMA integrity_check")
            .map_err(db_error)?
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?;
        let foreign_key_violations = self
            .conn
            .prepare("PRAGMA foreign_key_check")
            .map_err(db_error)?
            .query_map([], |_| Ok(()))
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?
            .len();
        let migrations = self
            .conn
            .prepare("SELECT version,name,applied_at FROM schema_migrations ORDER BY version")
            .map_err(db_error)?
            .query_map([], |r| {
                Ok(MigrationInfo {
                    version: r.get(0)?,
                    name: r.get(1)?,
                    applied_at: r.get(2)?,
                })
            })
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?;
        let catalog_ok = migrations.iter().map(|m| m.version).collect::<Vec<_>>()
            == (1..=SCHEMA_VERSION).collect::<Vec<_>>();
        Ok(DoctorReport {
            ok: integrity == ["ok"]
                && foreign_key_violations == 0
                && schema_version == SCHEMA_VERSION
                && catalog_ok,
            application_id,
            schema_version,
            sqlite_version: self
                .conn
                .query_row("SELECT sqlite_version()", [], |r| r.get(0))
                .map_err(db_error)?,
            journal_mode: self
                .conn
                .pragma_query_value(None, "journal_mode", |r| r.get(0))
                .map_err(db_error)?,
            foreign_keys: self
                .conn
                .pragma_query_value(None, "foreign_keys", |r| r.get(0))
                .map_err(db_error)?,
            busy_timeout_ms: self
                .conn
                .pragma_query_value(None, "busy_timeout", |r| r.get(0))
                .map_err(db_error)?,
            migrations,
            integrity,
            foreign_key_violations,
        })
    }
}
