# Host workflows with less bookkeeping

This document is the **application-host** argv workflow. Coding agents that
speak MCP or call the CLI should start from
[host integration layers](README.md), not here.

The [host example](../../examples/host-app/README.md) keeps application-owned
workflow state separate from AWR's authoritative project sources. A host can
carry session identities, revisions and execution receipts so the agent spends
less effort assembling repeated arguments.

This integration provides three additions: shared work preparation and
conditional management records, optional concise operation results, and report
assembly from actual execution receipts. Context caching and delta delivery are
outside this change.

## Compatibility contract

- Existing work, source files, evidence and completion rules retain their meaning.
- Capability discovery selects supported operations before any write. A failed or
  uncertain write is inspected through its original receipt; it does not trigger
  a second write through a fallback path.
- Work preparation delivers the full required context. The caller explicitly
  acknowledges the exact delivered hash before saving a checkpoint or finishing.
  An acknowledgement is a caller assertion, not proof of model comprehension.
- Management observations come from the host. Unknown observations remain
  unknown, and continuous work never automatically downgrades to lightweight work.
- A concise result must retain decisions, blockers, errors and recovery facts.
  Full receipts remain available; returning fewer bytes cannot imply that fewer
  completion conditions apply.
- Execution receipts describe commands that actually ran. A successful exit code
  alone does not establish acceptance: reviewed checks still have to cover the
  current work contract, and AWR revalidates evidence at completion.

## Measuring the change

Separate first-time project onboarding, normal task execution and independent
verification probes in workflow reports. Keep the total alongside these subtotals
so apparent savings cannot come from removing initialization or negative checks
from the accounting. Measure lightweight and continuous workflows against the
same acceptance conditions. Tool-return bytes do not establish model-token or
billing savings.

## Prepare through the host

`Workflow.prepare(observation=None, goals=())` negotiates `workflow.prepare` and
`work.management` from the pinned program's capability catalog. It calls
`work prepare` when available and returns full context, readiness, continuity and
management facts together. Older programs use `context compile`; the result
explicitly reports that management observations were not recorded.

Provide an observation only after the host has checked those facts. Unknown
fields remain absent or null. With no new observation, a matching assessment is
left alone unless AWR requests a new record. Explicit new observations are always
recorded, including a newly discovered wait or additional executor. Each call
fetches fresh context and resets its acknowledgement, even when no new management
record is necessary.

```python
prepared = workflow.prepare(observation=observed_facts, goals=["G"])
context = prepared["context"]
# Deliver context to the agent and consume the returned rendered_context first.
workflow.acknowledge(context["work_context"]["context_hash"])
workflow.progress("Reviewed the required context", "Implement the change")
```

Lifecycle methods can use the workflow's last observed revision when
`expected_revision` is omitted. This removes repeated argument assembly, not
concurrency checks: another writer can still cause `RevisionConflict`. Inspect
and explicitly reconcile that result before proceeding. The original `context`,
`evidence` and `finish` entry points remain available.

## Optional concise results

For a single conditional instruction with explicit basis and recheck triggers,
use preparation's new `--response-view action` / `response_view: "action"`.
See [context continuity](context-continuity.md) for its byte budget, retained
requirements, capability negotiation and native-compaction host protocol.
The `full` and `summary` modes below retain their existing meaning.

Use `--response-view summary` with CLI `work prepare` or source work transitions
(including `work complete`). MCP `awr_work_prepare` and `awr_work_transition`
accept `response_view: "summary"`. The default is `full`. Hosts negotiate the CLI
capability `workflow.response_summary`; MCP clients inspect the tool schema.
`Workflow.prepare`, `progress` and `finish` accept `response_view="summary"`.

Preparation omits only `selected_chunks` and `selected_entities`, two indexes of
the returned packet. Rendered context, hash, identity, source versions,
completeness, omitted-chunk reasons, claims, readiness, waits and management
requirements are retained. Every preparation is a fresh query. To retrieve full
indexes, repeat the read in full mode and compare its revision and context hash;
that query may observe newer sources.

