# Team PostgreSQL store

Team V1 coordination state lives in PostgreSQL. Personal CLI/MCP still use
SQLite and do not link this store.

## Local verification

```sh
docker compose -f docker/team-postgres.yml up -d
export AWR_TEAM_DATABASE_URL='postgres://postgres:awr-test@127.0.0.1:55432/awr_team_test?sslmode=disable'
cargo run -p awr-server -- migrate
cargo test -p awr-team-pg --features pg-tests
```

`awr-server check` exits non-zero when `awr_team.schema_state` is missing or
the version does not match. The command entry (`TeamStore::execute`) runs the
same check before opening a write transaction.

Upgrading a database bootstrapped by an older build: run
`awr-server migrate --app-role <role>` once with owner credentials. This
re-applies the application grants (idempotent, no schema rebuild, no data
loss); older grant sets did not allow the app role to read `schema_state`. `awr-server migrate` applies owner migrations on
a clean database and returns successfully when the expected version is
already present. The application role is not table owner and does not
receive `BYPASSRLS`. Event history is insert-only for that role.

Source publish is ingest → approve → activate. Path checks, hashing and
parser binding happen before the project lock. The lock only writes already
hashed, immutable rows. An unactivated candidate cannot be read as the
current contract. A failed activation keeps the previous `active_snapshot_id`.

## Connection pooling and TLS

Domain stores acquire connections from a `deadpool-postgres` pool instead of
opening one TCP connection per operation (ADR-0004). Pool size defaults to 8
per store instance and can be overridden with `AWR_TEAM_PG_POOL_MAX_SIZE`.
Acquire/create/recycle timeouts are fixed at 10s/5s/5s. Scope binding uses
transaction-local `set_config`, so fast connection recycling is safe.

Owner migration commands (`migrate`, `check_schema`) keep a dedicated single
connection and do not use the pool.

TLS is an opt-in `tls` cargo feature (rustls + webpki-roots). With the feature
enabled, `sslmode=require` in `AWR_TEAM_DATABASE_URL` selects a verified TLS
connection; `disable`/`prefer` or an omitted sslmode stays plaintext. Without
the feature, a TLS-requiring URL fails with an explicit error instead of
silently downgrading.

This is not a production high-availability topology.

## Consistent reads

`work.prepare`, `work.graph`, `session.inspect` and `events.list` run in
`REPEATABLE READ`. Event cursors are `awr-team-cursor-v1:{epoch}:{revision}:{index}`.
A changed coordinator epoch returns `EPOCH_CHANGED` instead of skipping history.
Required hard rules are never dropped to fit a context budget.

Experimental entry:

```sh
cargo run -p awr-server -- query --op capabilities
cargo run -p awr-server -- query --op work.prepare --body '{"tenant_id":"...","project_id":"...","work_id":"work-a"}'
```

Unknown query names return `Unsupported` without changing personal CLI/MCP.

## Sessions and leases

Claims are unique per work item while `state='active'`. Lease expiry uses
`clock_timestamp()` after the project lock, not transaction `now()`. Renew
replays keep the original `expires_at`. Wait records do not extend the lease.
Handoff increments the work fence so the previous actor cannot write.

## Dependencies and conflicts

Required dependency graphs are rejected if they cycle or reference missing
work. Resource reservations treat directory prefixes as overlapping path
segments (`src/foo` vs `src/foo/bar`), not raw string prefixes (`src/a` vs
`src/abc`). Splitting a work item does not complete the parent. Unknown
scopes are rejected instead of falling back to `main`.


## Execution protocol

`execution.prepare` writes the execution row, effect key and outbox record in one
transaction. Outbox delivery is claimed with `SKIP LOCKED` after the project lock
and sent outside that transaction. The reference runner persists `execution_id`
before side effects; a duplicate delivery returns the journaled outcome without a
new effect key. `unknown` sets `recovery_blocked` and keeps resource reservations.
`cancel_requested` is not `cancelled`. Callers cannot mint `trusted_executor`
receipts. Uncontrolled third parties do not receive an exactly-once claim.


## Evidence and completion

Completion receipts are written only through the domain `complete` entry.
`caller_asserted` reports cannot satisfy `trusted_execution_and_review`.
Authors cannot approve their own review round; a new bundle hash invalidates
the previous round. `work_runtime.state='completed'` requires
`selected_completion_id`. Ordinary confirmation is allowed only when the
current contract already selects that policy.


## Import and restore

