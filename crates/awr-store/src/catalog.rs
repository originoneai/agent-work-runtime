use crate::source_changes::{self, SourceState};
use crate::{Store, db_error};
use awr_core::{
    AuthorityMode, Error, Event, EventDraft, Freshness, Id, Project, Result, Revision, Source,
    now_millis,
};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::de::DeserializeOwned;
use std::path::Path;

pub struct SourceRegistration<'a> {
    pub domain: &'a str,
    pub role: &'a str,
    pub locator: &'a str,
    pub format: &'a str,
    pub adapter: &'a str,
}

pub(crate) fn id_at(row: &Row<'_>, index: usize) -> rusqlite::Result<Id> {
    let raw: String = row.get(index)?;
    raw.parse::<Id>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}

pub(crate) fn revision_at(row: &Row<'_>, index: usize) -> rusqlite::Result<Revision> {
    let raw: i64 = row.get(index)?;
    raw.try_into().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(e),
        )
    })
}

fn enum_at<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(row.get(index)?)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn project_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    let branch: Option<String> = row.get(4)?;
    Ok(Project {
        id: id_at(row, 0)?,
        external_key: row.get(1)?,
        name: row.get(2)?,
        root: std::path::PathBuf::from(row.get::<_, String>(3)?),
        current_branch_id: branch.map(|s| s.parse()).transpose().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?,
        project_revision: revision_at(row, 5)?,
        authority_mode: AuthorityMode::SourceFirst,
    })
}

