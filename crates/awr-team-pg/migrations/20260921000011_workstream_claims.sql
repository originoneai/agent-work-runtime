BEGIN;

-- Historical unscoped claims retain NULL attribution. They are never silently
-- adopted by the authenticated workstream protocol or assigned a current epoch.
ALTER TABLE awr_team.claims ADD COLUMN workstream_id TEXT;
ALTER TABLE awr_team.claims ADD COLUMN ownership_version BIGINT;
ALTER TABLE awr_team.claims ADD COLUMN coordinator_epoch TEXT;
ALTER TABLE awr_team.claims ADD CONSTRAINT claims_workstream_binding CHECK (
    (workstream_id IS NULL AND ownership_version IS NULL AND coordinator_epoch IS NULL) OR
    (workstream_id IS NOT NULL AND workstream_id <> '' AND ownership_version IS NOT NULL
        AND ownership_version > 0 AND coordinator_epoch IS NOT NULL AND coordinator_epoch <> ''));

UPDATE awr_team.schema_state SET version=11 WHERE component='awr_team';
COMMIT;
