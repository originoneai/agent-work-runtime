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

## Bind intake to the reviewed effects

`init --json` (including `--manifest` or a reviewed `--from-draft`) returns
`preview.fingerprint`, the source/configuration mapping, bounded source snapshots,
source issues, and a `writes` inventory with before/after fingerprints and exact
text for the manifest, `.gitignore` and generated intake files. Runtime database,
WAL and temporary staging effects are listed separately. This preview performs no
project writes; an explicitly requested `--write-draft` still writes its output file.

Pass that fingerprint as `init --accept --expected-preview <fingerprint>` with the
same input/mapping arguments. Changed sources, mapping semantics, existing manifest
bytes or ignore contents reject the old preview. Existing clients may omit the new
flag and retain the legacy behavior. A draft file's original inventory fingerprint
is still checked; editing a draft requires previewing that edited draft before
acceptance. Multiple source candidates remain an explicit ambiguity.

Repeated initialization keeps the existing manifest, project/work identities,
events, sessions and checkpoints. Non-Git projects are supported. Preview can read
a read-only project, but initialization requiring runtime writes is rejected there.
Source failures remain in the result: acceptance can create a partial index with a
nonzero `SourceStale` result, preserving usable source records. It must not be shown
as a fully successful import. Initializing multiple files is not a cross-file atomic
transaction; interrupted staging may require inspection before retry.

To change an initialized project's source mapping, provide a candidate manifest:

```text
awr source configure --manifest candidate.toml --json
awr source configure --manifest candidate.toml --accept --expected-preview <fingerprint> --json
awr source configure-status <fingerprint> --json
```

Configuration changes preserve project name, external key and authority mode.
They require a reviewed fingerprint, leave business source bytes untouched and
reindex the same project. Before/after manifests and a durable receipt are kept in
the ignored mutation directory; the preview fingerprint locates the receipt even
if the return pipe was lost. `configure-status` reads the recorded outcome and
compares current configuration fingerprints without applying or reindexing anything.
Pending, externally changed or partially indexed results are distinct from success.
A failed projection does not mean the configuration was unwritten; inspect the
receipt and current configuration before acting. Identical configuration returns
`no_change`. Configuration setup is separate from task/document source editing.

## Traverse the complete catalog

```text
awr object list work --limit 20 --json
awr object list work --cursor '<next_cursor JSON>' --json
awr object list source --scope all --json
```

Use `object list` for `goal`, `plan`, `rule`, `work`, `decision`, `source`,
`relation`, `artifact` and `evidence`. Each page contains at most 200 objects
(default 20), an exact `total` for the selected scope, `has_more`, and a nullable
`next_cursor`. Repeat the same kind and scope. The cursor binds the project identity,
project revision and last stable object ID. Any intervening source or runtime revision
requires a new traversal (`RevisionConflict`); do not append pages from different
versions. A changed title retains identity when the source's explicit key is stable.
Title-derived keys can change identity; AWR does not guess a rename from similarity.

Scope defaults to `active`. `retired` includes projections removed from a source or
whose source was unregistered; `all` combines both. These scopes describe retained
index membership, not completion, archive status or a reporting period. `active_total`
and `retired_total` remain separate. Select the desired milestone/relations for period
reporting; never infer engineering verification from `status: completed`.

Lists preserve source references, object/source/project revisions, raw status and
owner values alongside normalized status. Owner strings are source data, not runnable
Agent identities or runtime claims. Use `work show`/`ready` for readiness diagnostics
and current claims, and `object list relation` for dependencies and groupings. Unknown
states remain unknown; custom language or code-wrapped spellings use the project's
explicit `status_map`. Original bytes remain available through the source reader.

Long text is summarized and `content_included` is false. Drill down with
`object show <kind> <id> --full`, `decision show --full`, `source show --content`,
`artifact show`/`cat`, or `evidence show --content`, using their byte/version guards.
Source content reads require a current registration and the indexed fingerprint;
retired/revoked sources retain metadata and history without granting new file reads.
Explicit cached object reads expose historical projections, not fresh file content.

A failed source refresh can still return a useful page on stdout with a nonzero
`SourceStale` result. Its `total_basis` is `retained_indexed_objects` and
`total_is_current` is false; inspect `source_issues` and each source's freshness.
The retained count is not the authoritative total of an unreadable source. An empty
partial index must not be displayed as a confirmed empty project. Process stdout,
stderr and exit code together. Existing status/ready summary limits are unchanged.

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

## External clients, checkpoints and continuation

One work may have several AWR sessions and several native client conversations.
Keep the host's navigation key, the work's AWR ID/key, AWR session ID, native session
namespace and execution ID separate. Source `owner`, chosen Agent and actual session
Agent are separate facts. For a client without native hooks, call the generic
receiver explicitly; no hook installation or client-private history access is needed.

