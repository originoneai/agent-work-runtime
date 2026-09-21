BEGIN;

-- A separate row lets legacy readers share the admission lock without locking
-- ordinary project progress. Enablement takes this row exclusively BEFORE the
-- project lock. A repeatable-read caller with an obsolete snapshot fails with a
-- serialization error instead of entering through the old unscoped API.
CREATE TABLE awr_team.workstream_modes (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (tenant_id, project_id),
    FOREIGN KEY (tenant_id, project_id) REFERENCES awr_team.projects(tenant_id, id)
);
INSERT INTO awr_team.workstream_modes(tenant_id, project_id)
    SELECT tenant_id, id FROM awr_team.projects;
CREATE FUNCTION awr_team.initialize_workstream_mode() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO awr_team.workstream_modes(tenant_id, project_id)
        VALUES (NEW.tenant_id, NEW.id);
    RETURN NEW;
END $$;
CREATE TRIGGER initialize_workstream_mode AFTER INSERT ON awr_team.projects
    FOR EACH ROW EXECUTE FUNCTION awr_team.initialize_workstream_mode();

-- This dimension does not rename or reinterpret the existing work scope.
CREATE TABLE awr_team.workstream_catalogs (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    catalog_json JSONB NOT NULL,
    projection_hash TEXT NOT NULL,
    PRIMARY KEY (tenant_id, project_id, snapshot_id),
    FOREIGN KEY (tenant_id, project_id, snapshot_id)
        REFERENCES awr_team.source_snapshots(tenant_id, project_id, id)
);
CREATE TABLE awr_team.workstream_ownership (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    work_id TEXT NOT NULL,
    workstream_id TEXT NOT NULL,
    ownership_version BIGINT NOT NULL DEFAULT 1 CHECK (ownership_version > 0),
    PRIMARY KEY (tenant_id, project_id, work_id),
    FOREIGN KEY (tenant_id, project_id, work_id)
        REFERENCES awr_team.work_items(tenant_id, project_id, id)
);
CREATE TABLE awr_team.workstream_snapshot_ownership (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    scope_id TEXT NOT NULL CHECK (scope_id = 'main'),
    work_id TEXT NOT NULL,
    workstream_id TEXT NOT NULL,
    ownership_version BIGINT NOT NULL CHECK (ownership_version > 0),
    PRIMARY KEY (tenant_id, project_id, snapshot_id, work_id),
    FOREIGN KEY (tenant_id, project_id, snapshot_id)
        REFERENCES awr_team.workstream_catalogs(tenant_id, project_id, snapshot_id),
    FOREIGN KEY (tenant_id, project_id, snapshot_id, scope_id, work_id)
        REFERENCES awr_team.work_contracts(tenant_id, project_id, snapshot_id, scope_id, work_id)
);
CREATE INDEX workstream_snapshot_ownership_scope ON awr_team.workstream_snapshot_ownership
    (tenant_id, project_id, snapshot_id, workstream_id, work_id);

-- Trusted operator policy, independent of source declarations and request data.
CREATE TABLE awr_team.workstream_grants (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    workstream_id TEXT NOT NULL,
    authority_version BIGINT NOT NULL CHECK (authority_version > 0),
    can_read BOOLEAN NOT NULL,
    can_write BOOLEAN NOT NULL DEFAULT FALSE,
    can_manage BOOLEAN NOT NULL DEFAULT FALSE,
    grant_version BIGINT NOT NULL DEFAULT 1 CHECK (grant_version > 0),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    CHECK (can_read OR NOT (can_write OR can_manage)),
    PRIMARY KEY (tenant_id, project_id, actor_id, client_id, workstream_id),
    FOREIGN KEY (tenant_id, project_id, actor_id)
        REFERENCES awr_team.project_memberships(tenant_id, project_id, actor_id)
);

ALTER TABLE awr_team.sessions ADD COLUMN workstream_id TEXT;
ALTER TABLE awr_team.sessions ADD COLUMN ownership_version BIGINT;
ALTER TABLE awr_team.sessions ADD CONSTRAINT sessions_workstream_binding CHECK (
    (workstream_id IS NULL AND ownership_version IS NULL) OR
    (workstream_id IS NOT NULL AND ownership_version IS NOT NULL AND ownership_version > 0));
ALTER TABLE awr_team.events ADD COLUMN workstream_id TEXT;

DO $$
DECLARE t TEXT;
BEGIN
    FOREACH t IN ARRAY ARRAY['workstream_modes', 'workstream_catalogs', 'workstream_ownership',
        'workstream_snapshot_ownership', 'workstream_grants']
    LOOP
        EXECUTE format('ALTER TABLE awr_team.%I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE awr_team.%I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format(
            'CREATE POLICY %I_isolation ON awr_team.%I
             USING (tenant_id = current_setting(''awr.tenant_id'', true)
                AND project_id = current_setting(''awr.project_id'', true))
             WITH CHECK (tenant_id = current_setting(''awr.tenant_id'', true)
                AND project_id = current_setting(''awr.project_id'', true))', t, t);
    END LOOP;
END $$;
UPDATE awr_team.schema_state SET version=10 WHERE component='awr_team';
COMMIT;
