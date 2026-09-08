# Events, claims and work branch ownership

AWR coordinates local work through sessions, claims, work branches and optimistic
project revisions. Source files remain authoritative and source ownership is
separate from runtime claim ownership.

## Event history

Public Store callers can append an observation or invoke a domain operation. They
cannot obtain its SQL connection or arbitrary transaction callback. Reserved event
types such as `work.completed` and `claim.expired` require their corresponding
domain operation; appending an observation cannot fabricate those receipts.

An event bound to a session inherits that session's work and branch. A conflicting
work, branch, session or project is rejected. An explicit CLI `--branch main` or
MCP `"branch": "main"` must agree with that session, just like a named branch.
Store and Runtime callers use `append_event_in_branch` when the draft branch is
an explicit selection, including `None` for main. The existing `append_event`
method permits session inference when the branch is unspecified. Updating a returned event value only
changes the caller's copy. Existing stored event metadata and bodies are protected
against UPDATE, DELETE, UPSERT and REPLACE on configured Store connections.
Recursive SQLite triggers are enabled for new, reopened, readonly and in-memory
snapshot connections so REPLACE's implicit deletion cannot bypass that protection.
This is a connection setting; no schema migration is required.

Runtime changes and their receipts commit together with one project revision
advance. A handoff or resume can append multiple linked history receipts at that
same revision. Failed operations retain the previous rows and revision.

## Claim lifetime

A claim belongs to one project, work item, session, agent label and work branch.
Only one effective claim can occupy a work item within a branch. Competing
processes first compare `expected_revision` in the write transaction; a contender
that refreshes its revision must still respect the existing claim.

An omitted TTL has no automatic expiration. A finite TTL expires when its deadline
is reached. Zero and overflowing TTLs are rejected without creating partial
sessions or claims. Acquiring an expired reservation records the expired claim
identity and the new owner atomically. Failure to append that receipt rolls the
whole acquisition back.

Inspect a claim through its session or Doctor before selecting an explicit repair:

```sh
awr --json session show SESSION_ID
awr --json doctor
awr --json doctor repair expire-claim CLAIM_ID \
  --expected-revision CURRENT_REVISION \
  --reason "The lease has elapsed"
```

Expiry repair rechecks the deadline inside the transaction. It refuses an
unexpired or indefinite claim. It does not select a different claim or end the
owning session. Claim release identifies the holding session; another session
cannot release it. Ending or explicitly interrupting a session affects only that
session's claims. Handoff and resume preserve a live claim's existing expiration
unless an explicit, supported acquisition or handoff TTL is requested; an expired
claim does not confer inherited ownership.

## Work branches

Switching the default work branch changes selection. It does not move existing
sessions, claims, checkpoints, evidence or history, and it does not change the Git
checkout. Distinct branches can retain independent claims for the same work item.
Source writes still use the shared-source conflict guards.

Implicit session selection uses the selected branch and reports ambiguity.
An explicit session ID selects that retained identity; supplied work and agent
filters must agree. Event queries support exact branch and session filters with
stable pagination. Shared source projections remain visible. Evidence from another
branch can be inspected as historical evidence and does not become current for
the selected branch.

Handoff requires the same work item and branch, while retaining the original
checkpoint owner. Resume requires the original branch to be current and creates
one successor. Closed branches reject new sessions and observations; retained
history remains readable.

The [isolation contract](../../tests/concurrency/contract.json) fixes 20 conditions;
the [runner](../../tests/concurrency/README.md) includes live competing OS processes,
API privacy checks and real CLI/MCP regression fixtures. These checks establish
local coordination and provenance, not authentication between local users,
tamper-proof database files, distributed locking, E4 or a release. Cross-platform
validation remains a separate V1 gate.
