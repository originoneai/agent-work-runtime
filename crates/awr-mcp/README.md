# AWR MCP

One `awr-mcp` process can serve multiple initialized projects and clients through
a shared Streamable HTTP endpoint. Local single-project stdio remains available.
Build with `cargo build --locked -p awr-mcp`; Cargo.lock pins the official `rmcp`
SDK. Diagnostics use stderr; stdio stdout contains protocol messages only.

The shared service and lifecycle extensions below are included in **AWR 0.3.3**.
See the [shared service guide](../../docs/reference/mcp-service.md)
for the project registry, client access, persistent conversations, wait/reply flow
and request outcome recovery. Clients share one URL and select an authorized
project on each call; projects retain separate sources and runtime databases.

A generic configuration for local stdio is:

```json
{
  "mcpServers": {
    "awr": {
      "command": "/absolute/path/to/agent-work-runtime/target/debug/awr-mcp",
      "args": ["--project", "/absolute/path/to/initialized-project"]
    }
  }
}
```

Initialize and index projects through `awr init` and `awr source reindex` first.
Starting the server does not create or migrate a database. Stdio stays bound to its
startup project; HTTP accepts registered project keys, never arbitrary paths.
No shell or report command is executed by a tool.

## Work and context tools

These eight original tools retain their domain contracts. AWR 0.3.3 also
exposes twelve lifecycle/continuity tools below; HTTP adds `awr_projects_list`.

| Tool | Arguments and behavior |
| --- | --- |
| `awr_project_status` | Optional `branch`, `source_sha`; `view: summary` accepts `work` (array), `goal` or `milestone` scope. Compact progress and next suggestion. |
| `awr_work_ready` | Optional `branch`, `limit` (1–100, default 10); dependency readiness and bounded exclusion diagnostics. |
| `awr_work_get` | Required `work`; optional `branch`, `source_sha`; exact acceptance, dependencies and decision/evidence summaries. |
| `awr_context_compile` | Optional `work`, `session`, `agent`, `detached`, `branch`, `goals`, `paths`, `tags`, `source_sha`, `intent`, `budget`, `checkpoint` or `after_revision`; existing L1 context, hash, completeness and omission metadata. |
| `awr_work_transition` | Required `work`, `session`, `action`, `reason`, `expected_revision`; source actions `progress`, `block`, `unblock`, `cancel`, `reopen`, `complete`. Action fields follow the table below. |
| `awr_event_append` | Required `event_type`, `summary`, `expected_revision`; optional `work`, `session`, `branch`, `importance`, `payload`; append a generic event with the existing domain validation. Lifecycle events remain reserved. |
| `awr_evidence_record` | Required `external_key`, `evidence_type`, `level`, `summary`, `locator`, nonempty `scope`, `expected_revision`; optional `work`, `sha256`, `source_sha`, `command`, `branch`, `verified_at`; record caller-supplied evidence bindings. |
| `awr_search` | Optional `text`, `kind`, `status`, `work`, `limit` (1–100, default 10); existing FTS ranking/filtering over bounded summaries. Search includes retained history across branches; it does not feed unrelated branch events into context. |

`work` accepts an external key or internal ID where the corresponding domain query accepts it. `branch` accepts a work-branch name/ID; `main` explicitly selects the baseline. Omission/null selects the current work branch. A named context request uses that branch's fork baseline, so do not combine it with `checkpoint` or `after_revision`. Context with no branch uses the CLI compiler's ordinary selection and delta rules. `detached: true` requires explicit work without a session. `paths: null` means unknown; `paths: []` means known empty scope.

Input objects reject unknown fields. Total tool arguments are limited to 1 MiB; a completion mapping has the same 64 KiB cap as the CLI. Full source SHAs and timestamps use the core domain conventions; `verified_at` is Unix milliseconds. Tools return structured JSON and matching serialized text for clients that consume text content. The context token budget covers `work_context.rendered_context`, not the entire MCP envelope.

## Read behavior

The five read tools use the existing Source, Store, Context and Search services against a disposable SQLite memory snapshot. The persistent database is opened read-only. The source indexer and lazy FTS cache may write to that private snapshot; no cache or projection is copied back. If reindexing would change a source, registration or project revision, the read returns `SourceStale` instead of presenting an ephemeral revision as real state. Sources and the live project revision are checked again before the response.

On `SourceStale`, inspect and run `awr source reindex` or explicitly call
`awr_source_reindex`, then read again. On `RevisionConflict`, review the new state
before choosing the next action. The database snapshot is capped at 256 MiB;
source reads retain existing per-source caps. Large-project memory optimization
remains separate work. Runtime-only session and operation inspection does not
require the source files to be available.

