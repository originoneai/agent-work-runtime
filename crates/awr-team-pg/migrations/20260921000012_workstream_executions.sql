BEGIN;

-- Preserve legacy execution identity. Attribution requires explicit migration,
-- never a join against today's mutable source ownership.
ALTER TABLE awr_team.executions ADD COLUMN workstream_id TEXT;
ALTER TABLE awr_team.executions ADD COLUMN ownership_version BIGINT;
ALTER TABLE awr_team.executions ADD COLUMN executor_client_id TEXT;
ALTER TABLE awr_team.executions ADD COLUMN execution_version BIGINT NOT NULL DEFAULT 1
    CHECK (execution_version > 0);
ALTER TABLE awr_team.executions ADD CONSTRAINT executions_workstream_binding CHECK (
    (workstream_id IS NULL AND ownership_version IS NULL AND executor_client_id IS NULL) OR
    (workstream_id IS NOT NULL AND workstream_id <> '' AND ownership_version IS NOT NULL
        AND ownership_version > 0 AND executor_client_id IS NOT NULL AND executor_client_id <> ''
        AND session_id IS NOT NULL AND claim_id IS NOT NULL AND coordinator_epoch IS NOT NULL
        AND coordinator_epoch <> ''));

UPDATE awr_team.schema_state SET version=12 WHERE component='awr_team';
COMMIT;
