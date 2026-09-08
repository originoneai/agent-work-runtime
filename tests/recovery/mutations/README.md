# Mutation concurrency and recovery checks

`contract.json` version 1.0.0 fixes 20 conditions for AWR-SEC-003. Run every condition
from the repository root, using a new report path to preserve prior failures:

```sh
python3 tests/recovery/mutations/verify.py --report .local/recovery-check-001.json
```

The runner executes the runtime matrix, builds the real CLI/MCP distribution
binaries and runs a separate CLI recovery test. Every stage must succeed and emit
its exact set of condition markers. Reports bind the contract, source inputs,
commands, logs and executable hashes. Old results never contribute to the count.

The Rust fixture is compiled only inside awr-runtime's test executable. Each case
owns a separate temporary project, two source files and a real SQLite database.
The writer helper runs as an ordinary OS child process. The parent waits for an
observed boundary, confirms that the writer is alive, kills it, waits for exit,
reopens the database and checks actual source bytes and durable receipts. Child
processes and temporary fixtures are cleaned up by their owners.

| Area | Conditions | Method |
| --- | ---: | --- |
| Concurrent writers | 3 | Real competing processes, source lock and transactional revision conflicts, disjoint-source retry. |
| Changed source/revision/mapping | 4 | Entry and final replacement checks preserve intervening edits. |
| Process interruption | 7 | Before journal, after journal, after temp sync, after rename, after directory sync, after projection, after finalization. |
| Recovery failures and receipts | 6 | I/O/access errors, injected SQL event failures, newer source, either damaged snapshot, real CLI recovery exactly once. |

The engine also checks explicit lock release while a duplicated file descriptor
remains open. This makes the inherited-descriptor failure deterministic without
turning it into a separate business scenario or inflating the contract total.

Two tests are deliberately `ignored` in a normal Cargo test run: the child fixture
requires its private fixture specification, and the CLI test requires a built CLI
path. The parent invokes the former explicitly; `verify.py` builds and invokes the
latter with `AWR_RECOVERY_CLI`. Neither environment variable is consumed by shipped
AWR executables. No production fault flag or alternate mutation implementation is
introduced.

The [user reference](../../../docs/reference/mutation-recovery.md) describes recovery
outcomes and limits. Test success establishes local component/process/CLI behavior;
it provides no E4, benchmark, cross-platform or release credit.
