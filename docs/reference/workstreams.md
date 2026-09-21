# Workstream contract

This document specifies the workstream extension being implemented. It is not a
release announcement. A capability is available only when the running service
advertises it and enforces it at the relevant operation boundary. A source field
or a database column alone does not establish support.

The current domain module validates identity, ownership, scope selection and
explicit grants. Source import and SQLite projections retain the scope contract.
Session attribution, work-wide claims and reviewed ownership movement are wired
through the personal runtime. Explicit scoped read snapshots and registered-file
read APIs are available to trusted Rust integrations. Default context compilation,
transport authorization and resource/dependency
enforcement are separate integration work;
these rules do not enable isolated workstreams in an existing CLI, MCP service
or Team coordinator.

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

### Source projection in the development branch

`yaml-workstream-ledger-v1` is an explicit, read-only source adapter for one
complete primary ledger. The synthetic [ledger fixture](../../tests/fixtures/workstreams/ledger.yaml)
shows its `workstreams.version`, strict `definitions`, and one `workstream` key
per work. Definitions retain stable IDs, goal references and acceptance-contract
references. Contract references are identifiers, not evidence of acceptance.
Unknown versions, duplicate scopes, missing ownership and unresolved goal
references reject the candidate. Existing ledger adapters cannot silently consume
this declaration. Multi-source ownership activation is not supported yet.

The source fingerprint, catalog and complete ownership set commit together with
the ordinary ledger projection. Failed imports retain the previous projection
as stale; stale or retired authority cannot be read as current scope data.
Titles can change without changing scope identity. Authority changes require a
new authority version; established ownership changes require a separate runtime
migration. Retained scopes must be archived rather than removed from the source.

SQLite schema 5 gives legacy works a stable single-scope mapping without
renumbering works or changing sessions, claims, checkpoints, evidence or events.
The existing private-memory migration preview does not update the original
database. Applying the schema migration is transactional; a failed migration
can be retried after correcting its cause. Older binaries refuse a schema they
cannot represent. Source projection support alone does not claim runtime
isolation, dependency enforcement or a native-client acceptance result.

The synthetic acceptance matrix is
[`acceptance-matrix.json`](../../tests/fixtures/workstreams/acceptance-matrix.json).
It defines required counterexamples, not executed results. Domain tests,
SQLite/PG integration tests, native-client checks and complete business
acceptance remain distinct. Report actual evidence and missing coverage rather
than inferring availability or performance from this specification.

## Session attribution and reviewed movement

Schema 6 captures workstream and ownership revision when each session starts.
Claims and checkpoints retain that immutable session attribution. Starting a
work-bound session derives its scope from the work; a conflicting explicit scope
is rejected. Workless sessions need an explicit scope or a client/conversation
default when several scopes exist. Changing that default never rebinds existing
sessions, and a resumed session inherits its predecessor's scope.

Execution ownership is exclusive per work across work branches. Different works
can retain independent claims. Handoff and resume validate the current scope and
ownership generation before transferring execution rights. Paused or unavailable
authority still permits recovery checkpoints, release, session closure and an
unassigned handoff; it does not permit a new execution transfer.

The trusted Store API `commit_source_projection_with_moves` imports a reviewed
source candidate together with an exact move set. `workstream_ownership` provides
the current binding and revision for that review. The caller first invalidates
the edited source and supplies its current source/project revisions. An ordinary
reindex cannot silently move established work. Each move checks the previous
scope and ownership revision, requires an active destination, and rejects active
or otherwise unresolved sessions, effective claims, unfinished checkpoint saves
and nonterminal execution records. An external success report alone does not
establish a supervised terminal outcome. Candidate ownership, source fingerprint
and the move receipt commit atomically; failed imports retain the previous
projection marked stale.

Movement leaves historical sessions, checkpoints and claims in their original
scope. New sessions capture the new ownership generation; automatic recovery does
not import checkpoints from a previous generation, and old sessions cannot resume
execution after movement. This storage API does not itself edit files, authenticate
clients or implement cross-scope delivery adoption. Transport navigation and scoped
permission enforcement remain separate implementation stages.

Schema migration refuses simultaneous effective legacy claims for the same work
across branches. Resolve them through the previous runtime before retrying; the
migration never chooses a winner or deletes claims. An ambiguous historical
workless session retains an unknown scope instead of guessing. Preview and failed
migration leave the original schema and records unchanged.

## Scoped read snapshots

`Store::read_workstream` accepts current trusted `WorkstreamAccess` plus work,
session, explicit scope or conversation selectors. It resolves those selectors
against a coherent private-memory snapshot. Multiple authorized scopes without a
selector are rejected. A selector never grants permission. The returned
`WorkstreamRead` has no raw Store handle or mutation methods.

Catalog counts and pages, event feeds, artifact/evidence metadata, checkpoint
recovery and search use one derived visibility set. Filtering happens before
limits and counts. Search builds its FTS corpus from visible objects only, so
another scope's documents cannot affect BM25 scores or truncation. Scope-bound
cursors include the project, subject and authority version; they do not confer
access to an otherwise invisible object. The original database and its search
cache remain unchanged.

Goals use explicit workstream goal references. Plans and decisions need explicit
local or shared references; mixed or unknown private scope is withheld. Existing
rule scope expresses applicability, not confidentiality: the project rule source
remains shared policy, including hard and unknown rules. Whole-source catalogs
require the project administration interface. A dependency edge alone does not
authorize disclosure of the other workstream's private objects.

Session events keep immutable attribution. Work-only events use ownership at
their recorded revision. Runtime evidence follows its creation event; artifacts
and late artifact registrations follow their originating event. Moving work does
not move historical events, checkpoints, evidence or artifacts into the new
reader's scope. Recovery candidates are filtered before selecting the newest two.

`Runtime::read_artifact_in_workstream` and
`Runtime::read_evidence_report_in_workstream` authorize metadata before filesystem
access, retain path/size/digest checks, and recheck the binding before returning
content. Reading a report does not promote its evidence level.

The typed context readers use that same visibility set for goals, rules, current
work, accepted decisions and evidence associations. `dependency_closure` walks
only visible active tasks. A missing or inaccessible target produces an opaque
`unavailable_dependencies` entry containing the declaring task and an edge
reference; it reveals neither the target's identity/status nor its descendants.
Optional references are excluded when requesting required dependencies. Visible
cycles remain explicit, and an unavailable required dependency makes required
context incomplete. `awr_context::related_work_in_workstream` assembles these
scoped facts without access to the underlying Store. This is not cross-stream
versioned delivery adoption or execution admission.

Current-context session, checkpoint, execution and runtime-evidence readers also
check ownership history. Moving a task away and back does not make earlier
ownership generations current again. Historical receipts remain readable in
their original scope; ordinary work progress does not change ownership. Active
session selection applies scope before testing ambiguity. Legacy single-scope
dependency and related-fact responses keep their previous shape.

These are frozen read APIs, not reusable execution grants. Integrations must load
current authenticated policy for each request and revalidate authority at action
boundaries. Default context compilation, versioned semantic context identities,
and authenticated CLI/HTTP/MCP/Team integration are not yet connected to this
boundary. This implementation does not establish complete workstream isolation
or business acceptance.
