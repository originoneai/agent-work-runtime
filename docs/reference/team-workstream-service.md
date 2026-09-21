# Team workstream HTTP and MCP service

The development branch provides authenticated, multi-project HTTP/MCP queries
and durable sessions, coordination claims and caller-managed execution backed by
PostgreSQL. This is not a release
announcement or a complete Team execution service. It does not dispatch
executions, resume agents or adopt cross-workstream deliveries. Use its live
capabilities response to discover available operations.
An operator-local [reference runner](team-reference-runner.md) can consume scoped
admissions for bounded file writes and attest their results.

## Start an operator-bound service

Build `awr-server` from this source branch. Migrate the intended database to
schema 14 explicitly as its owner, and apply application-role grants using the
[PostgreSQL setup](team-postgres.md). `serve` checks the schema without migrating
it. Run the listener using the application connection, not an owner or superuser
connection.

```toml
version = 1
listen = "127.0.0.1:9908"
allowed_hosts = []

[[projects]]
key = "example"
tenant_id = "tenant-a"
project_id = "project-a"
```

```sh
# AWR_TEAM_DATABASE_URL is supplied by the operator's environment.
awr-server serve --config /absolute/path/team-service.toml
```

Each additional `[[projects]]` entry binds another public alias to an exact
tenant/project pair. The process shares a connection pool across aliases. The
request path selects an alias; request JSON cannot supply tenant, project,
actor, client or grants. Possession of an alias grants no access.

The loopback listener accepts its actual host/port and `localhost` at that port.
Non-loopback listeners require explicit `allowed_hosts` entries. Hosts are exact
authorities, including a port when clients send one. This server has no built-in
TLS: remote use requires operator-provided HTTPS termination and a protected
connection to the backend. All browser `Origin` headers are rejected. There is
no browser login, CORS or cookie authentication.

