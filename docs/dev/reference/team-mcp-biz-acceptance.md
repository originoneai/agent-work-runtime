# Team MCP business acceptance (AWR-TMCP-051)

Four multi-person remote Team MCP delivery scenarios (`TMCP-BIZ-01` … `04`) with
fixture packs, an acceptance harness, and honest `待验` gate tracking.

This work **does not** invent independent human review. Live gates stay `待验`
until a real trial team finishes them.

## Scenarios

| ID | Business arc | Named clients | Persons |
| --- | --- | --- | --- |
| TMCP-BIZ-01 | Role-based query/filter; UX adjust; trial-boundary fix | `codex_cli`, `claude_code` | 2 developers + independent reviewer |
| TMCP-BIZ-02 | Parallel work finds missing dep; approved replan; conventions; post-integration adjust | same | same |
| TMCP-BIZ-03 | Member handoff + retire old client; unknown-field site; review rework | same | same |
| TMCP-BIZ-04 | PR rework + service reconnect; root-cause fix; compatibility revise | same | same |

Fixtures: [`tests/fixtures/team-mcp/biz-acceptance/`](../../../tests/fixtures/team-mcp/biz-acceptance/).  
Evidence root: `.local/awr-team-mcp-acceptance-v1/tmcp-biz-0N/report.json`.

## Eight gates (per scenario)

1. `natural_client_initiation`
2. `actual_tool_or_role_execution`
3. `real_artifact`
4. `business_followup_1`
5. `business_followup_2`
6. `independent_review`
7. `delivery`
8. `traceable_receipt`

`待验` is **not** pass. Same person/agent signing execution and review is forbidden.

## Harness commands

```sh
# Structural verification + refresh honest 待验 reports
python3 tests/fixtures/team-mcp/biz-acceptance/verify.py --write-reports

# Same + optional loopback PG read-only probe (uses AWR_TEAM_TEST_DATABASE_URL)
set -a; source /path/to/awr-pg.env; set +a
python3 tests/fixtures/team-mcp/biz-acceptance/run_harness.py

# Unit tests for fixture invariants
python3 -m unittest tests.team-mcp-biz-acceptance.test_fixtures -v
```

## How a human team finishes remaining gates

1. **Spin trial stack** from the TMCP-041 deploy pack
   ([team-deploy-pack.md](team-deploy-pack.md), [team-member-handoff.md](team-member-handoff.md); clients: [team-mcp-codex-cli.md](../integrations/team-mcp-codex-cli.md), [team-mcp-claude-code.md](../integrations/team-mcp-claude-code.md)): exclusive PG + `awr-server` MCP
   with verified boundary. App role only for serve; owner URL stays ops-only.
2. **Create three persons** (or reuse with distinct credentials): two developers
   and one reviewer. Bind WS-024 adapters `codex_cli` and `claude_code` to the
   two developers (member handoff: MCP URL + personal bearer only — no DB URL).
3. **Per scenario**, copy the project fixture to a trial workspace. Initiate from
   member clients using **only** the natural prompts under `prompts/` (no
   internal IDs). Drive claim → execution → artifacts through remote MCP.
4. Apply each followup prompt after publishing the corresponding `updates/round-N`
   material into the project (or equivalent business change via MCP — never via
   backend SQL mutation).
5. Independent reviewer person opens review, records decision, and must not be
   the executor. Capture review artifact under `deliverables/`.
6. Register PR delivery, observe head, finalize. Record remote SHA in
   `report.json` `delivery_remote_sha` and set gate `delivery` +
   `traceable_receipt` with evidence paths.
7. Separate commits + verified remote SHA **per scenario** when delivery happens.
   Generic root-cause fixes get their own regression — no scenario-ID special cases.
8. Only then flip gates from `待验` to `passed` with concrete evidence paths.
   Update `.local/awr-tmcp-051/completion-evidence.json` accordingly.

## API / DB evidence policy

Read-only for acceptance evidence. Do **not** mutate business state with SQL to
make gates look green.

## Ledger shape

Per-scenario `report.json` matches `team_mcp_acceptance`: scenario id, namespace,
persons, named clients, eight gates with status/evidence/blocker, delivery SHA,
and overall status (`blocked_pending_trial_participants` until live completion).
