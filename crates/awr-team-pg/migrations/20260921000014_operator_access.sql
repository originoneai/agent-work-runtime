BEGIN;

-- Owner-only provisioning history is separate from client command identities.
CREATE TABLE awr_team.access_changes (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    operator_role TEXT NOT NULL,
    result_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, project_id, request_id),
    FOREIGN KEY (tenant_id, project_id) REFERENCES awr_team.projects(tenant_id, id)
);
ALTER TABLE awr_team.access_changes ENABLE ROW LEVEL SECURITY;
ALTER TABLE awr_team.access_changes FORCE ROW LEVEL SECURITY;
CREATE POLICY access_changes_isolation ON awr_team.access_changes
    USING (tenant_id = current_setting('awr.tenant_id', true)
        AND project_id = current_setting('awr.project_id', true))
    WITH CHECK (tenant_id = current_setting('awr.tenant_id', true)
        AND project_id = current_setting('awr.project_id', true));

UPDATE awr_team.schema_state SET version=14 WHERE component='awr_team';
COMMIT;
