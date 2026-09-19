-- CR #42 P2-2/P2-7: artifact content is persisted, and a completed work must
-- reference a REAL receipt of the same tenant/project/scope/work.
ALTER TABLE awr_team.artifacts
    ADD COLUMN content BYTEA;

ALTER TABLE awr_team.completion_receipts
    ADD CONSTRAINT completion_receipts_identity_key
    UNIQUE (tenant_id, project_id, scope_id, work_id, id);

ALTER TABLE awr_team.work_runtime
    ADD CONSTRAINT work_runtime_selected_completion_fk
    FOREIGN KEY (tenant_id, project_id, scope_id, work_id, selected_completion_id)
    REFERENCES awr_team.completion_receipts(tenant_id, project_id, scope_id, work_id, id);

UPDATE awr_team.schema_state SET version = 7 WHERE component = 'awr_team';
