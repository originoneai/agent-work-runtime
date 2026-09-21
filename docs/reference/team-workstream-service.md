# Team workstream read service

The development branch provides an authenticated, multi-project HTTP **read**
service backed by PostgreSQL. This is not a release announcement or a complete
Team execution service. It does not dispatch commands, expose Team MCP tools,
resume agents or adopt cross-workstream deliveries. Use its live capabilities
response to discover available operations.

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

The response advertises only the following queries and an empty `commands`
array. Unsupported operations or protocol versions fail explicitly.

| Operation | Selectors and result |
| --- | --- |
| `workstreams.list` | Authorized workstreams only; `limit`, `cursor` |
| `work.list` | Selected workstream's work summaries and count; `limit`, `cursor` |
| `work.search` | Same visibility boundary; required literal substring `search` |
| `work.prepare` | Required `work_id` or `session_id`; optional `max_context_bytes` |
| `events.list` | Selected workstream, optionally narrowed by work/session; metadata only |
| `session.inspect` | Required `session_id`; its current-ownership checkpoint |
| `work.recovery` | Required work/session; up to two current-ownership recovery candidates |

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

## Limits and errors

Requests are limited to 64 KiB, pages to 100 items, search to 512 bytes, and
requested context to 256 KiB (default 64 KiB). Complete serialized responses
have a 1 MiB ceiling. Oversized responses fail without truncating obligations;
reduce the page or narrow the selector. An individual oversized checkpoint still
requires a trusted operator recovery path. The listener permits 64 concurrent
queries with a 30-second query timeout. Responses use `Cache-Control: no-store`.

| HTTP status | Meaning |
| --- | --- |
| 400 | Invalid JSON, selectors or bounds |
| 403 | Missing/invalid credential, denied scope, or hidden/missing object |
| 409 | Ambiguous/mismatched scope, expired cursor, incomplete context or oversized response |
| 413 | Request body too large |
| 501 | Unsupported operation or protocol |
| 503 | Busy, timed out, transient transaction conflict or unavailable data |

Public errors do not include SQL, connection strings, bearer tokens or source
bodies. Database/file access outside the service remains an operator privilege;
HTTP authorization does not establish an OS sandbox.

Real PostgreSQL tests cover client/tenant separation, cursor binding, ownership,
dynamic revocation and both orders of concurrent credential revocation. HTTP
tests use a real loopback listener and PostgreSQL with simultaneous clients.
These are protocol/integration checks, not native coding-client business
acceptance. Authenticated writes, Team MCP, history migration and enabled-project
backup/restore remain unavailable through this surface.
