-- Agent-reviewed completion stays distinct from human approval and trusted execution.
ALTER TABLE awr_team.completion_receipts
    DROP CONSTRAINT completion_receipts_independence_kind_check,
    ADD CONSTRAINT completion_receipts_independence_kind_check
        CHECK (independence_kind IS NULL OR independence_kind IN
            ('team_independent','personal_self_review','ordinary_confirm','unspecified','agent_review')),
    ADD CONSTRAINT completion_receipts_agent_basis_check CHECK (
        independence_kind IS DISTINCT FROM 'agent_review' OR (
            policy='caller_managed_execution_and_agent_review'
            AND approved_by_json->>'approval_basis'='agent_review'
            AND approved_by_json->>'execution_basis'='caller_asserted_reconciled'
            AND approved_by_json->>'human_approval'='false'
            AND approved_by_json->>'team_independent_acceptance'='false'
        ) IS TRUE
    );
UPDATE awr_team.schema_state SET version = 36 WHERE component = 'awr_team';
