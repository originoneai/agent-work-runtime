# Doctor and database recovery

`contract.json` version 1.0.0 defines 24 component, process and CLI conditions for its isolated fixtures. It includes database ownership, schema definitions, migration
rollback, diagnostic findings, WAL recovery, source projection rebuild and
restoring runtime data from a coherent backup.

Run from the repository root with a new receipt path:

```sh
python3 tests/recovery/doctor/verify.py --report .local/database-recovery-001.json
```

The runner executes 20 primary tests, all Store tests and the directly related CLI
and MCP checks. It builds both binaries, checks all workspace targets and records
the contract, source inputs, commands, logs and executable hashes. Every stage
must pass, inputs must remain unchanged and the 24 observed IDs must exactly match
the current contract. Historical results do not complete an unexecuted condition.

Each test owns a marked temporary project. SQL fault injection, missing files,
bad schema, migrations and database replacement occur only in those fixtures.
Snapshots compare schema, persisted rows, version and source fingerprints;
artifact tests also compare their actual files. No SQL definitions or private
fixture bodies should appear in Doctor findings.

The two WAL cases start a distinct child test process, check its reported PID
against the live process handle and wait until commits are retained in WAL. A
copy of the main database without WAL must still lack those sessions. The parent
kills the live child and verifies the reopened committed state, including rollback
of a separate uncommitted transaction. `fixture_wal_writer` is intentionally
ignored in ordinary Cargo runs and invoked explicitly by these parent tests.
Shipped binaries do not read its fixture environment variable or invoke models.

The backup case uses SQLite's backup API, then restores a stopped fixture at its
recorded root. It proves that a restored registration still needs the matching
artifact file. A separate fresh-source rebuild must have no invented runtime
history. These are local recovery checks, not power-loss, relocation, distributed
backup, cross-platform, E4 or release acceptance.

See [the recovery reference](../../../docs/reference/database-recovery.md) for the
operator-facing boundaries.
