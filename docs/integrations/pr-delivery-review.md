# PR delivery ∩ authorized review & completion (AWR-TMCP-031)

## Purpose

Reuse the WS-018 evidence / review / rework / complete domain flow on the same
Team MCP management plane, with **independent** `review.decide` and
`delivery.finalize` permissions. Bind deliveries to exact repository, task
contract, PR, head SHA, test evidence, and merge SHA when applicable. Keep
GitHub PR facts visually and mechanically separate from AWR acceptance.

## Permissions

| Business action | Command ops | Who |
| --- | --- | --- |
| `delivery.submit_and_request_review` | `evidence.submit`, `review.open`, `delivery.submit_and_request_review`, `delivery.register_pr`, `delivery.observe_pr`, `work.rework` | developer / maintainer / project_admin (template). `work.rework` is author acknowledgment of a return, not independent review. |
| `review.decide` | `review.accept`, `review.return`, `review.decide` | **explicit** `independent_review` membership grant on an eligible template (developer / maintainer / project_admin). Never implied by role name, admin, or agent delegation. |
| `delivery.finalize` | `work.complete`, `delivery.finalize` | maintainer / project_admin |

Legacy membership label `reviewer` maps to the reader template and **cannot**
receive `independent_review`. Grant the developer (or higher) template plus the
flag via project-admin access apply (`independent_review: true`).

## PR version evidence (v1)

`delivery.register_pr` records:

- `repository`, `pr_number`, `pr_url`, `head_sha` (40-char lowercase hex)
- optional `merge_sha`, `test_evidence_id`
- attribution: `author_actor_id`, `owner_person_id`, `executor_actor_id`
- `fact_source`: `authorized_human_github_verification` or `operator_recorded_observation`
- `observed_at`: RFC3339 timestamp of the observation

This is **not** webhook auto-sync. A URL alone is insufficient; green CI, admin
role, or `gh_merged=true` never complete the work.

`delivery.observe_pr` updates GitHub approved/merged observations while rechecking
`expected_head_sha`. A head or live-contract mismatch **invalidates** the delivery
and open/approved AWR review rounds bound to the old contract.

## Status surfaces

`delivery.inspect` returns separate blocks:

- `github.submitted` / `github.approved` / `github.merged`
- `awr_acceptance.complete` / runtime state / selected completion id
- `cannot_skip_acceptance_via`: `pr_url_alone`, `green_ci`, `admin_role`, `already_merged`
- `webhook_auto_sync: false`

## Independence & attribution

Independence follows WS-015/018 person relations: two agents of the same person
are not team-independent. Existing human-review contracts reject Agent approvals.

Completion receipts attribute **author**, **owner**, **executor**, **reviewer**,
and **final submitter** separately (`approved_by_json` plus dedicated columns).
Failed / rejected / invalidated history is retained; only mismatched open rounds
are invalidated on head/contract change.

## Explicit Agent review

For a contract with `completion_policy: caller_managed_execution_and_agent_review`,
use `review.decide` with an authenticated Agent identity. An administrator must
explicitly grant `agent_review: true`; an active WS-016 `Review` delegation must
also cover that Agent, client, session and work. Role names alone grant nothing.
Model names remain descriptive metadata and do not establish identity or authority.

The reviewer must differ from the round author in both actor and client, and
cannot be the evidence creator or execution actor. Two Agents may have the same
responsible person. Decisions persist `approval_basis: agent_review`, actual
actor/client attribution, `human_approval: false`, and
`team_independent_acceptance: false`. `review.inspect` exposes the same facts.

`review.accept` and `review.return` remain human-review aliases. The legacy
ReviewStore does not support Agent review. This capability currently records
review decisions only: the new policy cannot finalize work or unlock dependencies.
Existing human policies are unchanged and cannot be downgraded through planning.

## Recheck boundaries

Admission and effect phases re-run TMCP action auth. Completion still requires
WS-018 evidence gates. An active PR delivery must match the live contract hash;
GitHub merge is recorded but never substitutes for AWR acceptance.
