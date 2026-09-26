# Scoped reference runner

The development source provides `awr-server runner` and the Rust
`ScopedReferenceRunner` adapter for workstream-enabled Team projects. It connects
the existing bounded file-write runner to authenticated execution admission and
result attestation. It does not launch coding agents, shell commands or remote
jobs, and the HTTP/MCP service does not dispatch effects itself.

Use the service **application-role** PostgreSQL connection in
`AWR_TEAM_DATABASE_URL`. Separately provision a system actor and client with
explicit read, write and `attest_execution` grants using the
[owner access CLI](team-operator-access.md). Merely having database access, an
admin membership or a mode named `reference_write_v1` does not grant execution
trust. Every admission and report checks the bearer and current scope inside the
coordinator transaction.

## Prepare and run

Create a JSON file-write plan:

```json
{
  "protocol_version": 1,
  "writes": [
    {"path": "src/generated/interface.json", "content": "{\"version\":1}\n"}
  ]
}
```

```sh
awr-server runner digest --input /absolute/path/plan.json
```

This validates and prints the input digest without executing or connecting to a
database. Plans contain 1–128 distinct portable relative paths and at most 1 MiB
of serialized JSON. Empty, duplicate, absolute, traversal and ambiguous paths
are rejected. The hash binds the protocol version, ordered writes, paths and
content; unknown fields fail. The command does not print file contents.

Use the scoped [session and claim protocol](team-workstream-service.md) to start
a durable work session and acquire its claim. Prepare an execution with the
returned `input_digest` and the intended `declared_scope`. Assemble `run.json`:

```json
{
  "tenant_id": "tenant-a",
  "project_id": "project-a",
  "command": {
    "protocol_version": 1,
    "request_id": "interface-write-1",
    "op": "execution.start",
    "workstream_id": "<workstream ULID>",
    "work_id": "generate-interface",
    "coordinator_epoch": "<current epoch>",
    "expected_project_revision": "<current revision>",
    "expected_authority_version": "<current authority version>",
    "expected_ownership_version": "<current ownership version>",
    "expected_contract_hash": "<current contract hash>",
    "args": {
      "session_id": "<owned session>",
      "expected_session_version": "<current session version>",
      "execution_id": "<prepared execution>",
      "expected_execution_version": "<prepared execution version>",
      "claim_id": "<owned claim>",
      "expected_fence": "<current fence>",
      "expected_lease_version": "<current lease version>",
      "expected_work_version": "<current work version>",
      "execution_mode": "reference_write_v1",
      "expected_input_digest": "<plan digest>"
    }
  },
  "plan": {
    "protocol_version": 1,
    "writes": [
      {"path": "src/generated/interface.json", "content": "{\"version\":1}\n"}
    ]
  }
}
```

Replace the placeholders with actual current protocol values; versions are
decimal strings. The plan digest must match both the provided plan and the
original prepared execution. The server checks the latter atomically with the
existing authority, contract, ownership, lease, dependency and resource checks.

```sh
awr-server runner run --input /absolute/path/run.json \
  --credential-file /secure/executor.token --root /operator-owned/runner
```

Only a fresh successful admission permits the adapter to execute. It saves the
admission before effects, then saves the observed result and exact attestation
request before reporting. The output identifies the saved `report_request_file`.
It never returns a reusable execution permission. Successful reporting settles
the execution and its bound resources; work completion and review remain separate.
On a start replay, `report_required` is `null`: the historical admission does not
establish current report status. A locally saved report path is returned when present.

Projects use separate directories beneath
`root/scoped-reference-v1/<tenant-and-project-hash>/`. Generated files are under
that directory's `worktree/`; `admissions/`, `observations/`, `reports/` and
`journal/` contain recovery records. These directories must persist across
retries. They contain paths and execution facts, not bearer credentials or the
original write bodies. Output files naturally contain the requested content.

## Recover without executing again

| Observation | Action |
| --- | --- |
| Admission request failed or reply was lost | Inspect the original request ID and execution. Exact retry returns history; it cannot run effects. |
| Admission committed but local persistence failed | Retain the receipt and use operator reconciliation. A fresh directory does not authorize another run. |
| File writes finished, report not confirmed | Inspect the report request ID, then retry the saved report. |
| Report conflicts with a newer project/session/execution version | Establish that the original report did not commit, inspect current facts, then submit a reviewed replacement with a new request ID and current preconditions. Preserve the saved facts. |
| Partial write, elapsed deadline or scope violation | Preserve unknown effects and resource barriers; operator reconciliation is required. |

```sh
awr-server runner report --input /path/returned/by/run.json \
  --credential-file /secure/executor.token --root /operator-owned/runner
```

The saved report has `tenant_id`, `project_id` and an `execution.attest` command.
`report` verifies its facts against the immutable local observation, while the
coordinator rechecks current authority and original execution ownership. It never
calls the file runner. A replay after successful reporting returns the historical
receipt without changing files or releasing unrelated resources. Journal facts
describe the observed execution, not a claim that its files still match today.

## Guarantee boundary

- Authority must be explicitly delegated both at admission and at reporting.
  Revocation prevents further authorized reports; it does not undo bytes already
  written under a committed admission.
- The adapter subtracts the admission round-trip time from the server's remaining
  lease and checks that conservative monotonic deadline before each file write.
  It cannot interrupt an OS write already in progress. An expired deadline stops
  further writes and retains an unknown result.
- The existing reference runner validates the complete write plan against the
  declared scope and rejects symlink escapes. Its local journal/fences apply to
  cooperating runs sharing this operator-controlled root. They do not isolate
  arbitrary native processes, separate hosts or undeclared external effects.
- Local files remain accessible to their OS owner. Use appropriate directory
  permissions/ACLs. This interface is not a sandbox for hostile agents.
- `fencing_class` remains `uncontrolled` and `exactly_once_supported` remains
  false. Crash recovery may require a person to settle unknown effects. Missing
  or corrupt journals never justify restarting the write plan.
- This adapter does not implement cross-workstream delivery adoption or general
  process supervision. Capabilities describe those boundaries explicitly.
