# Troubleshooting

Something in AWR is not behaving the way you expect. This guide works from symptoms to
fixes: health checks first, then database problems, then interrupted changes to your source
files. Each section gives the exact command to run and how to read the output. For the
day-to-day commands, see [Daily workflow](daily-workflow.md) and [CLI reference](cli.md).

## Step one: run a health check

AWR ships a built-in diagnostic command called Doctor, at two levels:

```sh
awr --json doctor --database-only
awr --json doctor
```

- `--database-only` checks the SQLite database itself: integrity, foreign keys, schema
  version, migration identities, and AWR-owned schema objects.
- Full `doctor` additionally inspects your selected source files and the bindings
  between runtime records and files on disk.

Doctor opens the database **read only**. It does not migrate the schema, reindex
sources, apply pending mutations, expire claims, or delete orphan files — so it is
always safe to run, even on a damaged project.

### Reading Doctor's findings

Doctor reports problems as explicit findings rather than guessing:

- `schema_issues` — missing or changed AWR-owned tables, columns, indexes, or triggers,
  checked against bundled migrations. Extra objects can coexist; Doctor will not repair
  definitions or print their SQL.
- `foreign_key_check_error` — a malformed reference prevented SQLite from even running
  its foreign-key check. If you see this field, the database is unhealthy, and a
  reported zero violations means "we could not count them", not "everything is fine".
- Active sessions — informational only; an old record does not prove the process is dead.
- Missing dependencies, dangling edges, unavailable sources, interrupted saves, and damaged
  or unregistered artifacts are each reported as findings. Unreadable pages or a foreign
  (non-AWR) database produce explicit errors, not a misleading status.

## Symptom: "AWR rejected my source file"

When a YAML source file has a syntax error or a field of the wrong type, AWR fails with
the error code `InvalidInput` and structured details:

```json
{
  "location": {
    "locator": "file:///project/work.yaml",
    "pointer": "/work_items/0/title",
    "line": 3,
    "column": 10
  },
  "rule": "ledger.string",
  "repair": "Use a quoted string or a YAML block scalar (|) for multiline text."
}
```

How to read it: `pointer` names the offending field using your source file's own vocabulary
(including configured Chinese field names); `line` and `column` are one-based. Pure syntax
errors may only have parser coordinates, and some lookups (such as YAML aliases) keep the
pointer with null coordinates — that does not reject an otherwise valid file. `rule` names
the failed expectation; `repair` describes the expected structure. It never edits the file
or echoes back the rejected value.

A common case: you wrote `title: [Draft, Review]`, which YAML parses as a list, but the
field requires a string. If the comma was meant literally:

```yaml
title: "Draft, Review"
```

For multiline descriptions use a YAML block scalar (`|`); valid Chinese text and paths with
spaces are fully supported.

## Symptom: "My changes don't show up in queries"

You edited a source file but AWR still shows old facts. Run a reindex:

```sh
awr source reindex
```

Then check **both** parts of the report: operation success *and* projection completeness. A
scan can succeed while the projection stays incomplete, because scanning observes sources
without importing their changed facts. The same report comes as JSON with `--json` and via
MCP `awr_source_reindex`. Compare output from equivalent starting snapshots; reindexing itself
updates source state.

If a work item's evidence looks off, `work show` (or MCP `awr_work_get`) reports
`evidence_groups` next to the flat `evidence` list. Each group has one exact locator; a source
reference and a verified report at the same path stay distinct, and grouping never transfers
verification between records.

## Symptom: "The database is corrupt or lost"

Your source files are authoritative for project facts, but the SQLite database
(`.awr/state.db`) also holds state those files do not contain: sessions, claims, checkpoints,
runtime events, artifact registrations, and mutation attempts. Reindexing refreshes projections
from the sources, but it cannot rebuild that runtime history — and neither can initializing a
fresh database from the same sources.

### Backing up properly

For a recoverable snapshot, keep all of these together, and pause project writers while
assembling them (SQLite's backup API gives a coherent database image, but does not snapshot
the surrounding files):