pub(crate) fn source_row(row: &Row<'_>) -> rusqlite::Result<Source> {
    Ok(Source {
        id: id_at(row, 0)?,
        project_id: id_at(row, 1)?,
        domain: row.get(2)?,
        role: row.get(3)?,
        locator: row.get(4)?,
        format: row.get(5)?,
        adapter: row.get(6)?,
        revision: revision_at(row, 7)?,
        fingerprint: row.get(8)?,
        freshness: enum_at(row, 9)?,
        config: serde_json::from_str(&row.get::<_, String>(10)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

const PROJECT_COLUMNS: &str = "id,external_key,name,root,current_branch_id,project_revision";
pub(crate) const SOURCE_COLUMNS: &str =
    "id,project_id,domain,role,locator,format,adapter,revision,fingerprint,freshness,config_json";

pub(crate) fn bump_revision(conn: &Connection, project: Id) -> Result<Revision> {
    conn.query_row("UPDATE projects SET project_revision=project_revision+1 WHERE id=?1 RETURNING project_revision",
        [project.to_string()], |r|revision_at(r,0)).optional().map_err(db_error)?
        .ok_or_else(||Error::NotFound(format!("project {project}")))
}

impl Store {
    /// Retained source identity is available even after removal from the active mapping.
    pub fn retained_source(&self, project: Id, id: Id) -> Result<(Source, bool)> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {SOURCE_COLUMNS},active FROM sources WHERE project_id=?1 AND id=?2"
                ),
                params![project.to_string(), id.to_string()],
                |r| Ok((source_row(r)?, r.get(11)?)),
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| Error::NotFound(format!("source {id}")))
    }
    pub fn register_project(
        &mut self,
        root: &Path,
        external_key: &str,
        name: &str,
    ) -> Result<Project> {
        if external_key.trim().is_empty() || name.trim().is_empty() {
            return Err(Error::InvalidInput(
                "project name and external key must not be empty".into(),
            ));
        }
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(Error::InvalidInput(
                "project root must be a directory".into(),
            ));
        }
        let root_text = root
            .to_str()
            .ok_or_else(|| Error::InvalidInput("project root must be UTF-8".into()))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let existing = tx
            .query_row(
                &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE root=?1"),
                [root_text],
                project_row,
            )
            .optional()
            .map_err(db_error)?;
        let project = if let Some(mut project) = existing {
            if project.external_key != external_key || project.name != name {
                tx.execute(
                    "UPDATE projects SET external_key=?1,name=?2 WHERE id=?3",
                    params![external_key, name, project.id.to_string()],
                )
                .map_err(db_error)?;
                project.project_revision = bump_revision(&tx, project.id)?;
                project.external_key = external_key.into();
                project.name = name.into();
            }
            project
        } else {
            let project = Project {
                id: Id::new(),
                external_key: external_key.into(),
                name: name.into(),
                root,
                authority_mode: AuthorityMode::SourceFirst,
                current_branch_id: None,
                project_revision: 0,
            };
            tx.execute("INSERT INTO projects(id,external_key,name,root,authority_mode) VALUES(?1,?2,?3,?4,'source_first')",
                params![project.id.to_string(),external_key,name,project.root.to_str()]).map_err(db_error)?;
            project
        };
        tx.commit().map_err(db_error)?;
        Ok(project)
    }

    pub fn project(&self, id: Id) -> Result<Project> {
        self.conn
            .query_row(
                &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id=?1"),
                [id.to_string()],
                project_row,
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| Error::NotFound(format!("project {id}")))
    }

    pub fn project_by_root(&self, root: &Path) -> Result<Project> {
        let root = root.canonicalize()?;
        self.conn
            .query_row(
                &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE root=?1"),
                [root
                    .to_str()
                    .ok_or_else(|| Error::InvalidInput("project root must be UTF-8".into()))?],
                project_row,
            )
            .optional()
            .map_err(db_error)?
            .ok_or_else(|| Error::NotFound(format!("project root {}", root.display())))
    }

    pub fn register_source(
        &mut self,
        project_id: Id,
        definition: &SourceRegistration<'_>,
    ) -> Result<Source> {
        if !["primary", "supporting"].contains(&definition.role)
            || definition.domain.is_empty()
            || definition.locator.is_empty()
            || definition.adapter.is_empty()
        {
            return Err(Error::InvalidInput(
                "source requires domain, primary/supporting role, locator and adapter".into(),
            ));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let existing=tx.query_row(&format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE project_id=?1 AND domain=?2 AND locator=?3"),
            params![project_id.to_string(),definition.domain,definition.locator],source_row).optional().map_err(db_error)?;
        let mut change = None;
        let source = if let Some(mut source) = existing {
            let active: bool = tx
                .query_row(
                    "SELECT active FROM sources WHERE id=?1",
                    [source.id.to_string()],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            if source.role != definition.role
                || source.format != definition.format
                || source.adapter != definition.adapter
                || !active
            {
                let before = SourceState::from_source(&source, active);
                tx.execute("UPDATE sources SET role=?1,format=?2,adapter=?3,active=1,revision=revision+1,freshness='stale' WHERE id=?4",
                    params![definition.role,definition.format,definition.adapter,source.id.to_string()]).map_err(db_error)?;
                change = Some((
                    Some(before),
                    "source.registered",
                    bump_revision(&tx, project_id)?,
                ));
                source.role = definition.role.into();
                source.format = definition.format.into();
                source.adapter = definition.adapter.into();
                source.revision += 1;
                source.freshness = Freshness::Stale;
            }
            source
        } else {
            let source = Source {
                id: Id::new(),
                project_id,
                domain: definition.domain.into(),
                role: definition.role.into(),
                locator: definition.locator.into(),
                format: definition.format.into(),
                adapter: definition.adapter.into(),
                revision: 0,
                fingerprint: String::new(),
                freshness: Freshness::Stale,
                config: serde_json::json!({}),
            };
            tx.execute("INSERT INTO sources(id,project_id,domain,role,locator,format,adapter,freshness) VALUES(?1,?2,?3,?4,?5,?6,?7,'stale')",
                params![source.id.to_string(),project_id.to_string(),definition.domain,definition.role,definition.locator,definition.format,definition.adapter]).map_err(db_error)?;
            change = Some((None, "source.registered", bump_revision(&tx, project_id)?));
            source
        };
        if let Some((before, kind, revision)) = change {
            let mut draft = EventDraft::new(kind, "Source authority registration changed");
            source_changes::annotate(
                &mut draft,
                before,
                SourceState::from_source(&source, true),
                vec![],
            )?;
            crate::transaction::insert_event(
                &tx,
                &Event {
                    id: Id::new(),
                    project_id,
                    work_item_id: None,
                    session_id: None,
                    branch_id: None,
                    event_type: draft.event_type,
                    importance: draft.importance,
                    summary: draft.summary,
                    payload: draft.payload,
                    project_revision: revision,
                    created_at: now_millis()?,
                },
            )?;
        }
        tx.commit().map_err(db_error)?;
        Ok(source)
    }

    pub fn sources(&self, project_id: Id) -> Result<Vec<Source>> {
        self.conn.prepare(&format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE project_id=?1 AND active=1 ORDER BY domain,locator,id"))
            .map_err(db_error)?.query_map([project_id.to_string()],source_row).map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
    }

    pub fn source(&self, project_id: Id, source_id: Id) -> Result<Source> {
        self.conn.query_row(&format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE project_id=?1 AND id=?2 AND active=1"),
            params![project_id.to_string(),source_id.to_string()],source_row).optional().map_err(db_error)?
            .ok_or_else(||Error::NotFound(format!("source {source_id}")))
    }
}
