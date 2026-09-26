BEGIN;

-- Agent review is an explicit grant, separate from independent human review.
ALTER TABLE awr_team.project_memberships
    ADD COLUMN agent_review BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE awr_team.review_rounds ADD COLUMN author_client_id TEXT;
ALTER TABLE awr_team.review_decisions
    ADD COLUMN reviewer_client_id TEXT,
    ADD COLUMN approval_basis TEXT NOT NULL DEFAULT 'unspecified'
        CHECK (approval_basis IN ('human_independent_review',
            'human_author_self_review', 'agent_review', 'unspecified'));

-- Historical client attribution is unknown. Do not infer it from live clients.
UPDATE awr_team.review_decisions SET approval_basis = CASE independence_kind
    WHEN 'team_independent' THEN 'human_independent_review'
    WHEN 'personal_self_review' THEN 'human_author_self_review'
    ELSE 'unspecified' END;
ALTER TABLE awr_team.review_decisions
    DROP CONSTRAINT review_decisions_independence_kind_check,
    ADD CONSTRAINT review_decisions_independence_kind_check
        CHECK (independence_kind IN ('team_independent',
            'personal_self_review', 'agent_review', 'unspecified')),
    ADD CONSTRAINT review_decisions_agent_attribution_check
        CHECK ((approval_basis = 'agent_review') = (independence_kind = 'agent_review')
            AND (approval_basis <> 'agent_review' OR reviewer_client_id IS NOT NULL));

UPDATE awr_team.schema_state SET version=35 WHERE component='awr_team';
COMMIT;