Successful transitions return changes, transition, target, intent, source outcome
and recovery information without repeating the full proposal patch and event
payload. The response lists its omitted fields and full-receipt lookups. CLI
clients can read the stored proposal and event with `--full`. MCP summary writes
require a stable `request_id`, including over stdio, so `awr_operation_get` can
return the original complete receipt within the same client/project scope.
Changing only the view of that same request reads its receipt rather than
executing the transition again. Domain arguments remain bound to the request.

Errors, incomplete context, partial writes and unknown results are returned in
full. Concise output grants no additional permission and cannot satisfy a missing
acceptance criterion. Source-change previews remain full: they must still be
reviewed before applying changes.

## Reports from actual managed execution

The optional Python host provides `run`, `collect_run`, `prepare_report`, and
`finish_report`. These call the existing native `execution run/inspect`, evidence,
and completion APIs. They do not add a supervisor, an MCP shell tool or an
unattended execution loop. The application explicitly supplies each command.

```python
workflow.run(
    key="verify-guide-1", purpose="Verify the guide examples",
    command=["/absolute/path/to/python3", "scripts/verify_guide.py"],
    source_paths=["scripts/verify_guide.py", "docs/guide.md"],
    artifact_paths=[".local/guide-check-1.json"],
)
observed = workflow.collect_run("verify-guide-1")
# If running/unknown: query later or inspect the original dispatch; never redispatch.
# Once eligible: read the actual logs/artifact and review each current criterion.
reviewed = workflow.prepare_report(
    "verify-guide-1", reviewer="guide reviewer",
    checks=[{"name": "Examples reviewed", "passed": True,
             "details": "Describe what was actually reviewed and the result",
             "criteria": ["The exact current acceptance criterion"]}],
    evidence_key="guide-check-1",
)
workflow.finish_report(reviewed["report"]["id"], "Reviewed the guide and execution evidence")
```

Use these four commands through `workflow.py` as well: `run --input RUN.json`,
`collect-run --key KEY`, `prepare-report --input REVIEW.json`, and
`finish-report --report-id ID --reason TEXT`. The input keys match the Python
arguments above. `finish-report` also accepts an expected revision and response
view. Existing `evidence` and `finish` interfaces remain unchanged.

Keep workflow state **inside the project**, in an ignored directory outside
registered sources. Before dispatch, the host saves explicit source file hashes,
command argv, work/session identity, consumed context hash and a unique native
operation key. The saved run key must retain this same intent. Calling it again
only queries the original execution. Without that original host state, source
provenance cannot be automatically reconstructed or attached to an older run.

Source and artifact lists are explicit, regular project files without symlinks or
path traversal. Output paths must be new; existing files cannot masquerade as a
fresh result. The report's `source_sha` is the SHA-256 digest of the canonical
project-relative source file hash map, **not a fabricated Git commit**. It includes
uncommitted source bytes within the declared scope. Select every file that matters
to the verification; the host does not prove that an incomplete caller-selected
scope is sufficient or that the run was hermetic.

Collection queries AWR's actual supervisor observation. Only a verified success
with exit zero, complete logs/result receipt, unchanged source bytes and present
artifacts can become eligible. It binds the collected file bytes once and rechecks
them during report preparation and before completion. It does not replace the old
collection with new hashes after a mismatch. The command, timestamps, native
execution identity and file hashes are copied into the report; checks and reviewer
identity are explicit caller assertions. Exit zero alone never generates a passing
acceptance check. AWR's preflight checks every current acceptance criterion, and
completion still runs the same domain gates.

A failed command remains inspectable; a running or unknown result cannot be
reported as complete. Dispatch response loss is recovered by querying the saved
operation key and explicitly reconciling, without running the command again.
Evidence response loss is resolved by checking the exact evidence/report binding.
Completion response loss is inspected before ending the existing session; it does
not repeat source completion. Lightweight and continuous tasks use this same
protocol. These are local execution and integrity checks, not independent business
acceptance or proof of model comprehension.
