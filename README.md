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

L1 compilation and refreshed resume are planned:

```sh
awr context compile --work <work-id> --budget 4000
awr session resume
```

Bootstrap targets at most 1,000 tokens; work context targets at most 5,000 tokens, with 100% recall of required hard facts. Over-budget hard context must be reported explicitly.

L0 startup context is available now:

```sh
awr context bootstrap
awr context bootstrap --work <work-key>
awr --json context bootstrap --session <session-id> --budget 1000
```

Bootstrap refreshes sources and restores project, phase, work state, next action, blocker, applicable hard rules, source revisions/fingerprints and the last checkpoint. It selects an explicit session/work, one matching active session, or one source item marked claimed/in_progress. Ambiguous candidates require an explicit ID. A new session can see the latest checkpoint from a closed session on the same work and branch; the output labels that origin separately from its own checkpoint or a handoff. Unfinished loops and hard rule text are retained verbatim.

Unknown rule metadata, unavailable sources and missing work facts produce `CONTEXT INCOMPLETE` with references and a nonzero exit. If the complete selected payload cannot fit, `BudgetExceeded` reports the required count without emitting a silently truncated context. L0 restores orientation; it does not certify acceptance, dependencies or evidence for execution. Those checks belong to L1.

The initial budget counts `rendered_context` with the fixed [`o200k_base` ordinary-text tokenizer](https://docs.rs/tiktoken-rs/0.12.0/tiktoken_rs/struct.CoreBPE.html), treating special-token-looking strings as ordinary source text. The same state and request yield the same text and SHA256 context hash. JSON includes a diagnostic envelope outside that payload budget; surrounding conversation and other model tokenizers are not included. Clients should feed `rendered_context` as L0 content and account for their own transport/model overhead. The broader budget and completeness contracts continue in their M4 items.

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

The storage library creates and reopens versioned AWR databases with WAL, foreign keys and migration metadata. Source work mutation and L1/delta/resume context commands remain scheduled.

Inspect evidence, decisions and artifacts by external key or internal ID:

```sh
awr evidence add --input evidence.json --expected-revision <revision>
awr evidence show <evidence-id-or-key> --source-sha <full-source-sha>
awr evidence show <evidence-id-or-key> --content --max-bytes 262144
awr decision show <decision-id-or-key>
awr decision show <decision-id-or-key> --full --max-bytes 262144
awr artifact add report.json --type report --mime application/json --source-event <event-id> --expected-revision <revision>
awr artifact show <artifact-id>
awr artifact cat <artifact-id> --max-bytes 262144
```

`evidence.json` is a structured record, for example:

```json
{
  "external_key": "REPORT-1",
  "work_item_key": "WORK-1",
  "evidence_type": "report",
  "level": "implemented",
  "summary": "Implementation report; verification pending",
  "locator": "reports/implementation.json",
  "sha256": null,
  "source_sha": null,
  "command": null,
  "scope": ["WORK-1"],
  "branch_id": null,
  "verified_at": null
}
```

For verified evidence levels, the store requires a report SHA256, full source SHA, command, scope and verification timestamp (Unix milliseconds). These are supplied bindings: `evidence add` does not run the command or promote work status. JSON reports that distinction and `evidence show` reports missing bindings and currency relative to an explicit source SHA and branch.

Default reads return summaries and references. `--content`, `--full` and `artifact cat` explicitly request bodies, defaulting to 64 KiB with a maximum of 16 MiB per read. Oversized or changed content returns an error before emitting a body. Artifact reads always verify recorded size and SHA256; evidence report reads verify SHA256 when present and report whether it was available. Local paths must resolve inside the project or configured authorized roots. Remote report locators remain references. Plain artifact cat emits exact bytes; JSON body output requires UTF-8. Artifact metadata remains readable without opening its body or refreshing sources.

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

L1 compilation follows in its ledger items. This project's rule headings now explicitly declare project scope: five substantive sections are hard rules, and the document heading is informational. Their text is unchanged. Including all applicable hard sections can exceed the default L0 budget; use an explicit larger budget to inspect the complete payload. This is not a passed 1,000-token benchmark.

The context library also assembles an indivisible hard-fact subset with exact work IDs, raw/normalized status, acceptance, blocker, next action, rule text and source revisions/fingerprints. Rule selection covers project, concrete paths, tags, work and agent; it keeps unresolved applicability separate from exclusions and retains work-source tags. Directory/glob path scopes remain unknown unless the caller supplies concrete paths. Hard-context completeness concerns this subset only; dependency/evidence admission and branch overlays remain separate work items.

Related-context selection retains the complete required dependency closure, including unresolved work, missing keys and cycles. Completed dependency facts from stale sources remain unresolved. Applicable accepted decisions retain their exact statements and scope metadata; uncertain associations remain explicit references, while rationale and unrelated history stay out of the default selection. Evidence contributes bounded summaries, report references, source/branch comparisons and binding/verification gaps without opening report bodies. A missing report association is a known evidence gap, not proof that a task is complete. The three existing architectural ADRs now explicitly apply project-wide; their IDs, titles, statuses, statements and rationale are unchanged.

Recent Delta accepts an explicit checkpoint or project revision. Automatic selection uses the selected session's own/inherited checkpoint, then a closed session's checkpoint on the same work and branch; without one it reports a session-start or project-start baseline. A checkpoint from another work or branch is rejected. The developer entry point refreshes sources before reading:

```sh
cargo run --locked -p awr-context --example delta_project -- . <work-key> [checkpoint-id-or-revision]
```

Delta includes every changed source after the baseline, including project-wide mapping changes and retired sources. New source events record before/after source states and entity/edge additions, revisions and removals in the same database transaction. Fingerprint-only refreshes retain unchanged entity revisions. Older events remain immutable and explicitly report unavailable entity history; the reader does not infer historical changes from today's facts. Source event names are reserved for source operations.

Process history includes this work and project-global events on the selected branch. High/critical events after the baseline return at most 12 bounded summaries by default, with critical events first and then newest revision/time/ID. Per-source entity summaries retain 24 changed identities by default; all omitted counts are explicit. Normal and older process events are folded into counts by type/importance. Full payloads remain available via `Store::event` and paginated `EventQuery`, using the returned event IDs and baseline/current revisions; context CLI integration follows in its ledger item. No event payload, artifact body or source fact body enters this delta. The snapshot API itself does not rescan files or certify execution-context completeness.

The budget library preserves required chunks whole. It tries optional chunks by ascending priority, descending recency, then section/key; an oversized candidate is skipped so a later smaller candidate can still fit. Final rendering uses fixed section/key order. Each trial counts the entire rendered string with the pinned `o200k_base` ordinary-text tokenizer, including headings, IDs, source versions/fingerprints and the omission footer. The count is exact for that tokenizer; transport JSON, tool framing and the surrounding conversation are excluded. Other tokenizers may differ, with no cross-tokenizer error bound claimed. This is a counting algorithm, not a measured compression/performance benchmark.

If the required text plus metadata exceeds the supplied budget, the result is `BUDGET_EXCEEDED` with the required count; it does not truncate acceptance, rules, status, blocker or next action. The SHA256 binds the canonical project/work revisions, branch, source versions, request, budget policy, selected entity IDs/revisions, selected/omitted chunk references and rendered content. Input ordering of source/chunk sets is normalized, and conflicting versions are rejected. The hard-subset preview below exercises this budgeter; full L1 assembly will additionally supply dependencies, goals, decisions and completeness information:

```sh
cargo run --locked -p awr-context --example budget_project -- . <work-key> <budget> [agent-id]
```

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
