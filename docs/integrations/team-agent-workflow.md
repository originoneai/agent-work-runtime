# Work with a team through your Agent

Connect the Agent you already use to the team's remote MCP service. Give it a
task in your normal conversation. The Agent reads shared work, claims an eligible
task and keeps the project updated as it develops locally. Inspector is an
optional view of the same project; signing in or claiming through a website is
not a prerequisite.

This guide applies to the development Team service. It does not describe the
published 0.5.0 personal MCP server.

## Connect once

Get a project MCP URL and an individual credential from the project administrator,
then clone the code repository. Each participant uses their own credential.
Repository permissions remain separate from AWR permissions.

For Codex CLI, load the credential from its private file in the terminal that
will launch Codex, then register the endpoint provided by the administrator:

```sh
export AWR_TEAM_BEARER="$(cat /absolute/path/to/access.token)"
codex mcp add awr_team --url https://team.example/v1/projects/example/mcp --bearer-token-env-var AWR_TEAM_BEARER
cd /absolute/path/to/your/checkout
codex
```

Other MCP clients use Streamable HTTP with the same URL and bearer authentication.
Configure credentials through the client's supported secret or environment
settings. Desktop apps do not automatically inherit a different terminal's
environment. Restart or reconnect the client after configuration changes.

Use the remote project as the shared work authority. A local checkout is for code
and tests; do not initialize a second local task ledger to replace the remote
project. Server database access is unnecessary.

## Give the Agent the outcome

For example:

> Use the connected AWR Team project to implement the issue API. Refresh the
> current tasks and resume my existing work, or claim an eligible task matching
> this request. Read the contract, dependencies and latest checkpoint before
> editing. Keep progress and evidence in AWR, submit the tested change as a PR,
> and request independent review.

The Agent handles coordination in the same conversation used for development.
It should report an actual permission, dependency or ownership conflict when
encountered. The user does not need to copy task IDs from Inspector or manually
maintain progress in a browser.

## Agent workflow

Use the discovered `awr_team_query` and `awr_team_command` tool schemas and the
[Team protocol reference](../reference/team-workstream-service.md) for exact
arguments. Each query/command rechecks the caller's current permissions.

| When | Agent action | Recheck when |
| --- | --- | --- |
| Starting or reconnecting | Query `capabilities`, `workstreams.list` and scoped `work.list` / `work.search`. Match the user's request to authorized work and inspect `work.recovery`. | Identity, project, scope or ownership changes. |
| Preparing a selected task | Consume `work.prepare`: current contract, required specifications, dependencies, recovery state and context hash. Resume only an active session owned by the current actor and client; otherwise use `session.start` when appropriate. | Contract, dependency or source changes; incomplete context. |
| Taking responsibility | Inspect existing claims with `claim.inspect`. Acquire or renew a live claim under the session using fresh preconditions. Another person's live claim must not be replaced. | Conflict, stale version, lease expiry or revocation. |
| Beginning effects | Use `execution.prepare` and a fresh `execution.start` response with `execution_authorized=true` for one execution within the declared scope. The Agent's host runs code and tools locally. A claim alone is not execution admission. | Permission, lease, scope or execution state changes. |
| Making progress or stopping | Save `session.checkpoint` using the consumed context hash, next action and open loops. Inspect and report execution results with `execution.report`; attach version-bound evidence. Release claims and end sessions only after active work and unknown outcomes are settled. | Interruption, handoff, unknown effects or changed context. |
| Delivering | Submit evidence and request review through `delivery.submit_and_request_review` or `review.open`. An authorized independent person reviews; authorized finalization follows acceptance policy. | PR head, contract or artifact changes; returned review. |

Persist a stable request ID and exact command envelope before a write. After a
timeout, inspect the original `command.inspect` result before an exact retry.
Replayed receipts are historical facts, not permission to run effects again.
Unknown execution effects require inspection and authorized reconciliation;
creating a new session does not bypass that requirement.

Reconnecting MCP does not end a durable work session or renew its lease. A second
Agent belonging to the same person is not an independent reviewer. Keep
implementation, verification, GitHub merge and AWR acceptance as separate facts.

## Optional workspace view

Open Inspector to inspect task relationships, ownership, progress and recorded
results. **Connect Agent** provides a connection command and project instruction;
each task also has an optional copyable brief. Copying either text creates no
session or claim. Administrator and review permissions are still enforced by the
central service, regardless of which client is used.