Import is freeze → export → dry-run → load → activate. The same
`import_key` and manifest hash replay the original job and do not create
duplicate work. Divergent local sources are rejected instead of last-write
wins. Historical self-reports stay `caller_asserted`. Restore mints a new
coordinator epoch, revokes restored credentials, fails pending outbox rows
instead of replaying them, and refuses a SQLite file rollback after the
team project has accepted new revisions.


## Import, freeze and restore integrity (schema 9)

`ImportStore` is the administrative cutover boundary. Drain running work before
freezing: ordinary command, source, session, execution, graph and review writes
serialize on the project row and require `status=active`. Freeze is idempotent.
Export requires a frozen project and uses a repeatable-read transaction plus the
coordination lock, returning the source project, snapshot, epoch and revision.
Read-only inspection remains available while frozen.

The `awr-team-import-v1` JSON manifest has `scopes: ["main"]`, a `works` array,
and an `evidence` array. Each work requires explicit `id` and `external_key`, and
an activatable manifest also requires a valid `awr-team-contract-v1` `contract`.
An optional `contract_hash` must recompute. Dependencies must name works in the
manifest and form an acyclic graph. Import into an existing project must cover
its existing work identities without remapping them. No local claim becomes a
Team lease.

Each evidence item requires `id`, `work_id`, `contract_hash`, `evidence_kind`,
and an object `payload_json`. Optional `input_digest`, `output_digest` and
`execution_result_digest` preserve their separate meanings. `artifact_bytes`
is a JSON byte array; when supplied, its SHA-256 must equal `output_digest`.
Missing bytes stay visible in the saved validation report and prevent activation.
There is no invented default work. Imported trust is always `caller_asserted`;
original trust, actor, execution and artifact identities remain in the immutable
manifest/source reference as provenance, never as live target-project authority.
New local artifact identities and evidence digests are computed on load. Historical
material needs new local execution/review verification to meet strict completion.

Call `freeze`, `load`, then `activate`. Load validates and stores the exact manifest,
report, source snapshot, contracts, dependency edges and evidence bytes. Its retry
identity is `(tenant, project, import key, canonical manifest hash)`. Only an exact
retry returns the old job without another event. Activation verifies the report,
stored projection, evidence and actual claims/recovery/executions in its own
transaction before switching the active snapshot. The boolean unknown hint can
veto activation, but `false` cannot override database facts. Metadata-only legacy
manifests can be staged but cannot activate. Already-staged incomplete imports
remain unavailable pending administrative repair/recovery; this API does not
silently drop missing material or overwrite an existing evidence identity.

Backup registration records a verified logical inventory, not a physical database
backup. Physical snapshot/restore remains the operator's responsibility. The
inventory binds the schema, active source, all source content/projections and all
artifact bytes. Requested source/artifact digests must exist. Restore rechecks
that inventory; a caller's `artifacts_present=true` is insufficient. Legacy backups
without an inventory cannot pass verification. A failed integrity check for an
existing backup records a blocked run, leaves the project degraded and revokes old
authorization. Repair the physical materials before retrying restore.

Successful restore changes the coordinator epoch, revokes active claims, interrupts
sessions, advances work fences, marks unfinished executions unknown and reservations
unknown, fails pending/sending dispatches, and leaves work recovery-blocked. New
execution admission checks both the recorded epoch and recovery flag. Credentials
are tenant-scoped in V1, so restoration conservatively revokes tenant credentials;
operators must account for other projects using those credentials.

**Resource boundary:** a database cannot retract commands already delivered to an
offline resource. Install every returned `fencing_barriers` entry at each resource
before clearing recovery state. `ReferenceRunner::install_recovery_barrier` persists
the fence under the same OS lock used for the entire effect phase, so older deliveries
cannot write after installation. If a resource already observed a higher fence than
the restored database, installation refuses; retain the recovery block and reconcile
that high-water mark before resuming. Confirm resource effects and use the authorized
reconciliation path; do not replay the pre-restore outbox. A completed restore run
means inventory verification and coordinator isolation, not completed business work
or proof that a disconnected resource has acknowledged the new fence.

Freeze, load, activation, backup and restore write lifecycle events and project
revisions in the same transaction. Event failure rolls back the entire transition.
Schema 9 quarantines old `import_jobs.project_id=NULL` rows behind project RLS and
rejects new unbound jobs. Existing non-null jobs must reference a real project;
repair invalid legacy project references before migrating. Pre-9 executions have
no verified coordinator epoch and cannot obtain new accept/start admission.
