# Native compaction and precise action guidance

AWR cooperates with native compaction. The host keeps it enabled and reports a
completed event. AWR records the measurements, evaluates the remaining context
occupancy, and returns one conditional instruction. It does not clear history,
create native windows, or approve a switch on the user's behalf.

This is a host observation protocol. An MCP connection alone cannot observe an
agent's context window or native compaction. A configured hook is not proof that
the host emits measurements or consumes its output. Test the actual host/version
before claiming automatic collection. The CLI, stdio MCP, shared HTTP MCP and
Python host example use the same domain implementation.

## Measurement contract

`post_compaction_percent = after_tokens / context_window_tokens * 100` is computed
only when `measurement_scope` is `full_request` and `measurement_basis` is
`host_reported`. `after_tokens` must be the request footprint immediately after
compaction, including system instructions and tool definitions. The denominator
is the host's effective capacity for that same model and request.

| Value | Meaning |
| --- | --- |
| `before_tokens`, `after_tokens` | Host measurements around this completed compaction, using the declared scope. |
| `context_window_tokens` | Effective host capacity; omit if only an advertised model maximum is known. |
| `duration_ms` | Duration of this compaction; optional. |
| `usage` | Optional input/output/cached-input tokens and actual reported `cost_usd` for this compaction. |
| `source` | Bounded host telemetry field or event locator; no raw conversation or secrets. |
| `measurement_scope` | `full_request`, `history_only`, or `unknown`. |
| `measurement_basis` | `host_reported`, `estimated`, or `unknown`. Estimates are retained but do not trigger threshold advice. |

Missing values stay null/absent. Summary length, cumulative session billing,
character counts, or AWR's context token estimate cannot substitute for actual
host occupancy. Cost is never inferred from occupancy. A host-reported charge is
an attributed assertion, not an independently audited bill. Cached input is a
subset of input usage, not additional occupied context.

The default `post_compaction_threshold_percent` is **50**, a configurable starting
heuristic, not an experimentally established break-even point. Hosts should send
their chosen policy with every observation. At or above it, an automatic
compaction becomes a handoff candidate; manual compactions are retained without
triggering this automatic policy. A single high observation is sufficient to
prepare a handoff, not to execute a switch. Compare successive percentages only
when model, effective capacity, trigger and usable measurement basis agree.

At a safe task boundary, preserve a checkpoint and estimate the complete restart
request: fixed instructions, tools, required work/rules and recovery material.
If it is smaller and work remains, present the user with the concrete checkpoint,
proposed continuation and expected context reduction. Obtain their decision
before opening a native session. A smaller restart request does not prove lower
billing or latency: cache behavior, recovery work and model output also matter.
If little work remains, the user defers, or the restart does not reduce context,
continue in the existing window. Do not create a window after every compaction.

The existing work ID survives handoff. After approval, the host explicitly uses
the existing checkpoint/resume flow and delivers freshly compiled required
context to the successor. Preserve the old session and evidence until recovery
has been checked. AWR does not control the host's sidebar or native session UI.

## CLI and host example

Create a JSON request using real host values. This is a **synthetic shape example**;
replace the session, revision, event identity and timestamp before use:

```json
{
  "session": "<bound-awr-session-id>",
  "expected_revision": 42,
  "observation": {
    "compaction_id": "native-compaction-7",
    "sequence": 7,
    "observed_at": 1789500000000,
    "trigger": "automatic",
    "model": "host-model-id",
    "source": "host.telemetry.completed-compaction-7",
    "measurement_scope": "full_request",
    "measurement_basis": "host_reported",
    "before_tokens": 230000,
    "after_tokens": 140000,
    "context_window_tokens": 256000
  },
  "policy": {"post_compaction_threshold_percent": 50}
}
```

```sh
awr --project /path/to/project --json session compaction observe --input observation.json
awr --project /path/to/project --json session compaction inspect --session SESSION
awr --project /path/to/project --json session compaction inspect --session SESSION --include-observation
awr --project /path/to/project --json session compaction defer --session SESSION \
  --observation-event-id EVENT --expected-revision REVISION
```

The Python example negotiates `client.compaction` and supplies the bound session
and last observed revision:

