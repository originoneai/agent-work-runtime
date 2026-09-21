# Workstream contract

This document specifies the workstream extension being implemented. It is not a
release announcement. A capability is available only when the running service
advertises it and enforces it at the relevant operation boundary. A source field
or a database column alone does not establish support.

The current domain module validates identity, ownership, scope selection and
explicit grants. Transport authorization, persistence and execution enforcement
are separate integration work; these domain rules do not enable isolated
workstreams in an existing CLI, MCP service or Team coordinator.

## Identity and authority

A project has one authoritative work graph and one or more workstreams. Each
work belongs to exactly one workstream. A goal explains intent, a milestone
groups stages, a work branch retains an execution alternative, and a Git
worktree isolates an editing directory; none substitutes for workstream identity.

Workstreams have immutable IDs, stable project-local keys, display titles,
explicit goal and acceptance-contract references, and lifecycle state. Renaming
a title does not change identity. Moving work requires an explicit transition
that accounts for claims, executions, dependencies and historical attribution.
Shared work is owned once and referenced by its consumers.

Source-first projects retain authoritative sources. Team source snapshots define
approved contracts while the coordinator owns execution facts. The extension
does not introduce a second ledger or require a server for personal use.

## Selection and authorization

Resolve scope from the exact work or persistent session. Explicit selectors
must agree with that binding. Without a bound work/session, a unique authorized
scope may be selected; multiple scopes require an explicit selection. Never
choose another client's most recently active scope. Conversation defaults are
client-scoped and cannot change existing session ownership.

All reads and writes enforce the same authorization, including search, counts,
event feeds, artifacts, recovery and caches. Dependency exports may expose an
authorized minimal contract and receipt without granting access to the
provider's private history. Query filtering is not an authorization boundary.

The default context contains the selected work, its relevant goals, session
checkpoint, applicable hard rules and required dependency proofs. Unrelated
active workstreams do not contribute their histories. Project-wide hard rules
and administrative barriers still apply.

## Concurrent operations

Ordinary writes validate their actual read set: authority, work and contract
versions, applicable policy, claim/fence, dependency bindings and resource
generations. Unrelated events can advance the project audit cursor without
invalidating an unchanged work. Repeated request IDs with the same intent return
the original receipt; changed intent conflicts. Unknown effects are inspected
before another dispatch.

Same-work ownership and overlapping resources remain exclusive. Source writers
use reviewed patches, source fingerprints and recovery journals. Multi-source
activation requires a coherent candidate and atomic activation; unsupported
adapters reject the operation. SQLite remains a single-writer database.

Project freeze, permission revocation and restore remain barriers. A restore
changes the coordinator epoch. Lease expiry does not prove process termination;
unknown executions retain resource protection. Worktrees separate local edits
but do not isolate a shared database, deployment destination or integration ref.
File confidentiality against an untrusted process requires a verified host
sandbox or OS boundary, not just AWR metadata.

## Cross-workstream dependencies

Edges connect specific works and required deliveries, not whole workstreams.
For example, `interface` in one stream may precede `sdk` in a second stream,
which precedes `integration` back in the first stream. The stream-level arrows
may return to the first stream while the task graph remains acyclic.

Required dependencies bind work identity, contract, artifact digest, acceptance
receipt, source/environment, adoption policy and export authorization. A source
status of completed is not verified delivery. Fixed accepted versions survive
unrelated upstream updates; current-contract dependencies revalidate when their
actual contract or delivery changes. Revocation is distinct from a newer version.

Check the complete required graph atomically, including concurrent graph edits.
Recheck relevant dependency proofs at preparation, dispatch and completion.
Only affected consumers need reevaluation; preserve historical receipts and
already observed external effects. References do not implicitly become hard
dependencies. Cross-project or cross-organization dependencies are outside this
version of the contract.

## Accounting and compatibility

Progress uses an explicit versioned required-work set, not the number of tasks
returned by a goal filter. Planning, implementation, acceptance, merge and
release are separate facts. Shared work is counted once.

Usage receipts identify the execution, work, owning stream at the time, provider,
model, pricing basis and observation coverage. Compression and request billing
must not be charged twice. Unknown usage is not zero. Shared costs require an
explicit allocation rule; allocations sum to the original charge. Parallel
wall time is distinct from summed execution time.

Legacy projects retain one compatible default scope and existing identities.
Existing project, work and session identifiers remain opaque strings, including
Team identifiers that are not ULIDs. Tenant adapters must obtain catalog and
permission records within the authenticated tenant before applying domain rules.
Legacy project-revision checks remain available. A client that cannot represent
scope must not silently write across newly isolated streams. Schema upgrades,
scope enablement and source migration require explicit, recoverable transitions.
Old dependency and ownership checks remain until replacement protocols pass
compatibility checks.

## Verification boundary

The synthetic acceptance matrix is
[`acceptance-matrix.json`](../../tests/fixtures/workstreams/acceptance-matrix.json).
It defines required counterexamples, not executed results. Domain tests,
SQLite/PG integration tests, native-client checks and complete business
acceptance remain distinct. Report actual evidence and missing coverage rather
than inferring availability or performance from this specification.
