BEGIN;

-- Declared host metadata never changes authenticated identity or authority.
ALTER TABLE awr_team.sessions ADD COLUMN client_info_json JSONB;
ALTER TABLE awr_team.sessions ADD COLUMN client_info_at TIMESTAMPTZ;
-- Optional business summaries are visible under the existing WorkRead boundary.
-- Raw execution receipts retain their separate inspection authorization.
ALTER TABLE awr_team.checkpoints ADD COLUMN progress_json JSONB;
ALTER TABLE awr_team.checkpoints ADD COLUMN usage_json JSONB;
CREATE INDEX checkpoint_feedback_latest ON awr_team.checkpoints
    (tenant_id,project_id,session_id,created_at DESC,id DESC)
    WHERE progress_json IS NOT NULL OR usage_json IS NOT NULL;

UPDATE awr_team.schema_state SET version=34 WHERE component='awr_team';
COMMIT;