1. Start an AWR session with `session start` and the actual Agent/provider/model,
   or let `client bind --client generic --external-session <namespaced-native-id>
   --work <key>` create it. Pass `--session <id>` to bind an already created session.
   Binding does not claim the work; claim ownership remains an explicit domain action.
2. `client bind` returns the current context. `context compile --session <id>` can
   return a separately budgeted context, rendered body, hash and completeness result.
   Record actual delivery/receipt in the host; a hash alone proves no model received it.
3. If the host supervises the client, register only an external execution with
   `execution register --session <id> --key <operation-key> --purpose <purpose>
   --reference <host-execution-reference>`. AWR never adopts the PID, launches or
   terminates that client. `execution run` is for commands owned by AWR's supervisor.
4. Save useful progress with `client progress --client generic --external-session
   <native-id> --digest <observed-summary> --next-action <next> --open-loop <issue>`.
   Deliver a lifecycle callback using `client hook --client generic --work <key>`
   and JSON on stdin: `session_id`, `cwd`, `hook_event_name` and optional `turn_id`.
   Events are `SessionStart`, `PostCompact`, `PreCompact`, `Stop`, `SessionEnd`,
   `Interrupt`. Start/PostCompact return context; the others persist a checkpoint.
   Stable repeated callbacks deduplicate against source state and saved progress.
   Changed progress/source facts can legitimately produce a new checkpoint even with
   the same native turn ID. Wait for `awr.checkpoint_saved` before calling a save done.
5. After reopening the host, use `client show` to recover the binding and
   `recovery inspect --session <id>` to inspect the last completed checkpoint,
   pending runtime writes and external/managed execution observations. This does not
   refresh sources, release claims, create a successor, restart or stop a process.
   Combine it with `doctor --json` for read-only current-source/artifact diagnostics;
   preserve nonzero diagnostics. Investigate unfinished writes and unknown effects.
6. Explicitly continue with `session resume` at the current project revision, or
   bind a new native conversation with `client bind --from-session <old-id>`.
   Continuation refreshes sources and compiles new context before work proceeds;
   stale checkpoints do not authorize replay. Repeating a binding keeps its session.
   A raw resume that already created a successor must be inspected, not retried as a
   new continuation. Native resumption remains a separate, client-supported host action.

### Durable external reports

`execution report --input report.json --expected-revision <revision>` records a
version 1 `ExternalExecutionReport`. For example, a synthetic host might send:

```json
{
  "version": 1,
  "request_key": "guide-execution/stage-2",
  "execution_id": "<registered AWR execution ULID>",
  "host_id": "example-host",
  "host_work_key": "workspace/guide",
  "native_session": "provider/conversation-id",
  "agent_id": "guide-author",
  "origin": "host_observed",
  "phase": "waiting_user",
  "observed_at": 1,
  "summary": "The draft needs the user's choice of examples.",
  "detail_references": ["host://example/logs/guide"]
}
```

Use the actual ULID and observation time (Unix milliseconds). Supported phases are
`started`, `progress`, `waiting_user`, `succeeded`, `failed`, `interrupted`, `unknown`.
These are report classifications, not another work/execution state machine. `origin`
is `caller_reported` or `host_observed`; both are host-supplied provenance assertions.
Neither is authentication or an AWR-owned observation. Reports reject managed or
foreign-project executions and cannot overwrite their execution snapshot, create a
worker, verify business completion or change source work status.

The project-scoped `request_key` is immutable: identical content returns the same
event (even with the original revision after a lost response); different content
returns `SourceConflict`. Concurrent identical submissions make one receipt.
First-time stale revisions still fail. Use `execution report-status --key <key>`
before retrying a timeout; `found: false` means no retained receipt was found at that
read, not proof an in-flight writer cannot finish. Report lookup is read-only.

Reports remain recordable after the originating session ends or source parsing fails.
The file is capped at 1 MiB, identifiers at 512 bytes, summary at 8192 bytes, and
references at 32 entries of 4096 bytes each. References are retained, never opened,
copied or authenticated by this operation. Use the host's controlled detail viewer
for original logs. No token streams, client secrets or fabricated native IDs are needed.

The `execution.external_reported` event binds the work, original session, execution
and immutable report. Read its history with the existing event cursor and its full
body with `event show --full`. `execution show`/`inspect` return the latest report
separately; `recovery inspect` includes latest reports and project runtime findings.
An external host's reported `succeeded` still leaves AWR's observation `unknown` and
unverified. Interpret the host report using its actual supporting records before
retrying or completing work. Generic fixture checks and actual Kimi/Grok receiving
context are separate acceptance evidence.

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