The project must have an explicitly approved and activated
[`workstreams.json` source bundle](workstreams.md#team-source-projection-in-the-development-branch).
Activation grants no permissions. A legacy project returns `Unsupported` after
authentication; old Team operations continue to refuse enabled projects.

## Credential and grant provisioning

Use [`awr-server access`](team-operator-access.md) to generate a local bearer file, inspect
and preview one client's policy, apply it with a state digest, and query an
uncertain result. This schema-owner CLI creates actors, project membership,
credentials and explicit workstream grants for an already enabled project.
It is not a public HTTP/MCP admin endpoint. Do not give agent clients direct
database credentials or use the owner connection for the running service.

A bearer has the form `awr1.<credential-id>.<secret>`. The ID contains 1–128 ASCII
letters, digits, underscores or hyphens; the secret is 32 cryptographically random
bytes encoded as 64 lowercase hexadecimal characters. Generate secrets with an
OS cryptographic generator; test-fixture tokens are never production credentials.
Store only the result of `awr_team_pg::workstream_credential_hash(bearer)` in
`awr_team.credentials.secret_hash`. Its versioned formula is:

```text
"sha256:" + lowercase_hex(SHA256(UTF8("awr-team-credential-v1:" + bearer)))
```

The credential row binds the ID to one tenant, actor and client, with optional
expiry and revocation. Each `workstream_grants` row binds the project, actor,
client and workstream ID to its current `authority_version`. Reading requires
an active `can_read` grant. Project membership, including the admin role, does
not implicitly grant all workstreams. Increment `grant_version` when changing a
grant and `membership_version` when changing membership. Source authority changes
require explicitly reviewed grants for the new version. These version values
are cursor/context identities, not substitutes for checking current permission.

Every request checks tenant/actor status, credential hash/expiry/revocation,
project membership and client-specific grants inside the same repeatable-read
transaction as the query. Shared row locks make a concurrent revocation take
effect before or after that transaction. A request that encounters a conflicting
snapshot may return `Unavailable`; it cannot return data using the stale grant.
Authorization is checked again on the next request. There is no authorization
cache or reusable execution permission in the returned context.

## Query protocol

POST JSON to `/v1/projects/<alias>/query` with an
`Authorization: Bearer <credential>` header. Version and operation are required:

```json
{"protocol_version":1,"op":"capabilities"}
```

The response advertises the following queries and twelve session/claim/execution commands.
The operation list describes implemented protocol, not a grant to invoke it.
Unsupported operations or protocol versions fail explicitly.

| Operation | Selectors and result |
| --- | --- |
| `workstreams.list` | Authorized workstreams only; `limit`, `cursor` |
| `work.list` | Selected workstream's work summaries and count; `limit`, `cursor` |
| `work.search` | Same visibility boundary; required literal substring `search` |
| `work.prepare` | Required `work_id` or `session_id`; optional `max_context_bytes` |
| `events.list` | Selected workstream, optionally narrowed by work/session; metadata only |
| `session.inspect` | Required `session_id`; its current-ownership checkpoint |
| `work.recovery` | Required work/session; up to two current-ownership recovery candidates |
| `command.inspect` | Required work/session and `request_id`; this actor/client's committed receipt or unknown outcome |
| `claim.inspect` | Required work/session and `claim_id`; current lease state, ownership, fence and epoch validity |
| `execution.inspect` | Required work/session and `execution_id`; current intent state/version, cancellation and contract/epoch validity |

Work/session selectors derive the workstream. An explicit `workstream_id` must
agree with them. Without work/session, a unique authorized workstream can be
selected; multiple grants require an explicit choice. Discovery/capabilities do
not take selectors. Unknown JSON fields and fields inappropriate for an operation
are rejected. Hidden and missing work/session IDs both return `Forbidden`.

Filtering precedes search, counts, ordering and pagination. Cursors bind the
credential/membership, selected authority and grant version, source snapshot,
operation and filters. Copying a cursor to another client does not transfer
permission. Event payloads and unattributed project events are withheld; work
events must agree with current ownership. Team ownership movement is not enabled
by this endpoint.

`work.prepare` includes the verified full contract hash and a `visible_contract`.
The latter omits cross-workstream dependency IDs until authorized export is
implemented, sets `dependency_export_unavailable` and marks context incomplete.
Its serialized content is therefore not the preimage of the full contract hash.
Required visible contract text is never shortened to fit a budget. A too-small
budget returns `ContextIncomplete` without a partial context.

The `awr-team-workstream-context-v1` semantic hash binds the selected contract,
visible facts, runtime state, ownership generation, workstream authority, reader,
grant version, coordinator epoch and project status. Unrelated project audit
revisions or source snapshot IDs remain in the envelope, outside that hash.
Absent runtime state is `null`, not an invented completion status. Preparation
always reports `execution_admission: "not_evaluated"`; context completeness and
a matching hash never authorize execution.

Recovery reports each checkpoint's `contract_matches_current` as true, false,
or null when there is no checkpoint. Matching the contract alone does not prove
current dependencies, claims or authority; `automatic_resume` stays false.
Checkpoints from different ownership generations are not recovery candidates.

## MCP clients

The same listener serves Streamable HTTP MCP at
`/v1/projects/<alias>/mcp`. Configure the client's remote MCP connection with
that URL and its bearer credential in the Authorization header. Do not put
credentials in URLs or tool arguments. Each URL selects one registered project;
many clients and project endpoints share the same process and database pool.
No process is started for an individual connection or work session.

The transport uses the repository's RMCP SDK for protocol negotiation,
initialization and tool dispatch. It is stateless, including for older supported
MCP protocol versions: no `Mcp-Session-Id` carries permissions or selects work.
It exposes two tools, with arguments identical to the corresponding HTTP JSON:

- `awr_team_query`: the query operations above. Start with
  `{"protocol_version":1,"op":"capabilities"}`.
- `awr_team_command`: the eight session/claim/intent commands below, with the same request
  identity and preconditions. Tool discovery is not a write grant.

Initialization, discovery and notifications require current project/workstream
read access. Each tool call additionally checks authorization inside the same
transaction as its selected operation; initialization never caches authority.
Tools return structured content and a text fallback. Domain failures set
`isError: true` and use the same sanitized `code` as HTTP. Authentication,
transport limits and malformed MCP messages can fail at the HTTP/protocol layer
before a tool result exists. Legacy personal tools and unimplemented execution
operations cannot bypass this boundary.

Closing or reconnecting an MCP connection does not end a durable work session.
HTTP and MCP share command identities and receipts: a command submitted through
one transport can be inspected or exactly replayed through the other. A timeout,
disconnection or oversized response leaves the command outcome uncertain until
`command.inspect` returns its committed receipt. An absent receipt is still
`unknown`, not proof of non-execution.

## Durable session commands

POST to `/v1/projects/<alias>/command` with the same bearer authentication. The
following operations record a session journal and grant no execution rights:

| Operation | Strict `args` object |
| --- | --- |
| `session.start` | `conversation_id` |
| `session.checkpoint` | `session_id`, `expected_session_version`, `context_hash`, `next_action`, `open_loops` |
| `session.end` | `session_id`, `expected_session_version` |

Every command supplies `protocol_version: 1`, a stable `request_id`, `op`,
`workstream_id`, `work_id`, `coordinator_epoch`, `expected_project_revision`,
`expected_authority_version`, `expected_ownership_version`,
`expected_contract_hash`, and `args`. Read these preconditions from a current
`work.prepare` response; use its `authority_version` and the prepared data's
`ownership_version`. Version fields are canonical nonnegative decimal strings;
authority, ownership and session versions must be positive. Session versions
come from command receipts or `session.inspect`. Unknown envelope and argument
fields are rejected, including caller-provided actor/client/permission fields.

An active tenant/actor, membership other than `reader`, and a current explicit
`can_write` grant are all required. The server fixes session actor/client,
workstream and ownership generation from authenticated state. A grant to read
another session does not permit writing it: only its original actor/client can
save or close it. Starting another active session for the same actor, client,
conversation and work is rejected; resume the existing session or close it first.

Checkpoint saving recomputes the current scoped context hash in the write
transaction and rejects mismatches. The supplied context must have been consumed
by the caller; a matching hash proves correspondence, not that a model read it.
The checkpoint retains the observed revision and contract hash. Session versions
prevent stale updates even if a caller refreshes its project revision. Each
successful command commits state, scoped event, project revision and immutable
outcome receipt together. Failure rolls back all of them.

This command version retains project-wide revision preconditions and project
serialization. Concurrent unrelated commands can still require a refresh;
task-level read sets and independent concurrent writes are a later protocol.
Never remove the revision requirement or automatically resubmit changed intent
to suppress these conflicts.

Paused/archived workstreams permit checkpoint preservation and session closure
with a still-valid write grant; they do not permit new sessions. A frozen,
importing or degraded project refuses new mutations. Closure also refuses active
claims, open waits or nonterminal/unknown execution on the work, and never treats
an ended session as proof that an external effect stopped. Execution admission,
claim transfer and resource release are not performed by these commands.

On timeout or an unconsumed response, query `command.inspect` with the original
`request_id` and work. It returns only this authenticated actor/client's receipt
under the current workstream and ownership boundary. A found receipt preserves
the original committed revision and result. Absence is `unknown`, because an
in-flight request might still commit. Retry only the **same request ID and exact
payload**: replay returns the original receipt without duplicating a session,
checkpoint or event; changing the payload returns `IdempotencyConflict`. Current
authorization and epoch/ownership still apply to replay. Already committed
receipts remain queryable during a freeze; replay performs no new mutation.

## Coordination claims

These commands use the same command envelope, live write authorization,
project revision, transaction and outcome protocol as session commands:

| Operation | Strict `args` object |
| --- | --- |
| `claim.acquire` | `session_id`, `expected_session_version`, `expected_work_version`, `ttl_seconds` |
| `claim.renew` | `session_id`, `expected_session_version`, `claim_id`, `expected_fence`, `expected_lease_version`, `ttl_seconds` |
| `claim.release` | `session_id`, `expected_session_version`, `claim_id`, `expected_fence`, `expected_lease_version` |

TTL is an integer from 1 to 3600 seconds. Version/fence fields are canonical
decimal strings; use work version `"0"` only when preparation reports no runtime
row. Fence and lease versions must be positive. The active session must belong
to the authenticated actor **and client**, selected work, current workstream and
ownership generation. Another client for the same actor cannot renew or release
the lease. Acquire and renew require an active stream and enabled work contract.
Release is also allowed for a paused/archived stream with current write access;
a frozen/importing/degraded project still refuses new mutations.

A claim records coordination ownership, not dependency admission, a resource
reservation or permission to perform effects. Capabilities advertise
`claim_semantics: "coordination_only"`, and receipts retain
`execution_authorized: false`. Acquiring a claim does not make a work item ready
or validate its upstream deliveries. Live competing claims, open waits, completed
work and unresolved executions/recovery block acquisition. Expiry cannot clear
unknown effects. A safely expired claim can be replaced atomically with a new
monotonic fence. An expired lease cannot be renewed.

Release requires the exact owner, epoch, current fence and lease version, and
resolved execution/recovery state. The owner may release an elapsed lease without
renewing it first. Release does not release resources; the receipt explicitly
reports `resource_release_performed: false`. Claimed work becomes `unclaimed`,
without inventing readiness.

Receipts are historical: `lease_state_basis: "at_commit"` describes the original
commit. Replaying an old acquire after expiry or release returns that same receipt
without creating or extending a lease. Use `claim.inspect` for current state:
`lease_live`, `owned_by_client`, `epoch_matches_current`, and `current_fence`.
Even a live lease is not execution permission. Missing/hidden/mismatched claims
return `Forbidden`; inspection never transfers ownership.

Schema 11 preserves legacy claim rows with null workstream/ownership/epoch
attribution. It does not infer ownership from today's source. Unattributed,
reassigned or old-epoch active rows require explicit recovery/migration before
replacement, even when their deadline has elapsed.

## Execution intents and cancellation

Execution intents persist a planned attempt under the same authenticated command
transaction. They are a prerequisite for the remaining execution lifecycle, not
permission to run a command. `execution.prepare` creates no outbox delivery and
returns `dispatched: false`, `admission: "not_evaluated"` and
`execution_authorized: false`. Explicit caller-managed start and observation
reporting, explicitly authorized executor attestations and operator reconciliation
are available. Dispatch is not exposed; capabilities distinguish these operations.

| Operation | Strict `args` object |
| --- | --- |
| `execution.prepare` | `session_id`, `expected_session_version`, `claim_id`, `expected_fence`, `expected_lease_version`, `expected_work_version`, `input_digest`, `declared_scope` |
| `execution.cancel` | `session_id`, `expected_session_version`, `execution_id`, `expected_execution_version` |

Preparation requires the active, exactly owned session and live claim, current
work version, active workstream and enabled contract. It refuses completed work,
open waits, recovery blocks, unfinished attempts and unknown resources. Each
intent captures immutable actor/client, session/claim, workstream ownership,
coordinator epoch, fence, contract and input digest. Another credential for a
different client cannot act as that executor, even when the actor is the same.
Preparation and cancellation advance work versions and atomically persist their
state, scoped event and command receipt.

`input_digest` is 64 lowercase hexadecimal characters identifying caller-held
input; recording it does not independently verify those input bytes.
`declared_scope` contains at most 128 unique, canonical, workspace-relative paths,
each at most 4096 bytes and contained by a contract scope path. Absolute paths,
parent/dot segments, repeated separators, backslashes, drive prefixes and control
characters are rejected. This is **lexical contract validation**, not filesystem
confinement, a resource reservation or a dependency check. The client cannot
self-select a trusted executor identity, receipt kind or stronger fencing class;
intents remain `uncontrolled` without an exactly-once claim.

The original client may cancel an intent after its claim deadline or while its
workstream is paused, provided it still owns the active session and has current
write access. A frozen project continues to reject new mutations. Cancellation
can synchronously mark an unexposed `prepared` intent as `cancelled`. Any outbox
delivery or execution receipt, or a queued/accepted/running/unknown state, instead
leaves the execution state intact and records `cancel_requested: true` with
`stop_confirmed: false`. No cancellation releases resource reservations or clears
a recovery block. A terminal execution cannot be rewritten through cancellation.

An execution receipt's `execution_state_basis: "at_commit"` is historical.
Replaying preparation after cancellation does not resurrect the attempt. Query
`execution.inspect` to distinguish current state, executor ownership, live lease,
contract currency and epoch currency; it never automatically resumes an attempt
or grants execution. Missing, hidden and mismatched execution IDs share the same
`Forbidden` response. Schema 12 leaves legacy execution attribution null rather
than assigning it from today's source. Such history needs explicit migration;
an old epoch can be inspected within its unchanged ownership but cannot be
cancelled under a new epoch without the recovery protocol.

## Caller-managed start and observations

`execution.start` takes `session_id`, `expected_session_version`, `execution_id`,
`expected_execution_version`, `claim_id`, `expected_fence`,
`expected_lease_version`, `expected_work_version` and
`execution_mode: "caller_managed"`. The optional `expected_input_digest` must
match the prepared input when provided. The bundled adapter uses
`reference_write_v1`, which requires that digest and explicit system attestation
authority at admission; other modes are rejected. The same
transaction checks current authority and ownership, the original prepared
contract, a live owned claim, open waits, recovery state and required completion
receipts. It reserves every declared path as a project-scoped lexical prefix and
moves the attempt to `running`. The admission receipt records its dependency
receipts and reservation identities. It creates no outbox and launches no process.
It also records the original input digest, declared scope and a conservative
remaining lease duration in milliseconds for a cooperating local adapter.

Only the **original successful response** has top-level
`execution_authorized: true`, permitting one caller-managed run under that lease.
The stored receipt always has `execution_authorized: false`; its
`admission: "granted_at_commit"` is a historical observation. Exact retries,
`command.inspect` and `execution.inspect` never issue fresh execution permission.
If the first response is lost, inspect and recover the attempt instead of starting
another external effect. A new request ID cannot restart an already running attempt.

A same-workstream required predecessor needs a selected completion receipt for
its current contract and completed runtime. A source status alone is insufficient.
Cross-workstream dependencies remain blocked until explicit export/adoption is
implemented, including when a client can read both workstreams. The admission
check is a snapshot; ongoing selective invalidation is not yet implemented.

The resource check covers cooperating AWR clients in this project's lexical path
namespace. It does not inspect client filesystems, separate physical worktrees,
reserve undeclared external services, stop a process, or provide a hard fence.
The client must honor lease expiry and cancellation. `fencing_class` stays
`uncontrolled` and `exactly_once_supported` stays false. Full workspace/external
resource identity is a later protocol.

`execution.report` takes `session_id`, `expected_session_version`, `execution_id`,
`expected_execution_version`, `outcome` (`succeeded`, `failed`, `cancelled` or
`unknown`), `output_digest` (64 lowercase hex, required for `succeeded`),
`observed_paths` (at most 128 canonical paths) and a nonempty `note` (at most 4 KiB).
Only the original actor/client with current write authority may report. An elapsed
claim does not prevent recording an observation; current session, ownership and
epoch binding still apply. Reports may be recorded while the workstream is paused.

These are **caller assertions**, not trusted result or stop confirmations. Each
report is kept verbatim in an attributed execution receipt, including observations
outside the declared scope and conflicting later reports. The execution becomes
`unknown`, recovery remains blocked and its work's reserved resources become
`unknown`; work is not completed and resources are not released. An already
terminal attempt cannot be rewritten by this operation. Unknown fields such as
`receipt_kind: "trusted_executor"` are rejected. A newer current contract does
not erase observations against the original execution contract.

## Executor attestations and operator reconciliation

Two commands can settle actual execution effects. Neither completes work, selects
a completion receipt, upgrades its execution policy or grants permission to run
again. Use `execution.inspect` first, then fresh `work.prepare` preconditions.

| Operation | Strict `args` object |
| --- | --- |
| `execution.attest` | `session_id`, `expected_session_version`, `execution_id`, `expected_execution_version`, `facts` |
| `execution.reconcile` | The same fields plus `expected_work_version`, `reviewed_receipt_id` (the latest inspected receipt ID, or null when absent), `clear_recovery_block` |

`facts` contains `outcome` (`succeeded`, `failed`, `cancelled`, `unknown`), the
original `input_digest`, optional `output_digest` (required for success),
`environment_digest`, `observed_paths` and `note`. Digests are 64 lowercase hex
characters. Paths and notes have the same bounds as `execution.report`. These
facts identify what the authorized reporter verified; the server does not inspect
the external process or independently hash its outputs.

An attestation requires an operator-provisioned `system` actor, effective write
access and the explicit `can_attest_execution` workstream grant. That authority
must exist **both at admission and when reporting**. Admission captures its grant
version and returns `result_authority: "trusted_executor"`; ordinary admission
returns `caller_asserted`. A later privilege change cannot upgrade an ordinary
or legacy attempt into trusted execution. Only the original actor/client/session
can attest, and the receipt kind is derived by the server as `trusted_executor`.
Caller strings cannot grant trust. An ordinary `execution.report` always remains
`caller_asserted`, even when submitted by a trusted executor.

Reconciliation requires an operator-provisioned `human` or `system` actor, admin
membership, effective write/manage access and explicit `can_reconcile_execution`.
An `agent` actor or admin membership alone is insufficient. The operator uses its
own current work-bound session and confirms the exact execution/work versions and
latest reviewed receipt. The new receipt is `reconcile`, never `trusted_executor`;
the original observation and executor attribution remain in history. Claim expiry
does not prevent settlement. Current ownership and coordinator epoch still apply;
adopting old ownership/epoch history requires a separate recovery protocol.

Terminal facts release only reservations bound to that execution. Unknown facts
retain reservations and block recovery. An executor that reports paths outside
its declared scope leaves the attempt unknown for operator review; an operator
cannot certify an out-of-scope success, but may reconcile failure or cancellation.
Existing terminal facts cannot be rewritten; an operator may confirm identical
facts to resolve an outstanding block.

Only reconciliation with `clear_recovery_block: true` may clear an existing work
block, and only when the result is settled and no other nonterminal execution or
reserved/unknown resource remains on the work. Clearing an unknown result is
rejected. Partial settlement keeps the remaining barrier and does not release
another attempt's resources. An executor cannot clear a work recovery block.

`execution.inspect` exposes current `attestation_authority` and
`reconciliation_authority`. Full `latest_receipt` facts are visible only to the
original actor/client or a currently authorized reconciliation operator; other
readers get metadata and `receipt_details_available: false`. Recheck after grant,
ownership, epoch, receipt or work-version changes. Exact retries return historical
command receipts and never repeat effects or issue execution permission.

Schema 13 gives existing grants neither new authority, leaves existing admissions
without attestation delegation, and retains legacy resource reservations as
unbound. It does not infer a resource's execution from today's work owner. Such
reservations cannot be released by these commands. Schema 14 adds the operator
provisioning CLI and its immutable receipts without granting existing clients
new rights. Bundled executor integration and enabled-project history migration
are still required for the complete Team execution workflow.

## Limits and errors

Requests are limited to 64 KiB, pages to 100 items, search to 512 bytes, and
requested context to 256 KiB (default 64 KiB). Complete serialized responses
have a 1 MiB ceiling. Oversized responses fail without truncating obligations;
reduce the page or narrow the selector. An individual oversized checkpoint still
requires a trusted operator recovery path. The listener shares 64 concurrent
request permits across HTTP and MCP, with a 30-second timeout. Responses use
`Cache-Control: no-store`.
Commands share the 64 KiB request limit, permits and timeout. A checkpoint's next
action is limited to 8 KiB and its open loops to 32 entries of 4 KiB each, within
the whole-request limit. HTTP timeout is not proof of transaction failure; use
the outcome-query procedure above. MCP limits include its entire JSON-RPC
request and response envelopes, including the text fallback; a result that fits
in HTTP JSON may need a smaller MCP page. Required text is never truncated.
MCP handlers retain their request permit and deadline even if the receiver
disconnects.

| HTTP status | Meaning |
| --- | --- |
| 400 | Invalid JSON, selectors or bounds |
| 403 | Missing/invalid credential, denied scope, or hidden/missing object |
| 409 | Scope/cursor/context limits, unavailable required dependency receipt, resource conflict, declared scope outside contract, stale preconditions/fence, held/expired lease, open wait, changed epoch, idempotency conflict, project barrier or unresolved recovery |
| 413 | Request body too large |
| 501 | Unsupported operation or protocol |
| 503 | Busy, timed out, transient transaction conflict or unavailable data |

Public errors do not include SQL, connection strings, bearer tokens or source
bodies. Database/file access outside the service remains an operator privilege;
HTTP authorization does not establish an OS sandbox.

Real PostgreSQL tests cover client/tenant separation, cursor binding, ownership,
dynamic revocation and both orders of concurrent credential revocation. HTTP
tests use a real loopback listener and PostgreSQL with simultaneous clients.
Session command checks cover owned journaling, concurrent replay/conflicts,
write revocation while a command is waiting, full rollback after an event failure,
frozen/paused state and unknown-execution preservation. These are
protocol/integration checks, not native coding-client business acceptance. Real
RMCP clients also verify discovery, simultaneous scope isolation, live revocation,
reconnection, HTTP/MCP receipt parity and refusal of oversized envelopes. Claim
checks cover concurrent acquisition, renewal/release, client ownership, epoch and
fence changes, expiry, atomic rollback, migration preservation and historical
replay versus live inspection over HTTP/MCP. Execution-intent checks cover exact
client ownership, scope/version guards, preparation/cancellation rollback, live
revocation, unknown-effect preservation, legacy migration and shared HTTP/MCP
receipts without dispatch. Admission tests additionally cover live lease/contract/
wait/resource checks, required receipt coverage, one-time start responses, atomic
rollback and preservation of unverified reports. Dispatch, trusted reporting,
reconciliation, history migration and enabled-project backup/restore remain
unavailable through this surface.