```python
# Invoke after the host reports a completed native compaction, not on a round timer.
result = workflow.observe_compaction(observation, policy={
    "post_compaction_threshold_percent": 50,
})
# Deliver result["guidance"] through a host-supported instruction/tool boundary.
# Once the user has postponed, stop repeating this observation's suggestion:
workflow.defer_compaction(result["observation_event_id"])
# For receipts or recovery, query without replaying the operation:
details = workflow.compaction(include_observation=True)
```

`workflow.py observe-compaction --input FILE`, `compaction
--include-observation`, and `defer-compaction --observation-event-id EVENT`
provide the same operations. The first file contains `observation` and optional
`policy`/`expected_revision`; the workflow supplies its own session.

Hosts using `awr client hook` can opt in on **PostCompact** by adding
`awr_compaction: {"observation": ..., "policy": ...}` to their normalized hook
input. Existing native inputs are unchanged. AWR returns the assessment in
`awr.compaction` and a bounded instruction in the returned context. These are
AWR's normalized fields, not a claim that any particular native hook supplies
them. If a host cannot consume PostCompact output, deliver the instruction at its
next supported tool/turn boundary. Missing telemetry does not fabricate an event
identity, sequence or measurements.

## Persistence, replay and shared MCP

The three tools are `awr_compaction_observe`, `awr_compaction_get`, and
`awr_compaction_defer`. They accept a bound `session` or `conversation`; shared
HTTP additionally requires the authorized `project` and a stable `request_id` for
writes. They retain existing client ownership and project isolation. Observation
and deferral are runtime records; their success does not certify source freshness.

Observations live in the existing append-only event journal. The tuple
project/session/`compaction_id` is stable: identical retries do not add an
observation; changed content or policy under that identity conflicts. Sequence
belongs to one native context bound to that AWR session; use the resume flow when
opening a successor native context instead of mixing two windows' measurements.
Sequence
must strictly increase, timestamp must not go backwards, and new writes require
the expected project revision. Sequence gaps are allowed; the sequence number
does not establish how many earlier events AWR received. Deferral applies only to
the latest observation in that session and expires when the next observation is
recorded. An old deferral retry cannot suppress a newer observation.

Normal replies contain a single assessment and at most one previous comparison,
not an accumulating history. `include_observation: true` returns current and
previous measurements/costs. Full history remains queryable with `awr work
history WORK --session SESSION --event-type client.compaction_observed` and its
existing event cursor. Domain event names cannot be forged through generic append.

If a write result is lost, query `awr_operation_get` and use
`awr_operation_recover` for a proven committed outcome. A replayed MCP result is
a historical receipt: query `awr_compaction_get` or prepare fresh work before
acting on its guidance. Nothing here automatically reruns an execution.

## One action, four fields

Prefer `awr work prepare WORK --session SESSION --response-view action`, or
`awr_work_prepare` with `response_view: "action"`. Each card has exactly:

```json
{
  "when": "Applicable current condition",
  "basis": "Observed facts or references that justify it",
  "next_action": "The next concrete action",
  "recheck": "The event that requires another assessment"
}
```

There is one card, capped at **1,024 serialized JSON bytes**. It is selected from
current state: inactive session/uncertain execution, user wait, required-context
gaps, work/claim blockers, compaction handoff candidate, management assessment,
then ordinary continuation. A task's own active claim is not treated as a
conflicting claim. Source diagnostics do not direct the agent to clean unrelated
ledger items. Instruction text is generated locally, without a model call.

The action view removes duplicate indexes, duplicate work provenance already
present in the context/work object, and optional maintenance prose. When exactly
equal, `context.completeness.source_versions` is omitted in favor of the retained
`context.work_context.identity.source_versions`. It retains
the rendered required context and exact hash, source versions, acceptance, hard
rules, required management actions, observation provenance, diagnostics, waits
and claims. It lists omissions and how to query full detail. The byte cap applies
to the card, not to indispensable context; AWR never truncates a hard requirement
to satisfy a display budget. Error and partial-write paths keep their existing
facts. `full` and `summary` contracts remain available and unchanged.

Hosts negotiate `workflow.action_guidance`. The Python preparation command
defaults to action and falls back to a supported summary/full read on older
binaries. Its Python method accepts `response_view="action"` explicitly. A fresh
management write requires one new preparation read so the returned instruction
matches the new assessment; ordinary repeated preparation remains one CLI call.
This change establishes bounded local guidance and observable continuity. It does
not establish unlimited model context, lower model fees or faster long tasks.
