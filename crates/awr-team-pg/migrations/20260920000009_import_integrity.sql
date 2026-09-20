BEGIN;
-- Legacy NULL-project jobs remain quarantined by RLS. New NULL jobs are refused.
ALTER TABLE awr_team.import_jobs DROP CONSTRAINT import_jobs_tenant_id_import_key_manifest_hash_key;
ALTER TABLE awr_team.import_jobs ADD CONSTRAINT import_jobs_project_identity UNIQUE (tenant_id, project_id, import_key, manifest_hash);
ALTER TABLE awr_team.import_jobs ADD CONSTRAINT import_jobs_project_fk FOREIGN KEY (tenant_id, project_id) REFERENCES awr_team.projects(tenant_id,id);
ALTER TABLE awr_team.import_jobs ADD CONSTRAINT import_jobs_project_required CHECK (project_id IS NOT NULL) NOT VALID;
ALTER TABLE awr_team.import_jobs ADD COLUMN manifest_json JSONB, ADD COLUMN snapshot_id TEXT;
ALTER TABLE awr_team.import_jobs ADD CONSTRAINT import_jobs_snapshot_fk FOREIGN KEY (tenant_id,project_id,snapshot_id) REFERENCES awr_team.source_snapshots(tenant_id,project_id,id);
DROP POLICY import_jobs_isolation ON awr_team.import_jobs;
CREATE POLICY import_jobs_isolation ON awr_team.import_jobs
 USING (tenant_id=current_setting('awr.tenant_id',true) AND project_id=current_setting('awr.project_id',true))
 WITH CHECK (tenant_id=current_setting('awr.tenant_id',true) AND project_id=current_setting('awr.project_id',true));
-- Legacy executions carry no verified epoch and cannot acquire new effect admission.
ALTER TABLE awr_team.executions ADD COLUMN coordinator_epoch TEXT;
ALTER TABLE awr_team.backups ADD COLUMN manifest_json JSONB;
UPDATE awr_team.schema_state SET version=9 WHERE component='awr_team';
COMMIT;
