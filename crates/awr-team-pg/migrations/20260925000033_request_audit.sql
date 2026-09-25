BEGIN;

-- Authenticated transport metadata only; never bearer values or request bodies.
CREATE TABLE awr_team.request_audit (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    credential_id TEXT NOT NULL,
    action TEXT NOT NULL,
    work_id TEXT,
    result TEXT NOT NULL DEFAULT 'unknown' CHECK (result IN ('unknown','succeeded','denied','failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    finished_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, project_id, id),
    FOREIGN KEY (tenant_id, project_id) REFERENCES awr_team.projects(tenant_id, id)
);
CREATE INDEX request_audit_page ON awr_team.request_audit(tenant_id,project_id,created_at DESC,id DESC);
CREATE INDEX request_audit_actor ON awr_team.request_audit(tenant_id,project_id,actor_id,created_at DESC,id DESC);
ALTER TABLE awr_team.request_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE awr_team.request_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY request_audit_isolation ON awr_team.request_audit
    USING (tenant_id=current_setting('awr.tenant_id',true) AND project_id=current_setting('awr.project_id',true))
    WITH CHECK (tenant_id=current_setting('awr.tenant_id',true) AND project_id=current_setting('awr.project_id',true));
UPDATE awr_team.schema_state SET version=33 WHERE component='awr_team';
COMMIT;
