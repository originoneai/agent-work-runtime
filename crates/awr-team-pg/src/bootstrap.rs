use crate::error::PgResult;
use tokio_postgres::Client;

pub struct Bootstrap;

impl Bootstrap {
    pub async fn grant_app(client: &Client, app_role: &str) -> PgResult<()> {
        let ident = quote_ident(app_role);
        client
            .batch_execute(&format!(
                "GRANT USAGE ON SCHEMA awr_team TO {ident};
                 GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA awr_team TO {ident};
                 REVOKE UPDATE, DELETE ON awr_team.events FROM {ident};
                 REVOKE UPDATE, DELETE ON awr_team.source_snapshots FROM {ident};
                 REVOKE UPDATE, DELETE ON awr_team.execution_receipts FROM {ident};
                 REVOKE UPDATE, DELETE ON awr_team.evidence FROM {ident};
                 REVOKE UPDATE, DELETE ON awr_team.review_decisions FROM {ident};
                 REVOKE UPDATE, DELETE ON awr_team.completion_receipts FROM {ident};
                 REVOKE UPDATE, DELETE ON awr_team.workstream_catalogs FROM {ident};
                 REVOKE UPDATE, DELETE ON awr_team.workstream_snapshot_ownership FROM {ident};
                 REVOKE ALL ON awr_team.schema_state FROM {ident};
                 -- The app role must read the schema version (check_schema at
                 -- the command entry) but must never modify it (CR #36 P2-1).
                 GRANT SELECT ON awr_team.schema_state TO {ident};"
            ))
            .await?;
        Ok(())
    }
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
