# Concentrated regression

`contract.json` fixes the complete local gate for AWR-QA-001. It executes all
seven Cargo workspace members, all targets and doctests, five current specialist
contracts, all eight CLI/MCP tool parity checks, YAML intake, the copied manual
Codex lifecycle, the legacy Doctor fixture and four developer entrypoints.

Use the pinned Rust toolchain, `rtk`, and a Python interpreter with PyYAML. Commit
reviewed changes first; every report binds a clean source commit and the hashes of
all tracked inputs. From the repository root:

```sh
rtk proxy .venv/bin/python tests/regression/verify.py --output .local/regression-001
```

Choose a new output directory on every run. The runner retains failed logs and
continues independent gates. It never patches the source ledger or the actual
project database. Stateful and destructive cases operate on separate fixture
directories. Cargo builds/tests run serially across stages. A stage timeout kills
its isolated process group on POSIX systems and records failure.

`report.json` records the source SHA, environment, workspace target inventory,
every stage and its command/log hash, nested specialist receipts, binary hashes,
and unchanged-input check. Never use a report produced on different inputs as a
current pass. A new specialist contract version or ignored test requires updating
the regression contract and its execution route.

The four ignored entries have specific roles: three are private processes invoked
by their owning tests; the fourth is an explicit crash-to-CLI recovery check whose
runner supplies the real executable. `--ignored --list` audits their inventory
without accidentally launching unconfigured private fixtures.

Workspace test functions, doctests, parity functions and the 122 namespaced
specialist conditions have separate counts. Specialist suites repeat some
workspace functions; those repetitions do not increase the workspace total.
All Cargo examples compile; four isolated developer entrypoints execute, together
with YAML intake and the manual lifecycle. Other historical examples retain
compile coverage and the corresponding current CLI/domain tests.

This is local regression evidence. Cross-platform execution, performance metrics,
native client use, independent business review and the eight complete E4 scenarios
remain separate ledger work. The lifecycle script does not invoke a model or
activate automatic hooks. A passing regression report does not release V1.
