# Host application contract

A host can invoke a pinned native `awr` binary with an argument array and separate
stdout/stderr pipes. AWR does not require a visible terminal, model client, HTTP
service or daemon. Pass the executable's absolute path; never interpolate project
paths or user content into a shell command. A host owns its UI, client credentials,
scheduling, notifications and knowledge processing. AWR owns the source-backed work
projection, runtime records and its supported source mutations.

## Discover before opening a project

```text
awr capabilities --json --protocol-version 1 --require source.read --require context.compile
```

This command neither opens a project nor reads, creates or migrates its database.
It works before initialization and ignores the global `--project` argument. Its
JSON contains:

| Field | Meaning |
| --- | --- |
| `protocol.name`, `protocol.version` | `awr.host` capability negotiation contract, currently version 1 |
| `program` | Build's program version and native OS/architecture, not proof of other platform releases |
| `database` | Current schema, read-only compatible schemas, schemas eligible for migration and future-schema rejection policy |
| `source_adapters` | Accepted adapter IDs and actual read/write limits |
| `capabilities` | Stable IDs, availability, command entrypoints and coded limitations |
| `scope` | Build capabilities only; never project-specific permission or readiness |
| `source_write_performed`, `runtime_write_performed` | Both false for discovery |

Hosts may ignore new fields and unrequested capabilities within protocol 1. Existing
IDs keep their meaning; a materially different operation requires a new ID or protocol.
The program version remains separate: pin the executable and its published checksum
for the command schemas you consume. This negotiation version does not retroactively
make every historical CLI result a new uniform envelope.

Repeat `--require` to check all prerequisites. An unavailable required capability
fails before project access. `CapabilityUnavailable.details.unknown` contains IDs
this build does not recognize; `details.unsupported` contains recognized but
unimplemented capabilities. Both lists are deterministic and deduplicated.
`ProtocolUnsupported.details` returns the requested and supported protocol versions.
Do not parse English messages to identify either condition.

## Preserve CLI outcomes

Use `--json` on operational commands. Successful JSON goes to stdout. Typed errors
go to stderr as `{ "code": "...", "message": "...", "details": ... }`; optional
details vary by error. Exit 0 means command success, 1 means a runtime/domain error,
and 2 means usage/argument error. Help and version flags intentionally return text.

A nonzero exit may accompany a useful partial stdout result, for example source
refresh failures. Preserve both streams and the exit code. A parseable stdout object
alone never means success or complete traversal. Timeouts or a lost pipe mean the
write outcome is unknown: inspect the existing proposal/attempt/execution/session
before retrying. Do not synthesize `source_write_performed: false` from a transport
failure. Input paths and maximum sizes remain command-specific (`--input`, reviewed
drafts, or documented lifecycle JSON on stdin); there is no universal JSON RPC wrapper.

## Current read/write limits

`source.read` and `object.read` operate on registered references with bounded body
reads. Status, ready and summaries do not provide a complete project catalog. Consult
`project.catalog` before relying on complete traversal. Queries that refresh source
projections can write the runtime database; `read_only` and freshness fields are part
of the result, not an inference from the command's name.

`mutation.yaml.record` supports selected YAML fields through source-bound proposals;
it reserializes the selected record. It preserves neither every comment nor its
original quoting/formatting. Require `mutation.yaml.lossless_fields` if the host
promises field-level preservation. Markdown adapters currently read sources; they do
not authorize Markdown body or ledger writes. Unsupported capabilities are explicitly
listed as unavailable, including human-save shortcuts, new tasks, multi-file writes
and user-confirmed completion. Never route an unsupported operation through a generic
status patch or a second writer.

`work complete` retains the engineering contract: a real session/claim and evidence
covering the source's acceptance criteria at the supplied source SHA. A source's raw
completed status is a separate assertion. Registering evidence does not run its
`command` field and does not itself verify the report.

`execution.external.register` stores an external reference with an unverified outcome.
It does not adopt a PID or create a managed supervisor. Native client resumption is
not implied by `session.resume`. Generic lifecycle callbacks can save checkpoints
without installing hooks; caller-provided summaries remain caller assertions. An
artifact import copies content, while references in evidence/events keep their own
documented read and verification rules.

## Compatibility and recovery

The schema declaration describes what this build can open, subject to ownership and
integrity checks. A read-only open requires the current schema. A write-capable open
may migrate eligible older schemas; discovery never does. A newer schema is rejected.
Back up matching program version, original sources, manifest and SQLite runtime
before an upgrade. Source projections are rebuildable; events and checkpoints are
not reconstructed by reindexing source documents. A binary downgrade alone is not a
database rollback procedure.

One project keeps one source ledger. When replacing an existing host writer, switch
only operations AWR actually supports, retain old runtime history as history, and
compare stable references and counts before cutover. Do not turn imported completion
claims into newly verified work. Host integration examples and local component checks
are separate from actual native-client or full application acceptance.
