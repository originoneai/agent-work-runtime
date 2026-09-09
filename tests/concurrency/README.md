# Event, claim and branch isolation matrix

`contract.json` version 1.1.0 fixes 20 component/process conditions for its isolated fixtures. Run the whole contract from the repository root with a new receipt:

```sh
python3 tests/concurrency/verify.py --report .local/isolation-check-001.json
```

| Area | Conditions | Verification |
| --- | ---: | --- |
| Events | 4 | Private SQL/API boundaries, immutable rows, reserved receipts, provenance and exact history queries. |
| Claims | 8 | Revision races, one owner, expired-claim races, effective-claim protection, selected expiry, release ownership, invalid TTL and transaction rollback. |
| Branches | 8 | Parallel ownership, default selection, session ambiguity, handoff, resume, cleanup, closure and checkpoint/evidence scope. |

The runner executes three primary stages and the related Store, CLI and MCP
regressions, then builds the distribution binaries and runs the three required
CLI/MCP branch-selector checks against those binaries. It records the contract and
input hashes, commands, logs, test totals and executable hashes. All stages must
pass and the observed condition IDs must exactly match the current contract.
Previous passes cannot fill missing conditions.

`branch_selectors.py` uses independent source projects and actual CLI/MCP stdio
processes. Explicit main and named branch selectors must agree with the bound
session; rejections must preserve all stored rows and source bytes. Matching
selectors and the existing omitted-selector behavior are also checked. Contract
1.1.0 requires this supplement for the event-provenance condition; the earlier
1.0.0 pass does not satisfy the current contract.

`event_guards.rs` is an internal unit test of SQL mutation forms on configured
Store connections, including read snapshots. Two compile-fail examples verify
that external callers cannot access the SQL handle or arbitrary transaction
callback; their expected compiler errors are checked by the runner.

`isolation.rs` exercises public Store domain methods in independent temporary
metadata projects. Its concurrency tests spawn distinct test processes, wait for
each live process to signal readiness, submit the same expected revision and
inspect actual committed sessions, claims and events. A losing process stays alive
and can retry using the refreshed revision. This distinguishes a version conflict
from a competing valid owner. TTL checks wait for the actual fixture deadline;
injected SQL failures are confined to fixture databases.

The private `fixture_contender` test is intentionally ignored by ordinary Cargo
runs. Its parent tests invoke it explicitly and supply a fixture marker and
specification. `AWR_ISOLATION_SPEC` is consumed only by that test executable;
shipped AWR binaries have no such hook. These child processes do not invoke models
or perform real agent collaboration.

The [user reference](../../docs/reference/runtime-isolation.md) describes operational
behavior. This matrix grants no E4, cross-platform, benchmark or release credit.
