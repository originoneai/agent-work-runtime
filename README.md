# Agent Work Runtime (AWR)

**Persistent work state and minimal context for long-running AI agents.**

AWR indexes project goals, plans, rules and task ledgers, tracks work across sessions, and compiles the context needed for the task happening now.

> Git versions code. AWR versions agent work.

## Project status

The Rust workspace, SQLite domain store, source adapters, incremental projection pipeline, dependency readiness, decision/evidence operations and source/work/search/session CLI are implemented. This repository uses AWR to index its own project sources and retain actual development sessions and checkpoints. **AWR is in development and has not been released.** See the [generated work index](ledger/README.md) for current progress and supported milestones.

The V1 plan contains **59 delivery work items**, **8 complete business acceptance scenarios**, and **10 benchmark targets**. Three repository preparation items are counted separately. A passing planning check validates the plan's structure; it does not prove runtime behavior.

## Product model

- **Intent:** goals, plans, rules and acceptance criteria.
- **State:** tasks, dependencies, blockers, ownership claims and work branches.
- **Memory:** decisions, events, evidence, checkpoints and artifact references.
- **Context:** deterministic, revision-bound context for a specific agent, task, branch and token budget.

Project files remain authoritative for project facts. SQLite holds their projections and is authoritative for AWR-generated runtime state. Historical events stay separate from current state. Hard rules and critical work facts remain intact during context selection.

## V1 direction

Local-first Rust binary, SQLite/WAL/FTS5, CLI and eight focused MCP tools. Feature development comes first, followed by agent integration, security hardening, system validation and release preparation.

The following task/context workflow is planned:

```sh
awr context bootstrap
awr context compile --work <work-id> --budget 4000
awr session resume
```

Bootstrap targets at most 1,000 tokens; work context targets at most 5,000 tokens, with 100% recall of required hard facts. Over-budget hard context must be reported explicitly.

## Design and implementation

| Document | Purpose |
| --- | --- |
| [Original design](docs/design/agent-work-runtime-design.md) | Unmodified product and engineering proposal |
| [Goals](docs/GOALS.md) | Intended outcomes and V1 scope |
| [Plan](docs/PLAN.md) | Feature-first delivery sequence |
| [Rules](docs/RULES.md) | Authority, evidence and development rules |
| [V1 contract](contracts/awr-v1.json) | Versioned scope, targets and acceptance matrix |
| [Work ledger](ledger/work-ledger.yaml) | Authoritative status, dependencies and acceptance per item |
| [Work index](ledger/README.md) | Generated readable view of the ledger |
| [Kickoff](docs/KICKOFF.md) | First implementation task and handoff instructions |

## Validate the planning baseline

Build the current CLI with Rust 1.93.1:

```sh
cargo build --locked -p awr-cli
target/debug/awr --help
target/debug/awr --version
```

Initialize and index this repository using its explicit source mapping:

```sh
target/debug/awr init --manifest examples/source-manifest/project.toml
target/debug/awr init --manifest examples/source-manifest/project.toml --accept
target/debug/awr source list
target/debug/awr source scan
target/debug/awr source reindex
```

Initialization previews the mapping before acceptance. An existing matching `.awr/project.toml` is reused without rewriting it; a conflicting mapping is rejected. Without `--manifest`, standard source paths are discovered, and ambiguous primary sources require an explicit mapping. Runtime databases, WAL files, artifacts and caches receive default Git ignore entries. A [small example](examples/basic/README.md) is included.

`source list` is read-only and shows the last observed state. `source scan` refreshes availability and flags pending changes; `source reindex` parses and commits those changes. `--json` provides structured reports and incomplete operations exit nonzero. Use read-only database diagnostics when needed:

```sh
target/debug/awr --json doctor --database path/to/awr.db
```

Read current work without opening the full ledger:

```sh
target/debug/awr status
target/debug/awr ready --limit 5
target/debug/awr work show AWR-P2-004
target/debug/awr --json work show AWR-P2-004
target/debug/awr search "依赖" --type work --limit 5
target/debug/awr search --type event --work AWR-P2-004 --status failed
```

These commands refresh source projections before querying; they update the rebuildable cache and leave authoritative files intact. Failed refreshes are reported and exit nonzero. Single-work JSON includes acceptance criteria, dependency diagnostics, source provenance and related decision/evidence summaries. Evidence currency is unknown unless `--source-sha <full-sha>` supplies a comparison; an event such as `test_passed` never promotes evidence or source-ledger status.

