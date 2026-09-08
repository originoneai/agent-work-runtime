//! Validate the owned schema against the shipped migrations without repairing it.
use crate::{CATALOG_SQL, DOMAIN_SQL, MigrationInfo, SCHEMA_VERSION, SEARCH_SQL, db_error};
use awr_core::{Error, Result};
use rusqlite::Connection;
use std::{collections::BTreeMap, sync::OnceLock};

type Definition = (String, String, String);
type Objects = BTreeMap<String, Definition>;
static EXPECTED: OnceLock<std::result::Result<Vec<Objects>, String>> = OnceLock::new();
const MIGRATION_NAMES: [&str; 3] = ["catalog", "domain", "search"];

fn objects(conn: &Connection) -> rusqlite::Result<Objects> {
    conn.prepare("SELECT name,type,tbl_name,sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |row| Ok((row.get(0)?, (row.get(1)?, row.get(2)?, row.get::<_,String>(3)?.trim().to_owned()))))?
        .collect()
}

fn expected() -> Result<&'static Vec<Objects>> {
    EXPECTED
        .get_or_init(|| {
            let build = || -> rusqlite::Result<Vec<Objects>> {
                let conn = Connection::open_in_memory()?;
                let mut versions = Vec::new();
                for sql in [CATALOG_SQL, DOMAIN_SQL, SEARCH_SQL] {
                    conn.execute_batch(sql)?;
                    versions.push(objects(&conn)?);
                }
                Ok(versions)
            };
            build().map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| Error::Storage(format!("cannot validate shipped schema: {error}")))
}

pub(crate) fn issues(conn: &Connection, version: i64) -> Result<Vec<String>> {
    if !(1..=SCHEMA_VERSION).contains(&version) {
        return Ok(vec![format!("unsupported schema version {version}")]);
    }
    let observed = objects(conn).map_err(db_error)?;
    let expected = &expected()?[(version - 1) as usize];
    let mut issues = Vec::new();
    for (name, definition) in expected {
        match observed.get(name) {
            None => issues.push(format!("missing {} {name}", definition.0)),
            Some(actual) if actual != definition => {
                issues.push(format!("definition mismatch for {} {name}", definition.0));
            }
            _ => {}
        }
    }
    // Extra objects are allowed, e.g. diagnostic indexes. They cannot substitute
    // for or redefine an owned object. Never emit SQL definitions or row values.
    Ok(issues)
}

pub(crate) fn migrations(conn: &Connection) -> Result<Vec<MigrationInfo>> {
    conn.prepare("SELECT version,name,applied_at FROM schema_migrations ORDER BY version")
        .map_err(db_error)?
        .query_map([], |row| {
            Ok(MigrationInfo {
                version: row.get(0)?,
                name: row.get(1)?,
                applied_at: row.get(2)?,
            })
        })
        .map_err(db_error)?
        .collect::<rusqlite::Result<_>>()
        .map_err(db_error)
}

pub(crate) fn catalog_matches(migrations: &[MigrationInfo], version: i64) -> bool {
    (1..=SCHEMA_VERSION).contains(&version)
        && migrations.len() == version as usize
        && migrations.iter().enumerate().all(|(index, entry)| {
            entry.version == index as i64 + 1 && entry.name == MIGRATION_NAMES[index]
        })
}

pub(crate) fn verify(conn: &Connection, version: i64) -> Result<()> {
    if !catalog_matches(&migrations(conn)?, version) {
        return Err(Error::Storage(
            "migration catalog does not match the supported schema".into(),
        ));
    }
    let issues = issues(conn, version)?;
    if !issues.is_empty() {
        return Err(Error::Storage(format!(
            "AWR schema mismatch: {}",
            issues.join("; ")
        )));
    }
    Ok(())
}
