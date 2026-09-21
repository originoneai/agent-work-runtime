# Team workstream HTTP service

The development branch provides authenticated, multi-project HTTP queries and
durable session journaling backed by PostgreSQL. This is not a release announcement
or a complete Team execution service. It does not dispatch executions, expose
Team MCP tools, resume agents or adopt cross-workstream deliveries. Use its live
capabilities response to discover available operations.

## Start an operator-bound service

Build `awr-server` from this source branch. Migrate the intended database to
schema 10 explicitly as its owner, and apply application-role grants using the
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

Provisioning is currently a trusted operator integration, not a public HTTP
endpoint or an installed credential-management CLI. The operator creates an
active tenant and actor, project membership, a credential, and explicit
workstream grants using the coordinator database administration boundary.
Do not give agent clients direct database credentials.

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

The response advertises the following queries and three session commands.
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

## Durable session commands

POST to `/v1/projects/<alias>/command` with the same bearer authentication. The
currently supported operations record a session journal, not execution rights:

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

## Limits and errors

Requests are limited to 64 KiB, pages to 100 items, search to 512 bytes, and
requested context to 256 KiB (default 64 KiB). Complete serialized responses
have a 1 MiB ceiling. Oversized responses fail without truncating obligations;
reduce the page or narrow the selector. An individual oversized checkpoint still
requires a trusted operator recovery path. The listener permits 64 concurrent
queries with a 30-second query timeout. Responses use `Cache-Control: no-store`.
Commands share the 64 KiB request limit, permits and timeout. A checkpoint's next
action is limited to 8 KiB and its open loops to 32 entries of 4 KiB each, within
the whole-request limit. HTTP timeout is not proof of transaction failure; use
the outcome-query procedure above.

| HTTP status | Meaning |
| --- | --- |
| 400 | Invalid JSON, selectors or bounds |
| 403 | Missing/invalid credential, denied scope, or hidden/missing object |
| 409 | Scope/cursor/context limits, stale preconditions, changed epoch, idempotency conflict, project barrier or unresolved recovery |
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
protocol/integration checks, not native coding-client business acceptance. Execution
writes, Team MCP, history migration and enabled-project backup/restore remain
unavailable through this surface.