Search combines exact type/status/work filters with SQLite FTS5 and returns external IDs, bounded summaries, provenance and BM25 rank (lower is more relevant). Chinese character pairs support short Chinese task-name queries without embeddings. The revision-bound search cache uses an allowlist of titles and summary fields; goal/plan bodies, decision rationale, event payloads, artifacts and logs are excluded. Summary inputs retain one bounded line, omit code/control text and redact recognized credential markers. This is an initial indexing policy; the broader security review remains scheduled.

Run sessions and retain unfinished work:

```sh
awr --json status
awr session start --work <work-key> --agent <agent-id> --provider <provider> --model <model> --claim --ttl-ms 3600000 --expected-revision <revision>
awr session checkpoint --session <session-id> --context-hash <sha256-of-context> --digest "Progress so far" --next-action "Next step" --open-loop "Unfinished review" --expected-revision <revision>
awr session show <session-id>
awr session list --active
awr work history <work-key> --limit 20
awr work handoff <work-key> --session <session-id> --expected-revision <revision>
```

Each successful mutation returns its new project revision. Use that revision for the next mutation; a concurrent source/runtime change rejects stale writes. Start, claim and checkpoint refresh project sources before checking the supplied revision. A session can start without `--claim`; `work claim <work-key> --session <id> --expected-revision <revision>` acquires it later. Runtime claims never rewrite source-ledger ownership or work status.

`session end --session <id> --outcome ended|interrupted|incomplete --expected-revision <revision>` closes a session and releases its claims. `work release <work-key> --session <id> --claim <claim-id> --expected-revision <revision>` releases just one claim. Omitting `--session` selects only one active matching session on the current branch; `--agent` and `--work` can narrow the selection. Multiple candidates produce an error with their IDs.

Handoff requires a latest checkpoint. Without `--to-session`, it closes the sender as incomplete and releases its claim for later pickup. With `--to-session <id>`, the receiver must be active on the same work and branch; the live claim transfers atomically with both history receipts. An expired claim is never revived. `session show` exposes the inherited checkpoint separately, including next action and open loops. Context compilation and refreshed resume remain scheduled; a caller-supplied checkpoint hash does not certify context completeness.

Session inspection, history, handoff, release and end work from the runtime database even when source files are unavailable; JSON reports `source_refresh_performed: false`. History returns bounded summaries and references, with `--session`, `--event-type`, `--after-revision` and `--all-branches` filters. For pagination, pass the returned JSON `next_cursor` to `--cursor`; it retains multiple events at the same revision.

The storage library creates and reopens versioned AWR databases with WAL, foreign keys and migration metadata. Source work mutation and context commands remain scheduled.

The source library also resolves configured local files and immutable Git blobs. It has been used to read this project's own goal, plan, rules and work ledger:

```sh
cargo run --locked -p awr-source --example read_source -- . examples/source-manifest/project.toml 0
```

This entry point reports source metadata and fingerprints. The unified developer indexer projects YAML work items, milestones, dependencies and evidence references; Markdown goal/plan/rule sections; and ADR decisions:

```sh
mkdir -p .local
cargo run --locked -p awr-source --example reindex -- . examples/source-manifest/project.toml .local/awr.db
```

Unchanged sources are skipped. Content and parser-configuration changes trigger indexing; unchanged entity content retains its revision while source provenance is refreshed. Sources removed from a successfully scanned directory or the manifest become inactive, preserving historical references. Unavailable sources remain visibly stale/unavailable. Unknown rule metadata and raw statuses are reported explicitly.

Running the indexer against a new database rebuilds source projections. Runtime events, sessions and checkpoints require the original database or its backup; they cannot be reconstructed from a work ledger. Source indexing creates new indexing events, not copies of previous work history.

Context compilation follows in its ledger items. The current project's unannotated rules are retained with unresolved scope/severity until an explicit mapping is supplied for context use.

Python 3.11+ is sufficient for this repository's planning tools. Rust is pinned in rust-toolchain.toml and dependency resolution is committed in Cargo.lock.

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements-dev.txt
.venv/bin/python scripts/check_ledger.py
.venv/bin/python scripts/check_ledger.py --render
```

The checker verifies scope counts, task dependencies, design coverage, scenario isolation, evidence metadata and the generated index. It does not run AWR or replace independent acceptance review.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md) before beginning an implementation item.

## License

[Apache License 2.0](LICENSE). Copyright 2026 Agent Work Runtime contributors.
