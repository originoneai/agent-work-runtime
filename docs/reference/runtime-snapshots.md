# Matched runtime snapshots

The native CLI exposes `runtime.matched_snapshot` through `awr capabilities`.
These commands bind the executing CLI bytes, schema, project identity and root,
configuration, registered source bytes and retained runtime history. They never
rewrite a host's `runtime-binding.json` or switch its executable pin. A development
build and a published build can report the same version while having different
binary hashes; the hashes determine restore compatibility.

```sh
awr --project /absolute/project --json runtime binding
awr --project /absolute/project --json runtime backup \
  --output /absolute/backups/new-snapshot \
  --companion /absolute/bin/awr-mcp
awr --json runtime check --backup /absolute/backups/new-snapshot
awr --project /absolute/project --json runtime restore-preview \
  --backup /absolute/backups/new-snapshot
```

Create the output's parent first. The output must be new and outside `.awr`.
`--companion` is optional: it retains and hashes the explicitly selected executable;
it does not infer protocol or version compatibility. Optional `--source-sha` records
a caller assertion; it is not a build attestation. Retain the release's verified
build manifest separately when commit-to-program provenance is required.

`binding`, `check` and `restore-preview` are read only. The backup command writes
only its new output directory. It refuses stale source content, changed mappings,
unindexed directory members and damaged databases. It validates source inventory
in a RAM copy without refreshing the original project's projections. A coherent
SQLite image includes committed WAL history. Sources and persistent runtime files
are compared again before sealing; concurrent file changes require a fresh retry.
This is an optimistic file consistency check, not a filesystem-wide transaction.

The bundle contains:

- `snapshot.json`: versioned identities, revisions, source bindings, payload sizes,
  hashes and the manifest fingerprint.
- `program/awr` and optional `program/awr-mcp` (with `.exe` on Windows): retained
  executable bytes for the original platform.
- `runtime/state.db`: a standalone SQLite backup made from a coherent RAM image.
- Other persistent `.awr` files, including managed artifacts, host binding files
  and mutation/checkpoint journals. Database sidecars, `.lock` files and the
  transient restore marker are excluded.
- `sources/<source-id>.bin`: exact registered source content, including supported
  Git locators and explicitly authorized external sources.

External artifacts, unregistered business files and host state outside `.awr`
need their own backup. A restored artifact registration does not restore an
excluded file. Bundles can contain private work, so keep them in private storage;
they are not release assets. On Unix, new directories are owner-only, data files
are mode 0600 and executable copies are mode 0700. Limits are 256 MiB per file,
1 GiB combined and 10,000 payloads. An interrupted or rejected backup without a
valid final manifest is unsealed and cannot be restored. Existing output paths
are never reused automatically.

`check` verifies manifest integrity, all listed payload bytes, SQLite integrity,
schema, project identity, history revision and registered source bindings. It does
not need the original project to exist. Fingerprints use `sha256:` followed by
hexadecimal SHA-256; the manifest digest covers compact JSON with recursively
sorted object keys and an empty `fingerprint` field. Digests establish integrity,
not authorship; only use backups whose provenance you trust.

## Preview and restore

Automatic restore replaces only `.awr/state.db`. It requires the original canonical
project root, current schema and exact executing CLI bytes. If a companion was
retained, its original path must still contain the matching bytes. Every registered
source and every known persistent runtime/configuration file must still match the
backup. Changed or missing known files are rejected instead of overwritten. A
source relocation, changed configuration or schema migration needs its separate
reviewed recovery procedure.

Additional files remain in place, including files added inside `.awr`. The preview
lists those additional runtime files. Database history after the restore revision
will be displaced; the exact database, WAL, SHM and journal bytes being replaced
are retained under `.awr/restores/<id>/rollback/`. Newly created business files
are not scanned, deleted or overwritten. They may be unregistered after a history
restore and need explicit review.

The current database must have a verifiable matching AWR identity. A wholly absent
database with no sidecars is supported when the configuration, sources and known
runtime files still match. An existing unreadable, foreign or unsupported database
is refused: preserve and diagnose it separately. This command is not an automatic
repair of arbitrary corruption or a cross-root migration tool.

Stop all AWR clients, hosts and execution supervisors before taking the restore
preview. Then pass its exact `fingerprint`:

```sh
awr --project /absolute/project --json runtime restore \
  --backup /absolute/backups/new-snapshot \
  --expected-preview 'sha256:<reviewed-preview-hash>' --offline
```

`--offline` acknowledges that **all** clients are stopped, including older AWR
versions. New native clients hold a shared runtime lease; restoration requires an
exclusive lease and fails if they remain open. Older versions do not participate
in this lock, so the flag is an operator assertion, not proof of their absence.
Target changes invalidate the preview. Restoration retains rollback bytes and a
durable receipt before replacing the database, validates the installed history,
then removes the pending marker. Normal opens refuse a pending restore.

## An interrupted restore

```sh
awr --project /absolute/project --json runtime restore-status
awr --project /absolute/project --json runtime restore-recover --offline
awr --project /absolute/project --json runtime restore-status --id <restore-id>
```

Status works without opening SQLite. Recovery resumes only the recorded request
from its exact before/after database state, matching backup and matching sources,
configuration and known runtime files. An interrupted sidecar removal is accepted
only if remaining sidecars match their recorded bytes. External database changes
leave the marker in place and require inspection; recovery never guesses a new
baseline. If replacement already completed, it finalizes the receipt/marker only.
A completed restore is never replayed over later history. Retain all journal and
rollback files while investigating a refusal.

After success, restart clients, inspect `doctor`, and compile fresh context before
work. Restored claims retain their original lease times, and host-side request
receipts may describe history that was displaced. Do not automatically replay
those requests. The local CLI tests cover history identity, checkpoints/evidence,
source drift, preserved additions, runtime leases, missing databases and simulated
interruption before/after replacement. They do not establish machine power-loss,
native application acceptance or execution-supervisor recovery on other platforms.
