# Workstream parallel / handoff business acceptance (AWR-WS-051)

Six independent natural business closed-loops (`WS-BIZ-01` … `06`) frozen at
`scope_revision=2`, with fixture packs, an acceptance harness, and honest
`待验` gate tracking.

This work **does not** invent live multi-person / multi-client / Web proof.
Live gates stay `待验` until a real trial team finishes them.

## Hard gates (one per scenario)

| ID | Hard gate | Arc |
| --- | --- | --- |
| WS-BIZ-01 | `three_way_parallel` | Frontend / backend / test parallel delivery |
| WS-BIZ-02 | `cross_line_dependency_rework` | Missing dep → scoped replan → selective adopt |
| WS-BIZ-03 | `cross_person_handoff_recovery` | Mid-work handoff + verify prior execution |
| WS-BIZ-04 | `same_person_agent_switch_subtask_parallel` | Same person, two Agents, parallel children |
| WS-BIZ-05 | `auth_revoke_and_reject` | Reject handoff + revoke auth + reassign |
| WS-BIZ-06 | `time_forecast_stage_acceptance` | Forecast → stage checkpoints → real acceptance |

Fixtures: [`tests/fixtures/workstreams/parallel-handoff-biz/`](../../../tests/fixtures/workstreams/parallel-handoff-biz/).  
Deliverable / evidence root: `.local/awr-workstream-implementation-20260921/business/scope-r2/`.

## Eight gates (per scenario)

1. `natural_client_initiation`
2. `actual_tool_or_role_execution`
3. `real_artifact`
4. `business_followup_1`
5. `business_followup_2`
6. `independent_review`
7. `delivery`
8. `traceable_receipt`

`待验` is **not** pass. Same person/agent signing execution and review is
forbidden. Each scenario must exercise **two named Agent clients**
(`codex_cli`, `claude_code`) **and** real Team Web ops.

## Harness commands

```sh
# Structural verification + refresh honest 待验 reports + materialize deliverable
python3 tests/fixtures/workstreams/parallel-handoff-biz/run_harness.py

# Materialize only
python3 tests/fixtures/workstreams/parallel-handoff-biz/prepare.py

# Unit tests for fixture invariants
python3 -m unittest tests.workstream-parallel-handoff-biz.test_fixtures -v
```

## How a human team finishes remaining gates

1. **Spin trial stack** with Team Web cookie entry
   ([team-web-entry](../integrations/team-web-entry.md) / WS-044) and named
   agent hosts ([named-agent-host](../integrations/named-agent-host.md) /
   WS-024): `codex_cli` and `claude_code`.
2. **Create distinct persons** per scenario fixture `identities.json`
   (never reuse prior business results across scenarios). Bind agents and a
   Web operator; reviewer must not be the executor (for WS-BIZ-04, reviewer
   must not be the same-person second Agent either).
3. **Per scenario**, copy the project fixture from
   `tests/fixtures/workstreams/parallel-handoff-biz/ws-biz-0N/` (or the
   materialized `.local/.../scope-r2/ws-biz-0N/fixture`). Initiate from real
   clients using **only** natural prompts under `prompts/` (no internal IDs).
4. Drive claim → execution → artifacts through named clients; perform the
   Web-marked nodes via normal Team Web UI.
5. Apply each followup after publishing `updates/round-N` materials.
6. Independent reviewer records decision under `deliverables/`.
7. Register PR delivery with **independent commit + remote SHA per scenario**.
8. Only then flip gates from `待验` to `passed` with concrete evidence paths.
   Update `.local/awr-ws-051/completion-evidence.json` accordingly.

## API / DB evidence policy

Read-only for acceptance evidence. Do **not** mutate business state with SQL
to make gates look green. Do **not** auto-pass missing live client gates.

## Ledger shape

Per-scenario `report.json` matches `workstream_parallel_handoff_biz_acceptance`:
scenario id, namespace, hard gate, persons, named clients, required surfaces,
eight gates with status/evidence/blocker, delivery SHA, and overall status
(`blocked_pending_trial_participants` until live completion).
