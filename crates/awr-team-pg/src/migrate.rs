use crate::error::{PgError, PgResult};
use tokio_postgres::Client;

pub const EXPECTED_SCHEMA_VERSION: i32 = 10;
const MIGRATIONS: &[(&str, i32)] = &[
    (include_str!("../migrations/20260917000001_init.sql"), 1),
    (
        include_str!("../migrations/20260918000002_session_wait.sql"),
        2,
    ),
    (
        include_str!("../migrations/20260918000003_graph_resources.sql"),
        3,
    ),
    (
        include_str!("../migrations/20260918000004_execution_protocol.sql"),
        4,
    ),
    (
        include_str!("../migrations/20260918000005_review_completion.sql"),
        5,
    ),
    (
        include_str!("../migrations/20260918000006_import_restore.sql"),
        6,
    ),
    (
        include_str!("../migrations/20260919000007_completion_integrity.sql"),
        7,
    ),
    (
        include_str!("../migrations/20260920000008_execution_result_binding.sql"),
        8,
    ),
    (
        include_str!("../migrations/20260920000009_import_integrity.sql"),
        9,
    ),
    (
        include_str!("../migrations/20260921000010_workstreams.sql"),
        10,
    ),
];

pub async fn migrate(client: &Client) -> PgResult<()> {
    let current = schema_version(client).await?;
    if let Some(version) = current {
        if version > EXPECTED_SCHEMA_VERSION {
            return Err(PgError::SchemaIncompatible(format!(
                "schema version {version}, expected {EXPECTED_SCHEMA_VERSION}"
            )));
        }
        if version == EXPECTED_SCHEMA_VERSION {
            return Ok(());
        }
    }
    for (sql, version) in MIGRATIONS {
        if current.unwrap_or(0) >= *version {
            continue;
        }
        client.batch_execute(sql).await?;
    }
    check_schema(client).await
}

async fn schema_version(client: &Client) -> PgResult<Option<i32>> {
    match client
        .query_opt(
            "SELECT version FROM awr_team.schema_state WHERE component='awr_team'",
            &[],
        )
        .await
    {
        Ok(Some(row)) => Ok(Some(row.get(0))),
        Ok(None) => Ok(None),
        // Only an absent schema/table means "not initialized". Other read
        // errors (for example revoked SELECT) must surface as themselves,
        // not be misreported as a missing database (CR #36 P2-1).
        Err(error) => {
            if error
                .as_db_error()
                .map(|db| *db.code() == tokio_postgres::error::SqlState::UNDEFINED_TABLE)
                .unwrap_or(false)
            {
                Ok(None)
            } else {
                Err(PgError::Db(error))
            }
        }
    }
}

pub async fn check_schema(client: &Client) -> PgResult<()> {
    match schema_version(client).await? {
        Some(version) if version == EXPECTED_SCHEMA_VERSION => Ok(()),
        Some(version) => Err(PgError::SchemaIncompatible(format!(
            "schema version {version}, expected {EXPECTED_SCHEMA_VERSION}"
        ))),
        None => Err(PgError::SchemaIncompatible(
            "schema_state missing; refuse to treat an empty or half-migrated database as ready"
                .into(),
        )),
    }
}
