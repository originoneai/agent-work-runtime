# Team operator access management

The development branch provides `awr-server access` for an operator to provision
clients of an already enabled workstream project. It does not create a project,
activate sources, launch an executor or announce a release. Complete the
[Team service setup](team-workstream-service.md) first.

Database operations require the schema owner's PostgreSQL privileges through
`AWR_TEAM_DATABASE_URL`. Ordinary service application credentials and HTTP/MCP
bearers cannot use this operator interface. The running service continues to use
its separate application connection. Upgrade explicitly with
`awr-server migrate --app-role <service-role>` as owner; schema 14 adds operator
receipts, and bootstrap denies the application role all access to that table.

## Register a client

Generate a bearer before registration, into a new file in an operator-controlled
directory:

```sh
awr-server access token --credential-id worker-one --output /secure/worker-one.token
```

The command uses operating-system randomness and prints only the credential ID,
registration hash and output path. It does not connect to PostgreSQL or register
anything. The file contains the bearer, is created exclusively, and is never
overwritten (including a symlink). Unix files use mode `0600`; on other systems,
the containing directory must provide the intended user ACL. Keep the bearer out
of command arguments, source files, logs, reports and version control. A file-write
failure registers nothing; inspect/remove the incomplete local file explicitly
before generating another credential.

Inspect the intended identity and available workstream catalog:

```sh
awr-server access inspect --tenant-id tenant-a --project-id project-a \
  --actor-id worker --client-id coding-client
```

Save an access plan as local JSON. Use an actual workstream ID and current authority
version from inspection, and replace the hash placeholder with `access token`'s
`secret_hash` output:

```json
{
  "protocol_version": 1,
  "tenant_id": "tenant-a",
  "project_id": "project-a",
  "actor": {"id": "worker", "kind": "agent", "display_name": "Coding worker"},
  "client_id": "coding-client",
  "role": "worker",
  "grants": [{
    "workstream_id": "00000000000000000000000001",
    "authority_version": "1",
    "read": true,
    "write": true,
    "manage": false,
    "attest_execution": false,
    "reconcile_execution": false
  }],
  "credential": {
    "id": "worker-one",
    "secret_hash": "sha256:<64 lowercase hex characters from access token>",
    "expires_at_unix_ms": null
  },
  "revoke_credentials": []
}
```

`actor.kind` is `agent`, `human` or `system`; `role` is `reader`, `reviewer`,
`worker` or `admin`. Existing actor kind/name must match and are not overwritten.
An absent actor is created active. Read permission is required for every listed
grant. Worker/reviewer/admin membership may carry write permission; manage requires
admin. Executor attestation requires a system actor plus write permission.
Reconciliation requires human/system, admin and write/manage. These are explicit
trust delegations: ordinary coding agents should not receive executor/operator
identities. The service also checks authority at actual command execution.

Preview, review the exact scope, then apply using the returned `state_digest`:

```sh
awr-server access preview --input /secure/worker-access.json
awr-server access apply --input /secure/worker-access.json \
  --request-id register-worker-one --expected-state <state_digest>
```

Preview performs no policy mutation. Apply requires the reviewed state to remain
current and commits policy, versions, audit event and an immutable receipt together.
A stale preview is rejected. The plan is bounded to 64 KiB and 256 grants/revocations;
unknown fields are rejected. Share the bearer with the intended client through its
supported secret configuration; send it as the service's Authorization bearer.
The database stores its hash, never the raw bearer.

## Change, rotate or revoke access

The grants array is the **complete desired grant set for this actor/client/project**.
Omitted grants are deactivated, with their history and incremented versions retained.
Other clients' grant rows are preserved. An empty array removes all of this client's
workstream grants in the selected project. Reactivation requires another explicit
plan against the current catalog and never rewrites historical receipts.

Membership role is shared by **all clients of this actor in the project**. A role
change therefore affects their effective permissions even when their grant rows
are unchanged. Credential revocation is **tenant-wide across all projects that use
that credential**. Preview labels both boundaries; create separate actors or
credentials when independent administration is required.

Set `credential` to null to preserve existing credentials. To rotate, generate a
new credential ID/file, include its hash in `credential`, and list the old ID in
`revoke_credentials`. Registration and revocation commit atomically. Revocations
must name credentials of the selected actor/client; another client's ID is refused.
A credential ID cannot be rebound to another identity, hash or expiry, and revoked
credentials cannot be resurrected. Use a new ID. Expiry is an optional future Unix
timestamp in milliseconds. Grants to another workstream still require its explicit
current authority version.

## Recover an uncertain outcome

```sh
awr-server access outcome --tenant-id tenant-a --project-id project-a \
  --request-id register-worker-one
```

A committed result returns the original receipt. An unknown result is not proof
that an in-flight request failed. Retry only the identical plan, request ID and
expected-state digest. Exact retry returns the historical receipt without changing
policy; it cannot restore privileges that were later revoked. Reusing a request ID
with changed parameters fails. Use a fresh preview and request ID for a new intent.
Receipts retain previous/current policy and omit credential hashes and bearer strings. Their `state_basis: at_commit`
is historical; use `access inspect` for current policy.

This interface does not bypass project/source activation or migrate old execution
ownership. It preserves admission-time executor trust: granting attestation now
does not retroactively authorize an earlier ordinary execution.
