# Database health and retained runtime data

Source files are authoritative for project facts. SQLite also holds state that
those files do not contain: sessions, claims, checkpoints, runtime events,
artifact registrations and mutation attempts. Reindexing source files refreshes
their projections and derived search; it preserves retained runtime ownership and
old receipts. Initializing a new database from the same source files cannot
reconstruct missing runtime history.

## Inspecting a database

```sh
awr --json doctor --database-only
awr --json doctor
```

Doctor opens the database read only and does not migrate, reindex, apply pending
mutations, expire claims or delete orphan files. Database-only mode checks SQLite
integrity, foreign keys, schema version, migration identities and owned schema
definitions. Full diagnosis also inspects the selected sources and runtime/file
bindings. Active sessions are informational; their age does not prove process
death. Missing dependencies, dangling edges, unavailable sources, interrupted
saves and damaged or unregistered artifacts remain explicit findings.

`schema_issues` identifies missing or changed owned tables, columns, indexes and
triggers. Definitions must match the bundled migrations, including immutable
event guards and the unique active-claim index. Extra diagnostic objects can
coexist with the owned schema. This comparison does not infer semantic equivalence
for manually rewritten SQL, repair those definitions or emit their SQL bodies.

If a malformed reference prevents SQLite from running its foreign-key check,
`foreign_key_check_error` is present and the database is unhealthy. In that case,
zero reported violations is only the observed count, not a successful check.
Unreadable database pages or a foreign database produce explicit errors.

Normal read/write opens require the current owned schema. `Store::open` can
upgrade supported older schemas; it validates the base before startup changes,
rechecks it under the writer lock, and verifies the result before committing the
upgrade. A rejected catalog or migration SQL failure cannot leave new tables,
catalog rows or a new schema version committed. SQLite connection/WAL setup is
distinct from the migration transaction. Doctor never invokes that upgrade path.

## Backup and recovery

The native [`runtime` snapshot commands](runtime-snapshots.md) provide coherent
backups, integrity checks, same-root matching restore previews and explicit offline
database restoration. They retain source/configuration copies for verification,
preserve added files and refuse source drift. The broader manual recovery inventory
below also covers external files and schema upgrades outside automatic restore.

For a recoverable project snapshot, retain all of the following together:

- A coherent SQLite backup of `.awr/state.db`, made with SQLite's backup API or
  equivalent tooling. Copying only the main file while writers are active can omit
  committed WAL records.
- The matching source files, Git revision, `.awr/project.toml`, authorized-root
  configuration and any explicitly authorized external sources.
- Managed artifact files, registered local artifacts outside that directory and
  `.awr/mutations` recovery journals/snapshots referenced by pending operations.

Pause project writers while assembling this set so the database, sources and
external files describe the same recovery point. SQLite's backup API provides a
coherent database image; it does not atomically snapshot the surrounding files.

Restore to a stopped project at its recorded canonical root, retain the displaced
state, then run database-only and full Doctor checks. Database restoration can
recover an artifact registration while its file is still missing; restore the
matching file or keep the finding unresolved. Source reindex is neither runtime
restoration nor a database relocation tool. Review pending mutation journals
before selecting an explicit recovery operation.

Named Doctor repairs require the current project revision, a selected object and
a reason. They preserve source files and unrelated runtime state. Broken database
integrity disables suggested runtime repairs; there is no automatic reconstruction
of missing references or a healthy status inferred from source files alone.

The [recovery matrix](../../tests/recovery/doctor/README.md) verifies these local
boundaries with temporary projects, real CLI calls and killed live writer
processes. It does not establish machine power-loss or other-platform behavior.
