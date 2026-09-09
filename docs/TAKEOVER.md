# Project intake and work continuity

This addition covers four separately verified capabilities: project intake, client checkpoints, execution registration, and recovery inspection. It extends the existing AWR work runtime; it does not transfer arbitrary process memory or reconstruct unrecorded conversations.

## Intake

Run `awr --project /absolute/project init` to inspect the inventory and proposed source mapping. Nothing is initialized until `--accept` is supplied. Conventional YAML sources are retained; Markdown task tables and checklists can be projected with the read-only `markdown-ledger-v1` adapter. Existing Markdown remains the authority and is edited in its original file, then reindexed.

Missing goals, plans, rules and task ledgers are proposed under `.awr/intake/`. These are source files, not disposable runtime state. The first work item establishes the project baseline. Plan headings create planned review tasks; they do not assert that implementation is missing or completed. Existing project files are never overwritten.

For a blank project, state its purpose with `awr init --goal "Deliver a document portal" --accept`. For a project with complex requirements, save a JSON draft with `awr init --write-draft /outside/project/intake.json`. An agent or owner can use the inventory and original documents to replace the generated work with concrete actions, dependencies and acceptance criteria. Apply the reviewed draft with `awr init --from-draft /outside/project/intake.json --accept`. A changed input inventory requires a new review. Generated paths are limited to the four owned intake source files.

The scanner records filenames, sizes, document hashes and observed Git state. It skips hidden/vendor/build directories and symlinks and has explicit resource limits. It does not infer business completion from code filenames or Git commits. Arbitrary spreadsheet/Word/PDF ledgers still require conversion or an explicit adapter; unrecognized states remain unknown.

## Client checkpoints

The Codex project adapter is installed with:

```sh
awr client install --client codex --work INTAKE-001
awr client install --client codex --work INTAKE-001 --accept
```

It merges `SessionStart`, `PreCompact`, `PostCompact`, `Stop`, `SessionEnd` and `Interrupt` handlers into the project's `.codex/hooks.json`, preserving existing handlers and config. Review and trust the exact definitions in a fresh client's `/hooks` view. Installation always reports `activation_verified: false`; a generated file or a synthetic receiver call is not proof that a native client activated the hooks. The adapter follows the [official lifecycle contract](https://learn.chatgpt.com/docs/hooks).

The receiver binds a native conversation ID to a work-bound AWR session. SessionStart/PostCompact return recovery context. Stop/PreCompact/SessionEnd/Interrupt save the persisted next action, open loops and actual AWR event/source delta. Shutdown hooks are advisory and do not terminate sessions, release claims or kill processes. Explicit session end/handoff retains those responsibilities.

Record a changed next action before it is lost from the client:

```sh
awr client progress --client codex --external-session CLIENT_ID \
  --next-action "Apply the reviewer corrections" --open-loop "Independent review remains"
awr client show --client codex --external-session CLIENT_ID
```

The adapter does not read transcript bodies or infer facts from assistant prose. A binding command returns the compiled context; its hash identifies that snapshot, not proof that a model consumed it. Duplicate events with unchanged work reuse their checkpoint. A continued turn with changed persisted progress creates a new checkpoint. Process locks release automatically on crashes. A checkpoint completed before a lost client receipt is recovered by its delivery key; incomplete saves are never credited.

For explicit cross-client handoff, bind a new native conversation to a predecessor:

```sh
awr client bind --client generic --external-session NEW_CLIENT_ID \
  --work INTAKE-001 --from-session AWR_PREDECESSOR_ID
```

`generic` and `kimi` identities can use the normalized JSON receiver; automatic installation is currently provided for Codex only. The receiver accepts the documented `session_id`, `cwd`, `hook_event_name`, optional `turn_id` and `model` fields. Native mode emits only documented hook output fields; `--json` adds diagnostic AWR receipts. This does not start a native client or migrate its process memory.

## Executions

```sh
awr execution run --session AWR_SESSION_ID --key build-reviewed-report \
  --purpose "Build the reviewed report" -- report-builder --output report.pdf
awr execution list --work INTAKE-001
awr execution show EXECUTION_ID
awr execution register --session AWR_SESSION_ID --key external-report \
  --purpose "Track the CI report build" --reference 'ci://build/42'
```

Intent is committed to the immutable event journal before dispatch. Each project-wide operation key identifies one exact intent and work/branch binding, even after a session handoff. Repeating it returns that execution; changing its intent is a conflict. A registered operation whose dispatch was interrupted is not automatically retried. This is at-most-once dispatch, not a promise of exactly-once effects across crashes.

Managed commands run through a separate local AWR supervisor with null stdin and their argument vector unchanged. The invoking CLI can exit, and the AWR session can end, while the command continues. Start/end records, exit code or signal, stdout/stderr and an atomic result receipt are retained under `.awr/executions/`, excluded from Git. The supervisor records only its direct child's outcome; a command that backgrounds unrelated descendants does not transfer their lifecycle to AWR. Commands inherit the launch environment; AWR does not persist environment secrets or grant new permissions. On Windows, the launcher clears inheritance on its own standard pipe handles before dispatch so a piped caller can return while the managed command is running; the caller's handles remain open. The implementation uses the documented [handle inheritance flags](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-sethandleinformation).

External references remain unverified. Recording a PID, a URL or an asserted success does not produce a managed completion. There is no automatic takeover of arbitrary existing processes or client memory.

## Recovery inspection

```sh
awr execution inspect EXECUTION_ID
awr recovery inspect --session AWR_SESSION_ID
awr session resume --from-session AWR_SESSION_ID --agent successor \
  --provider generic --model selected-model --no-claim --expected-revision REVISION
```

`execution inspect` and `recovery inspect` open the runtime database read-only. They never restart commands, kill processes, update sources or create successor sessions. Recovery inspection includes the last successful checkpoint, while `session resume` performs the existing source refresh, revision checks and successor transition.

Each execution observation is `running`, `succeeded`, `failed` or `unknown`, with an observation time and evidence basis. Running requires a response from the registered loopback supervisor matching its execution ID, nonce, supervisor PID and child PID. Success/failure comes from the supervisor's immutable completion event, or its identity-bound atomic result receipt if the final database write was interrupted. A missing supervisor, invalid receipt, absent dispatch or unsupported external executor remains unknown. A numeric PID or a produced file alone is insufficient. Observations are local snapshots; running status can change immediately afterward.

L0 and L1 retain every execution's recorded facts for the selected work and branch, including prior sessions' results. These records participate in deterministic context hashing and mandatory token budgeting. When they do not fit, compilation fails explicitly rather than omitting them. Live observations are separately labeled in resume output and client SessionStart/PostCompact context, because network/time observations are not part of the source-context hash. Automatic inspection has a two-second work budget; remaining records are returned as unknown with a reason and an explicit inspection command. The complete native additional-context output has a 10,000-token ceiling.

Recovery checks use actual owned subprocesses to cover caller/session exit, live continuation, success, failure, a killed fixture supervisor, duplicate keys and a fault-injected final database write. These are local integration checks. Native hook trust/activation, full E4 acceptance, other operating systems and publication of this feature batch remain separate delivery steps.
