BEGIN;

-- Existing grants acquire no attestation/recovery authority on upgrade.
ALTER TABLE awr_team.workstream_grants
    ADD COLUMN can_attest_execution BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN can_reconcile_execution BOOLEAN NOT NULL DEFAULT FALSE;

-- A resource belongs to one actual execution, not merely to today's work owner.
-- Keep historical reservations unbound for explicit recovery; never infer it.
ALTER TABLE awr_team.executions ADD CONSTRAINT executions_resource_identity
    UNIQUE (tenant_id,project_id,work_id,id);
-- Current privilege cannot retroactively upgrade an untrusted admission.
ALTER TABLE awr_team.executions ADD COLUMN attestation_grant_version BIGINT
    CHECK (attestation_grant_version > 0);
ALTER TABLE awr_team.resource_reservations ADD COLUMN execution_id TEXT;
ALTER TABLE awr_team.resource_reservations ADD CONSTRAINT resource_execution_identity
    FOREIGN KEY (tenant_id,project_id,work_id,execution_id)
    REFERENCES awr_team.executions(tenant_id,project_id,work_id,id);
CREATE INDEX resource_reservations_execution
    ON awr_team.resource_reservations(tenant_id,project_id,execution_id,state)
    WHERE execution_id IS NOT NULL;

UPDATE awr_team.schema_state SET version=13 WHERE component='awr_team';
COMMIT;