- A coherent backup of `.awr/state.db` made with SQLite's backup API or equivalent
  tooling — copying the file while writers are active can miss committed records still
  in the write-ahead log.
- The matching source files, Git revision, `.awr/project.toml`, authorized-root
  configuration, and any explicitly authorized external sources.
- Managed artifact files, registered local artifacts outside the managed directory, and
  the `.awr/mutations` recovery journals and snapshots referenced by pending operations.

### Restoring

1. Stop the project.
2. Restore at the recorded canonical root; keep the displaced state rather than deleting it.
3. Run `awr --json doctor --database-only`, then full `awr --json doctor`.

A restore can recover an artifact registration while its file is still missing — restore the
file or leave the finding unresolved. `awr source reindex` is neither a runtime restore nor a
database relocation tool.

Doctor can perform named repairs, but only with the current project revision, a selected
object, and a reason; repairs preserve sources and unrelated runtime state. If database
integrity is broken, suggested runtime repairs are disabled — no automatic reconstruction.

## Symptom: "A mutation was interrupted mid-write"

AWR applies approved proposals to your source files with durability guards: immutable
before/after snapshots are saved before the attempt is recorded, and success requires the
intended source bytes, a rebuilt projection, and the final application event. If a process dies
mid-write, the attempt stays open and recoverable.

If you get `MutationIncomplete` with `write_outcome: pending_recovery`, inspect first:

```sh
awr --json doctor
awr --json proposal show PROPOSAL_ID
```

Read the current `project_revision` from the output, then resume the proposal:

```sh
awr --json proposal recover PROPOSAL_ID \
  --actor reviewer \
  --reason "Resume the interrupted report review" \
  --expected-revision CURRENT_REVISION
```

What recovery does depends on how far the attempt got:

| State after interruption | What recovery does |
| --- | --- |
| Snapshots exist, no application journal | Source is unchanged; a fresh `proposal apply` can begin. |
| Journal exists, source still matches "before" | Revalidates ownership, configuration, source and revision, then installs the recorded "after" bytes. |
| Source already matches "after", projection incomplete | Rebuilds the projection without rewriting the file. |
| Projection committed, final event absent | Validates the projection and commits the final event — no second file write. |
| Final event committed, response lost | Returns the durable receipt; repeating recovery fails with `InvalidTransition`, no duplicate event. |
| Source or snapshots disagree with the recorded plan | Preserves the source, reports the conflict, withholds completion. |

A successful retry reports `recovered`, names the original attempt, and binds its
`resolved_event_id` to the final event.

Two things not to do:

- **Do not edit the snapshots to force an outcome.** If the current source matches neither
  snapshot, resolve the source conflict and prepare a new proposal.
- **Do not assume a retry wrote the file.** `source_write_performed` describes only the current
  invocation — which is why recovery revalidates first.

Concurrency: AWR writers coordinate via a per-source OS lock and transactional
`expected_revision` checks. Separate sources progress independently, but an intervening
project revision can require an explicit recovery retry. External editors do not join the
lock; fingerprint checks detect their changes.

## When to stop and ask for help

Stop and escalate instead of improvising when:

- Doctor reports broken database integrity (`foreign_key_check_error`, unreadable pages) —
  runtime repairs are disabled, with no automatic reconstruction.
- A mutation recovery reports a source/snapshot conflict — the supported path is a new
  proposal, not hand-editing snapshots or journals.
- Your scenario involves machine power loss, distributed writers, or other operating systems —
  recovery guarantees cover local process interruption; beyond that is not established.

Before asking for help or filing an issue, capture the state while it is fresh:

```sh
awr --json doctor --database-only
awr --json doctor
awr --json proposal show PROPOSAL_ID   # if a mutation is involved
```

Keep the displaced database, the `.awr/mutations` journals, and the relevant source files
untouched — they make a report actionable, and they are your fallback if the fix goes wrong.
