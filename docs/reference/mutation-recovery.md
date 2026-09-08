# Recovering an interrupted source mutation

AWR binds each approved proposal to one source identity, configuration, revision,
fingerprint and target. It saves immutable before/after snapshots before recording
the application attempt. A successful response requires the intended source bytes,
their rebuilt projection and the final application event.

Use `awr --json doctor` and `awr --json proposal show PROPOSAL_ID` to inspect an
interrupted attempt. Read the current `project_revision`, then resume that same
proposal explicitly:

```sh
awr --json proposal recover PROPOSAL_ID \
  --actor reviewer \
  --reason "Resume the interrupted report review" \
  --expected-revision CURRENT_REVISION
```

Do not edit the snapshots to force a result. A current source matching neither
snapshot requires resolving the source conflict and preparing a new proposal.

| Persisted state after interruption | Recovery behavior |
| --- | --- |
| Snapshots exist, application journal absent | Source remains unchanged; a fresh `proposal apply` can begin. |
| Application journal exists, source still matches before | Revalidate ownership, configuration, source and runtime revision, then install the recorded after bytes. |
| Source already matches after, projection incomplete | Rebuild the projection without rewriting the file. |
| Projection committed, final event absent | Validate the existing projection and commit the final event without another file write. |
| Final event committed, process response lost | Inspect the durable receipt. Repeated recovery returns `InvalidTransition` and does not commit another event. |
| Source or snapshots disagree with the recorded plan | Preserve the source, report the conflict and withhold completion. |

`MutationIncomplete` with `write_outcome: pending_recovery` means the durable
attempt remains open. Storage, revision, I/O and source-access failures retain
this recovery path even when an earlier process already wrote the source. A
successful retry reports `recovered`, identifies the original attempt and binds
its `resolved_event_id` to the final event. `source_write_performed` describes
this invocation; it is not proof that a previous invocation never wrote.

Source projection invalidation is a separate event. A rejected projection
transaction can leave the previous facts explicitly stale; it cannot publish
partially updated facts. A failed final receipt transaction leaves the proposal
uncompleted and recoverable. Ordinary domain conflicts can stop an attempt;
work completion retains its stricter proof and recovery rules.

Concurrent AWR writers use a per-source OS lock and transactional
`expected_revision` checks. A writer explicitly unlocks when its application
scope ends; a temporary duplicate of its file descriptor cannot prolong that
reservation. Separate sources can progress, but an intervening project revision
can require an explicit recovery retry. External editors do not participate in
the advisory lock; fingerprint checks detect observed changes but do not make
the filesystem and SQLite one atomic transaction.

The [recovery contract](../../tests/recovery/mutations/contract.json) fixes 20
component/process/CLI conditions. The [runner](../../tests/recovery/mutations/README.md)
kills live writers at seven observed durability boundaries and uses the real CLI
to recover three of those states. These checks cover local process interruption,
not machine power loss, distributed writers, other operating systems, E4 or a
released product. Interrupted staging may retain unreferenced snapshot directories
or temporary files; this work does not add automatic garbage collection.
