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

L1 work-context compilation is available:

```sh
awr context compile --work <work-key> --budget 5000
```

Session resume refreshes sources and compiles current context for the receiving agent:

```sh
awr session resume --from-session <session-id> --agent <agent-id> --provider <provider> --model <model> --expected-revision <revision>
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

The budget counts `rendered_context` with the fixed [`o200k_base` ordinary-text tokenizer](https://docs.rs/tiktoken-rs/0.12.0/tiktoken_rs/struct.CoreBPE.html), treating special-token-looking strings as ordinary source text. The same state and request yield the same text and SHA256 context hash. JSON includes a diagnostic envelope outside that payload budget; surrounding conversation and other model tokenizers are not included. Clients should feed `rendered_context` as context content and account for their own transport/model overhead. L1 combines this budget contract with required-fact completeness.

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
target/debug/awr --json doctor --database path/to/awr.db --database-only
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

Checkpoint saving now records the session's actual event references and observed source changes automatically. The saved window starts after its previous successful checkpoint, or at session start, and ends at the caller's refreshed project revision. Process summaries belong to that exact session; project-wide source changes are labeled observations and do not assert authorship. Source receipts retain before/after states and structural entity changes, including explicit unknown details for legacy receipts. Artifact/evidence changes require domain-created receipts. Arbitrary event payloads and artifact/source bodies stay outside the saved delta.

The caller still supplies the digest, next action, open loops and hash of its last-used context. Hash syntax is checked; checkpoint saving does not independently verify that context, and JSON reports `context_hash_verified: false`. Additional `--changed-entity` entries are retained as caller-reported references separately from automatically observed changes. In new saves, `checkpoint.changed_entities` contains observed identities. The older single-transaction store API remains available for compatibility; its checkpoints explicitly have no recorded delta.

A save first commits `checkpoint.started`, then atomically commits the delta, checkpoint, latest-checkpoint pointer and `checkpoint.created` receipt. A successful CLI save advances the project revision twice; the checkpoint's base revision remains the pre-save snapshot. If the process stops or the final transaction fails, the prior checkpoint remains active. `session show <id>` reports incomplete attempts as `pending_or_interrupted`, without treating them as recovery points; the label also covers a save currently in flight. Use `event show <attempt-id> --full` to inspect its retained draft. An intervening project change invalidates that attempt and requires a fresh save with the current revision. After confirming the writer has stopped, an explicit Doctor repair can mark an unfinished attempt `abandoned`, retaining its draft and excluding it from the incomplete count. It cannot be completed afterward and never becomes a recovery checkpoint.

Default session/checkpoint queries expose save counts and receipt references. `object show checkpoint <id> --full` explicitly retrieves the immutable `session_delta`, subject to the same read cap as the checkpoint body. Event history can page older attempts using `--session <id> --event-type checkpoint.started`. Full history is persisted for recovery and inspection without automatically putting it into L0/L1 context.

`session end --session <id> --outcome ended|interrupted|incomplete --expected-revision <revision>` closes a session and releases its claims. `work release <work-key> --session <id> --claim <claim-id> --expected-revision <revision>` releases just one claim. Omitting `--session` selects only one active matching session on the current branch; `--agent` and `--work` can narrow the selection. Multiple candidates produce an error with their IDs.

Handoff requires a latest checkpoint. Without `--to-session`, it closes the sender as incomplete and releases its claim for later pickup. With `--to-session <id>`, the receiver must be active on the same work and branch; the live claim transfers atomically with both history receipts. An expired claim is never revived. `session show` exposes the inherited checkpoint separately, including next action and open loops. A caller-supplied checkpoint hash does not certify context completeness.

Resume creates a new session with the requested agent/provider/model. Use `--from-session <id>` for an explicit predecessor, or `--work <key>` to select its active session or most recent recoverable closed session on the current branch. Without either selector, it selects an active or interrupted/incomplete session for nonterminal work; competing active sessions require an explicit predecessor. A predecessor can have one resume successor, exposed by `session show`; continue that successor after a retry instead of creating parallel descendants.

The command refreshes sources and checks `--expected-revision` before compiling target-agent context. It retains checkpoint next action/open loops, current hard rules and source changes after the checkpoint. The checkpoint is inherited by reference, retaining its original owner and saved delta. If no successful checkpoint exists, recovery uses current sources and changes since session start and explicitly reports that unrecorded memory is unavailable. That recovery revision is retained separately from the successor's actual start, so ordinary context reads, retries and further resumes keep the earlier source-change window. Incomplete saves are never recovery points. Provider/model fields record the receiving environment; AWR does not invoke or configure that provider.

By default, resume transfers only a still-live predecessor claim with its exact expiration time. `--claim` acquires a fresh claim under the normal readiness/conflict checks, optionally with `--ttl-ms`; `--no-claim` skips ownership. An expired or previously released claim is not revived automatically. Closing an active predecessor, creating its successor, inheriting the checkpoint and handling claims commit in one transaction. A claim conflict rolls back the entire transition.

Use `--budget` (default 5,000), repeated `--path`, `--tag`, `--goal`, and `--source-sha` to describe the execution context. Unknown rule scope or incomplete preflight returns diagnostics without creating a successor. The final context is compiled again for the committed successor. If that compilation fails or exceeds its budget, JSON retains `resumed.session.id`, `context_phase: "resumed_session"`, and the error with `context_ready: false`; the command exits nonzero. Compile context for that existing session with corrected inputs rather than repeating resume. Source refresh can advance the project revision even when resume does not create a session.

Session inspection, history, handoff, release and end work from the runtime database even when source files are unavailable; JSON reports `source_refresh_performed: false`. History returns bounded summaries and references, with `--session`, `--event-type`, `--after-revision` and `--all-branches` filters. For pagination, pass the returned JSON `next_cursor` to `--cursor`; it retains multiple events at the same revision.

The storage library creates and reopens versioned AWR databases with WAL, foreign keys and migration metadata. Source work mutation remains scheduled.

Diagnose interruptions and retained state:

```sh
awr --json doctor
awr doctor --database-only
awr doctor repair expire-claim <claim-id> --expected-revision <revision> --reason "The lease has elapsed"
awr doctor repair interrupt-session <session-id> --expected-revision <revision> --reason "The writer stopped; preserve its checkpoint"
awr doctor repair abandon-checkpoint <attempt-event-id> --expected-revision <revision> --reason "The interrupted save will not be retried"
awr doctor repair clear-invalid-branch <branch-id> --expected-revision <revision> --reason "The selected branch is no longer valid"
```

Default Doctor is read only. It combines SQLite/schema checks with expired claims, inconsistent/orphan sessions, incomplete checkpoints, invalid branches, retained mutation state, source access/configuration/fingerprints, and registered/managed artifact checks. Source reads compare current files against the cache without reindexing or marking retained freshness; directory additions/removals are visible. Registered local artifact files are checked against their size and SHA256. Content is never emitted; source reads are capped at 16 MiB each, and artifact hashing defaults to 16 MiB per file (`--max-bytes`, up to 1 GiB). Files outside current authorized roots are not read. Unregistered managed files may belong to an import in flight, so they are reported and preserved.

`ok` is false, with a nonzero exit, when warning/error findings remain; `database_ok` separately reports SQLite/schema health. Normal active sessions are informational because AWR has no process-liveness proof. A missing manifest or unavailable source does not hide database/runtime findings. `--database-only` retains the original database inspection scope and does not certify project/file health. Concurrent project revision changes are explicit diagnostics.

Each repair targets one object and requires a current project revision and a nonempty reason. Expiration applies only to an elapsed active lease; interruption explicitly closes one active session and releases its claims; abandonment closes only an unfinished save; invalid-branch repair clears only the invalid current pointer and preserves the branch records. The condition is rechecked inside the transaction, with an immutable receipt and rollback on failure. Repairs open an existing current-schema database without creating, migrating or changing its journal mode. They do not refresh or rewrite project sources, delete artifact files, infer that a live writer stopped, complete pending mutations or repair every reported problem. A successful repair reports only that selected change; rerun Doctor to inspect remaining findings.

Create and review a source mutation proposal:

```sh
awr status
awr proposal create --kind work --target <work-key> --intent "Continue reviewing the report" --patch '{"next_action":"Review the revised report"}' --session <session-id> --expected-revision <revision>
awr proposal list --status draft
awr proposal show <proposal-id> --full
awr proposal submit <proposal-id> --actor <reviewer> --reason "Ready for review" --expected-revision <current-revision>
awr proposal approve <proposal-id> --actor <reviewer> --reason "Reviewed the exact proposed fields" --expected-revision <current-revision>
awr proposal apply <proposal-id> --actor <reviewer> --reason "Request application" --expected-revision <current-revision>
awr proposal recover <proposal-id> --actor <reviewer> --reason "Resume an interrupted application" --expected-revision <current-revision>
awr proposal reject <proposal-id> --actor <reviewer> --reason "Superseded by a new proposal" --expected-revision <current-revision>
```

A proposal binds one source-backed goal, plan, rule, work item, decision or evidence record. Its immutable envelope contains the target ID/key/revision and exact SourceRef, source configuration, base fingerprint, creation project revision, intent and proposed field replacements. `--patch-file` accepts the same JSON object from a file; patches are capped at 64 KiB. Identity and source-binding fields cannot be replaced. Runtime-only records cannot be source-mutation targets. Source evidence retains its own work association; a creating session cannot assign it to different work. Lifecycle receipts retain the creating session's branch, while reviewer identity is recorded separately.

`draft → ready → approved` records creation, submission and review separately. Each action requires the current project revision and appends a receipt atomically with the proposal state. `expected_revision` inside the proposal remains its creation baseline. Creating, submitting and approving recheck the actual manifest mapping and file/Git snapshot without implicitly reindexing. Application rechecks the same binding before writing. Drift requires a new proposal after explicit reindexing; an existing proposal becomes `conflict`. A failed source read becomes `failed`. These states and `rejected` are terminal. Rejecting an open proposal remains possible without its source or manifest, except while its application is unfinished. Reviewer names are caller-reported identities, not authentication or independent acceptance evidence.

**Approved local YAML proposals can be applied.** The writer replaces the exact source record, preserves bytes outside that record, saves through a temporary file and atomic rename, reindexes the source, and verifies the resulting target before recording `applied`. It retains the original and planned bytes in `.awr/mutations/<write-plan-id>/`, bound to an immutable `proposal.apply_started` event. Supported fields and formatting boundaries are documented in the [YAML adapter](adapters/yaml-ledger/README.md). Markdown, Git snapshots and unsupported YAML targets retain `approved`, record `proposal.required`, and return `proposal_required` with a nonzero exit. Generic field proposals cannot change work status, ownership, blockers, verification or evidence membership; work domain actions and completion gates remain separate delivery items.

An interrupted application stays `approved` with an unresolved apply attempt. `proposal show` includes the attempt and recovery location derivable from its plan ID. `MutationIncomplete` reports a pending recovery and returns a nonzero exit. After fixing the reported storage problem, `proposal recover` compares the actual source with both recorded fingerprints: original bytes can be written; planned bytes can be reindexed/finalized without a second write; any other bytes produce a conflict without overwriting the current file. Ordinary apply and rejection cannot replace an unfinished attempt. Recovery snapshots are retained after success or failure. `source_write_performed` describes this invocation (`null` means not established); `source_refresh_performed` records whether this invocation rebuilt the projection. A failed start receipt leaves the source untouched and may retain unreferenced staging snapshots.

`proposal list` and `proposal show` read retained state without source access; summaries omit field values and `show --full` returns the complete reviewable patch. Failed/conflicting actions print their durable transition receipt when one was recorded, before returning a nonzero exit. Receipts include the new project revision for the next action. AWR serializes its own writers per source and repeats the fingerprint check immediately before replacement and after reindexing. Filesystem rename does not provide a compare-and-swap against unrelated editors; avoid simultaneous external saves to the same source during application.

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

This project's rule headings explicitly declare project scope: five substantive sections are hard rules, and the document heading is informational. Their text is unchanged. Including all applicable hard sections can exceed the default L0 budget; use an explicit larger budget to inspect the complete payload. This is not a passed 1,000-token benchmark.

The context library also assembles an indivisible hard-fact subset with exact work IDs, raw/normalized status, acceptance, blocker, next action, rule text and source revisions/fingerprints. Rule selection covers project, concrete paths, tags, work and agent; it keeps unresolved applicability separate from exclusions and retains work-source tags. Directory/glob path scopes remain unknown unless the caller supplies concrete paths. Hard-context completeness concerns this subset only; dependency/evidence admission and branch overlays remain separate work items.

Related-context selection retains the complete required dependency closure, including unresolved work, missing keys and cycles. Completed dependency facts from stale sources remain unresolved. Applicable accepted decisions retain their exact statements and scope metadata; uncertain associations remain explicit references, while rationale and unrelated history stay out of the default selection. Evidence contributes bounded summaries, report references, source/branch comparisons and binding/verification gaps without opening report bodies. A missing report association is a known evidence gap, not proof that a task is complete. The three existing architectural ADRs now explicitly apply project-wide; their IDs, titles, statuses, statements and rationale are unchanged.

Recent Delta accepts an explicit checkpoint or project revision. Automatic selection uses the selected session's own/inherited checkpoint, then a closed session's checkpoint on the same work and branch; without one it reports a session-start or project-start baseline. A checkpoint from another work or branch is rejected. The CLI refreshes sources and shares L1 work/session selection:

```sh
awr --json context delta --work <work-key>
awr --json context delta --session <session-id> --checkpoint <checkpoint-id>
awr --json context delta --work <work-key> --after-revision <revision> --event-limit 12 --entity-limit 24
```

Delta includes every changed source after the baseline, including project-wide mapping changes and retired sources. New source events record before/after source states and entity/edge additions, revisions and removals in the same database transaction. Fingerprint-only refreshes retain unchanged entity revisions. Older events remain immutable and explicitly report unavailable entity history; the reader does not infer historical changes from today's facts. Source event names are reserved for source operations.

Process history includes this work and project-global events on the selected branch. High/critical events after the baseline return at most 12 bounded summaries by default, with critical events first and then newest revision/time/ID. Per-source entity summaries retain 24 changed identities by default; all omitted counts are explicit. Normal and older process events are folded into counts by type/importance. Full payloads are available through `event show --full` and paginated history, using the returned IDs and baseline/current revisions. No event payload, artifact body or source fact body enters this delta. Source refresh failures remain structured in `source_issues` and exit nonzero; historical gaps remain labeled in `delta.gaps`. Neither the CLI nor the snapshot library API certifies execution-context completeness.

The budget library preserves required chunks whole. It tries optional chunks by ascending priority, descending recency, then section/key; an oversized candidate is skipped so a later smaller candidate can still fit. Final rendering uses fixed section/key order. Each trial counts the entire rendered string with the pinned `o200k_base` ordinary-text tokenizer, including headings, IDs, source versions/fingerprints and the omission footer. The count is exact for that tokenizer; transport JSON, tool framing and the surrounding conversation are excluded. Other tokenizers may differ, with no cross-tokenizer error bound claimed. This is a counting algorithm, not a measured compression/performance benchmark.

If the required text plus metadata exceeds the supplied budget, the result is `BudgetExceeded` with the required count; it does not truncate acceptance, rules, status, blocker or next action. The SHA256 binds the canonical project/work revisions, branch, source versions, request, budget policy, selected entity IDs/revisions, selected/omitted chunk references and rendered content. Input ordering of source/chunk sets is normalized, and conflicting versions are rejected. The hard-subset preview below exercises this budgeter; L1 assembly additionally supplies dependencies, goals, decisions and completeness information:

```sh
cargo run --locked -p awr-context --example budget_project -- . <work-key> <budget> [agent-id]
```

Completeness assessment refreshes the configured sources, including newly configured paths that have no cached Source row. It reports `source_fresh`, `work_item_found`, `work_state_complete`, `acceptance_complete`, `rules_complete`, `dependencies_complete`, `decision_context_complete`, evidence gaps and reasons. Required missing/unknown/stale facts produce `CONTEXT INCOMPLETE`; a missing task returns the same structured report. A snapshot-only call explicitly reports that freshness was not checked and cannot return a current complete verdict. A report from a different project/revision is rejected.

`dependencies_complete` means the required graph and its facts are known, current and free of missing links/cycles. Unfinished dependencies with known status, next action and blocked reason remain listed separately. `rules_complete` covers all possibly applicable hard rules. A new task can have complete context while reporting absent or unverified evidence. These checks assess fact availability and associations; they do not independently evaluate acceptance quality, run report commands, promote task status or certify release readiness.

```sh
cargo run --locked -p awr-context --example completeness_project -- . <work-key> [agent-id] [source-sha]
```

The example prints the machine-readable report and exits nonzero on incomplete context. Goal selection is assessed separately by the L1 compiler and appears as `goal_context_complete`; the standalone assessor leaves that field unset.

Compile the full current work context:

```sh
awr context compile
awr context compile --work <work-key> --session <session-id> --budget 5000
awr --json context compile --work <work-key> --agent <agent-id> --path src/main.rs --tag rust
awr context compile --work <work-key> --goal <goal-key> --source-sha <full-source-sha>
awr context compile --work <work-key> --checkpoint <checkpoint-id>
awr context compile --work <work-key> --after-revision <project-revision>
```

Task selection uses explicit work/session, one matching active session, then one source work marked claimed/in_progress. Ambiguous candidates fail before producing a pack; an explicitly missing task returns incomplete diagnostics with `work_context: null` and no invented work identity/hash. Paths, tags and goals may be repeated. A requested session must match the agent, work and current branch. `--checkpoint` and `--after-revision` are alternatives. `--intent` labels the request and participates in hashing; it does not grant mutation permissions.

The compiler refreshes sources, resolves work, selects required dependencies, evaluates rules, selects accepted decisions, reads checkpoint delta, selects evidence, assesses completeness and applies the deterministic budget. Goal context defaults to nonterminal sections from primary project goal sources, labeled as project-level goals; `--goal` selects explicit keys. Unknown/missing goal status remains incomplete. This project's primary goal mapping now explicitly declares its ongoing goals active; goal IDs, titles, bodies and document fingerprints are unchanged.

Required output retains exact task state/acceptance, applicable hard rules with severity/scope, unresolved required dependencies, accepted decision statements, checkpoint next action/open loops, source-change summaries and all completeness/evidence gaps. Selected critical event summaries are required. Goal/work prose, resolved dependencies, high event details, soft/info rules, evidence summaries and ordinary history compete as whole optional chunks. Original event/rationale/report/artifact bodies are not expanded.

Plain output is the selected rendered text. JSON puts that same text and its hash/token metadata in `work_context`, with `completeness` alongside it. `work_context.omitted_chunks` lists budget omissions; `omitted_refs` records limits applied during earlier selection. An incomplete result retains available facts and exits nonzero. Hard overflow returns an error before any pack is emitted. The payload budget applies to the rendered text, and this functional behavior is not a passed system benchmark or release gate.

Follow a reference without expanding the whole project:

```sh
awr --json object show goal <id-or-key>
awr --json object show goal <id-or-key> --full --entity-revision <revision> --max-bytes 262144
awr object show rule <id-or-key> --full
awr object show plan <id-or-key> --full
awr object show work <id-or-key> --full
awr object show checkpoint <checkpoint-id> --full
awr --json source show <source-id-or-domain>
awr --json source show <source-id> --content --source-revision <revision> --fingerprint <fingerprint> --max-bytes 262144
awr --json source show <source-id> --content --start-line 10 --end-line 20
awr --json source history <source-id> --after-revision <baseline> --through-revision <context-revision> --limit 20
awr --json event history --work <work-key> --session <session-id> --through-revision <context-revision>
awr --json event show <event-id>
awr --json event show <event-id> --full --max-bytes 262144
```

`omitted_chunks[].entities` carries object kinds, IDs and entity revisions, including optional checkpoint digests. Use `object show` for goal/plan/rule/work/checkpoint, `decision show` for decisions, `evidence show` for evidence, and `event show` for events. A `delta:<kind>` reference describes a historical entity change: use the source ID in its chunk key with source history and the context's revision window to retrieve immutable change receipts, including deleted identities. `source_entity_changes` omissions use the same source-history path. Important-event omissions and folded history can be paged with event history; omit the work/session filters to include project-global events, and use `--main` or `--branch <id>` for an exact branch. `--importance` and `--event-type` narrow the result. No history body enters context automatically.

Object queries refresh sources by default; `--cached` explicitly reads the last recorded projection. `--entity-revision` rejects a changed object instead of substituting it for the requested version. Checkpoints and events are immutable records and need no source refresh. Default object/event reads expose metadata and bounded summaries; full bodies require `--full`. Event full-read limits apply to payload bytes; projected-object limits apply to serialized object bytes.

Source metadata/history reads do not refresh projections and remain available for retired sources by exact ID, even without the manifest. A domain or locator must identify one active source. `source show --content` checks the current manifest's authorized roots, reads within the byte cap, and verifies the indexed fingerprint before emitting content. Optional source revision/fingerprint guards bind an earlier reference; changed bytes require reindexing and obtaining a new reference. A line-range read still caps and verifies the whole source first. Historical file bodies are not reconstructed from source events.

History returns summaries with `next_cursor`; repeat the same scope and `--through-revision` with `--cursor '<returned-json>'` for another page. The inclusive upper bound can pin history to a context revision while new work continues. Source history includes only source lifecycle/change events for that source, and all reads reject invalid revision windows. Evidence/report/artifact references keep their dedicated bounded, hash-checked read commands above.

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
