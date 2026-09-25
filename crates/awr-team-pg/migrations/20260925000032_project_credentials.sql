-- Newly issued member credentials can be restricted to one project. Existing
-- operator credentials retain their original grant-based scope.
ALTER TABLE awr_team.credentials ADD COLUMN project_id text;
ALTER TABLE awr_team.credentials ADD CONSTRAINT credentials_project_fk
    FOREIGN KEY (tenant_id, project_id) REFERENCES awr_team.projects(tenant_id, id);
CREATE INDEX credentials_project ON awr_team.credentials(tenant_id, project_id, actor_id);
UPDATE awr_team.schema_state SET version = 32 WHERE component = 'awr_team';
