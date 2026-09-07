use crate::{
    Store,
    catalog::{SOURCE_COLUMNS, revision_at, source_row},
    db_error,
    projection::table,
};
use awr_core::{
    EntityKind, Error, Goal, Id, Plan, Projected, Result, Rule, RuleContext, RuleMatch,
};
use rusqlite::{Connection, params};
use serde::de::DeserializeOwned;

pub(crate) fn projections<T: DeserializeOwned>(
    conn: &Connection,
    project: Id,
    kind: EntityKind,
    key: Option<&str>,
) -> Result<Vec<Projected<T>>> {
    let source_columns = SOURCE_COLUMNS
        .split(',')
        .map(|c| format!("s.{c}"))
        .collect::<Vec<_>>()
        .join(",");
    // Payload, source and project revision are selected together, including for multi-row reads.
    let sql = format!(
        "SELECT {source_columns},e.payload_json,p.project_revision FROM {} e
        JOIN sources s ON e.source_id=s.id AND e.project_id=s.project_id
        JOIN projects p ON e.project_id=p.id
        WHERE e.project_id=?1 AND e.active=1 AND s.active=1 AND (?2 IS NULL OR e.external_key=?2)
        ORDER BY e.external_key",
        table(kind)
    );
    conn.prepare(&sql)
        .map_err(db_error)?
        .query_map(params![project.to_string(), key], |row| {
            let item = serde_json::from_str(&row.get::<_, String>(11)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(Projected {
                item,
                source: source_row(row)?,
                project_revision: revision_at(row, 12)?,
            })
        })
        .map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_error)
}

pub(crate) fn projection<T: DeserializeOwned>(
    conn: &Connection,
    project: Id,
    kind: EntityKind,
    key: &str,
) -> Result<Projected<T>> {
    projections(conn, project, kind, Some(key))?
        .into_iter()
        .next()
        .ok_or_else(|| Error::NotFound(format!("{kind:?} {key}")))
}

impl Store {
    pub fn goal(&self, project: Id, key: &str) -> Result<Projected<Goal>> {
        projection(&self.conn, project, EntityKind::Goal, key)
    }
    pub fn plan(&self, project: Id, key: &str) -> Result<Projected<Plan>> {
        projection(&self.conn, project, EntityKind::Plan, key)
    }
    pub fn rule(&self, project: Id, key: &str) -> Result<Projected<Rule>> {
        projection(&self.conn, project, EntityKind::Rule, key)
    }
    pub fn goals(&self, project: Id) -> Result<Vec<Projected<Goal>>> {
        self.project(project)?;
        projections(&self.conn, project, EntityKind::Goal, None)
    }
    pub fn plans(&self, project: Id) -> Result<Vec<Projected<Plan>>> {
        self.project(project)?;
        projections(&self.conn, project, EntityKind::Plan, None)
    }
    pub fn rules(&self, project: Id) -> Result<Vec<Projected<Rule>>> {
        self.project(project)?;
        projections(&self.conn, project, EntityKind::Rule, None)
    }
    /// No severity or applicability filtering here: unknown and hard rules remain visible.
    pub fn rules_for(&self, project: Id, context: &RuleContext) -> Result<Vec<RuleMatch>> {
        Ok(self
            .rules(project)?
            .into_iter()
            .map(|rule| rule.evaluate(context))
            .collect())
    }
}
