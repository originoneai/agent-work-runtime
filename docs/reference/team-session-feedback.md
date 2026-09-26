# Team session feedback

Check `capabilities.session_feedback.version` before using these optional v1
fields. Team schema 34 stores feedback alongside existing sessions and checkpoints.
Old clients remain valid. The existing scoped command envelope, request replay,
session version, context hash and credential checks still apply.

## Client declaration

`session.start` and `session.checkpoint` accept optional `client_info`:

```json
{
  "product": "Example Agent",
  "version": "1.2.3",
  "model": {"id": "example-model", "provider": "example", "source": "host_metadata"},
  "capabilities": {"model": "supported", "usage": "unsupported", "progress": "supported"}
}
```

Omit unknown version/model fields. Each capability is `supported`, `unsupported`
or `unknown` (the default). Model source is `host_metadata` or
`client_configuration`; configuration describes the requested model and does not
prove provider routing. Do not guess model names. A supplied client declaration
replaces the previous declaration; omission preserves it.

These are **caller-declared metadata**, not an authentication method. Actor and
credential-client identity still come from the credential. Formal responsibility,
delegated executor and current claim remain separate. Metadata cannot grant
permission or satisfy an authorized model binding.

## Nonterminal progress

Batch `progress` with an ordinary `session.checkpoint`. Its existing `next_action`
and `open_loops` remain required. For example, add:

```json
{
  "phase": "testing",
  "summary": "Atomic writes implemented; durability checks are next.",
  "completed": ["Implemented atomic storage writes"],
  "blockers": [],
  "artifacts": [{"label": "Implementation", "reference": "src/store.rs"}],
  "tests": [{"name": "durability", "outcome": "not_run"}]
}
```

Phases: `starting`, `implementing`, `testing`, `waiting_user`, `blocked`,
`ready_for_review`. Phase is descriptive; it does not change work status, create
a user wait, renew a lease, settle execution or accept delivery. Use the existing
operations for those transitions. `execution.report` remains a terminal outcome
claim requiring reconciliation; do not use it as a progress heartbeat.

Summary is at most 2,048 UTF-8 bytes. Each list has at most eight entries:
completed/blocker text is at most 1,024 bytes; labels/test names 256 bytes;
references 1,024 bytes. Tests use `passed`, `failed` or `not_run`, with optional
`reference`. These are reported outcomes and references, not verified evidence.
No files or URLs are fetched automatically. Submit concise, redacted business
facts; raw logs, private prompts and credentials do not belong here.

## Optional host usage

Only submit counters actually exposed by the host and whose meaning is known:

```json
{
  "source": "native_host_event",
  "source_ref": "logs/example-session.jsonl#event-3",
  "counter_id": "host-session-counter-1",
  "scope": "host_session",
  "coverage": "partial",
  "observed_at_unix_ms": 1700000000000,
  "input_tokens": 4000,
  "output_tokens": 500,
  "cached_input_tokens": 1000
}
```

`usage` is a **cumulative snapshot of a named host session counter**, never a
delta, task total or bill. The host session may cover other work. AWR retains
snapshots but does not sum them, allocate them to tasks, estimate cost or invent
coverage. `coverage` is `complete`, `partial` or `unknown` for that host counter.
At least input or output is required; cached input is optional and a subset of
input. Integers must fit JavaScript's safe integer range. Source/counter labels
are at most 128 bytes, `source_ref` at most 1,024 bytes.

Duplicate requests replay their original receipt. Older observations and conflicting same-time samples are rejected across the
session, including counter resets. For the same source/counter, decreasing
counters are also rejected; a host counter reset needs a new `counter_id`. Future timestamps beyond
five minutes are rejected. Unknown host counter semantics mean omit `usage`;
raw events are not automatically added together.

## Read and freshness

`work.observe` exposes the selected session's client, model, latest structured
progress and latest usage under existing WorkRead permissions. Summaries are
shared with authorized work readers; raw execution receipts keep their stricter
ownership/reconciliation gate. Missing raw receipts say `permission_restricted`.

Each report includes its checkpoint ID, server report time, caller-declared
provenance and contract match. A later checkpoint without feedback preserves the
last report and its original timestamp. Usage freshness uses the host measurement
time, not ingestion time. A contract mismatch or age beyond 15 minutes marks the
report `stale`; this is an observation age, not proof the Agent stopped working.
The response's `observed_at_unix_ms` is query time, never report time.

Missing reasons distinguish `no_session`, `client_capability_unknown`,
`client_collection_unsupported` and `not_reported_by_client`. Old clients are
unknown-capability clients; absence is not silently labeled a reporting failure.