The returned metadata records `freshness_basis: source_verified_readonly`, `source_refresh_performed: false` and `read_only: true`. Sessions, claims, events, checkpoints and selection are not changed by these tools. Named branch context retains the existing execution boundary: closed branches cannot be used for a new execution pack, while their historical records remain available through CLI drill-down commands.

## Source actions

Start an actual session/claim through `awr_session_start` or the CLI, then get the
current revision from a read tool. HTTP sessions must be bound to the authenticated
client; local CLI sessions remain usable through stdio. MCP source actions reuse
the same domain services, including source fingerprint checks, claim ownership,
dependency/acceptance gates, mutation proposals and durable event receipts.

| Action | Action-specific fields |
| --- | --- |
| `progress` | Required `next_action`; optional `summary`. Requires an active owned claim. |
| `block` | Required `blocker`; optional `next_action`. Requires an active owned claim. |
| `unblock` | Required `next_action`; clears the blocker and rechecks dependencies. |
| `cancel` | Optional `next_action`; clears blocker and releases this session's claims. |
| `reopen` | Required `next_action`; returns completed/cancelled work to planned with a reason. |
| `complete` | Required `completion`; no progress fields. Requires an active owned claim and verified report bindings. |

Example `awr_work_transition` arguments after reading the current session and revision:

```json
{
  "work": "REPORT-001",
  "session": "<active-session-id>",
  "action": "progress",
  "reason": "The analysis draft is ready for review",
  "next_action": "Review the customer findings",
  "expected_revision": 42
}
```

`complete` uses the same input object as `awr work complete --input`:

```json
{
  "version": 1,
  "source_sha": "<full-source-sha>",
  "acceptance": [
    {"criterion": "<exact source acceptance>", "evidence": ["<registered evidence key>"]}
  ]
}
```

Supply that object as `completion`, with `work`, `session`, `action: complete`, `reason` and the reviewed `expected_revision` alongside it. Optional `minimum_level` can strengthen the baseline of `locally_verified`; optional `required_evidence` adds required references. Every authoritative criterion must map exactly once to nonempty evidence. Registered report bytes, hashes, source SHA, scope, command, checks and verification time are verified by the existing completion service. Registering an evidence record alone does not certify its assertions or complete a task.

Source work actions retain the domain service's fingerprint checks before accepting a proposal; changed source bytes can return `SourceConflict` without refreshing the persistent projection. Event and evidence registration refresh sources before their domain write; a refresh that advances the expected revision is retained but returns a conflict before the requested operation. Inspect, explicitly reindex where needed, and retry only with reviewed state. The five read tools persist nothing. `expected_revision` is mandatory, and failed source writes preserve the domain proposal/compensation report when available.

## Errors and receipts

Domain errors are tool results with `isError: true` and the core AWR `code` and message. Incomplete context retains its L1 diagnostics and `completeness` alongside the error. A source write failure that has already produced a proposal returns that proposal's report, including its recovery outcome. Unknown tools are JSON-RPC protocol errors. Successful tool results have `isError: false`.

An interrupted or cancelled request may have committed. Shared HTTP writes require
stable `request_id` values; stdio accepts them optionally. Inspect the identified
operation before retrying. The request journal consumes revisions too: use returned
revisions, not a calculated increment. See the [recovery contract](../../docs/reference/mcp-service.md#write-identity-and-unknown-outcomes).

## Session and continuity tools

| Tool | Purpose |
| --- | --- |
| `awr_session_start` | Bind a host conversation to a work session and optionally claim work. |
| `awr_session_get` | Read the session, claims, checkpoint, waits and successor. |
| `awr_session_list` | Page through the authenticated client's sessions. |
| `awr_session_checkpoint` | Save consumed context, progress and next action. |
| `awr_session_claim` | Acquire or release the selected session's claim. |
| `awr_session_end` | End or interrupt a session and settle claims. |
| `awr_session_resume` | Explicitly continue a predecessor in a successor session. |
| `awr_session_wait` | Save a checkpoint and durable question before waiting for the user. |
| `awr_session_reply` | Save an answer or cancellation without executing work. |
| `awr_operation_get` | Inspect the authenticated client's saved request outcome. |
| `awr_operation_recover` | Recover a proven committed outcome without replaying its action. |
| `awr_source_reindex` | Refresh configured source projections explicitly. |

The [service guide](../../docs/reference/mcp-service.md) defines arguments, scope,
limits and continuation. Branch creation/selection and other detailed administration
remain CLI operations; adding MCP continuity does not change those domain rules.

The implementation follows the [official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk) and [MCP tools specification](https://modelcontextprotocol.io/specification/2026-07-28/server/tools). Local stdio checks and development self-use are recorded as feature evidence; they do not constitute E4 business acceptance or release approval.
