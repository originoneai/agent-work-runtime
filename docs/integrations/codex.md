# Using AWR from Codex

AWR supplies current project facts, work context and recoverable session memory.
Codex performs the work using its available tools. The integration uses AWR's CLI
for session/claim lifecycle and eight MCP tools for compact reads and work
mutations. Both access the same initialized project.

## Build and bind the project

Build both executables from an AWR checkout:

```sh
cargo build --locked -p awr-cli -p awr-mcp
```

Use absolute executable and project paths. Initialize the target project with a
reviewed source manifest as described in [the basic example](../../examples/basic/README.md).
For this repository, use its existing `.awr/project.toml` and database. Inspect
`session list --active` before creating another session on the same work.

Merge [the MCP configuration template](../../examples/codex/config.toml.example)
into the target project's `.codex/config.toml`, replacing its two paths. Preserve
existing configuration. Codex loads project configuration only for trusted
projects; the local CLI, desktop and IDE clients share MCP configuration on the
same host. [Official MCP documentation](https://learn.chatgpt.com/docs/extend/mcp)

An alternative is the CLI's user-level registration command:

```sh
codex mcp add awr -- /absolute/path/to/awr-mcp \
  --project /absolute/path/to/initialized/project
codex mcp get awr --json
```

Choose either project configuration or user-level registration for this server.
`codex mcp add` changes shared user configuration; the template itself changes
nothing. The server binds to one canonical project root at startup. Give servers
for different projects distinct names and explicit roots.

`codex mcp get` checks configured values. In the receiving client's `/mcp` view,
confirm a connected `awr` server, then request its project status and confirm the
project identity before work. Restart/reconnect the client after configuration
changes when required. A configured server is not proof of an active connection.
The expected tools are:

| Read tools | Mutation tools |
| --- | --- |
| `awr_project_status` | `awr_work_transition` |
| `awr_work_ready` | `awr_event_append` |
| `awr_work_get` | `awr_evidence_record` |
| `awr_context_compile` | |
| `awr_search` | |

Their argument and error contracts are in [the MCP reference](../../crates/awr-mcp/README.md).
The server does not require a model API key. Provider/model labels in AWR session
records do not configure or invoke Codex.

## Manual session workflow

These are commands for Codex's terminal tool or the operator's terminal. Run one
step at a time and inspect the response. The snippets use Bash, `jq`, an already
initialized project, and a work key selected from `ready`. Replace the
example identity with the current agent/provider/model. Keep the receipt
directory in the handoff; it contains local work context.

```sh
AWR_BIN=/absolute/path/to/awr
AWR_PROJECT=/absolute/path/to/initialized/project
AWR_WORK=EXAMPLE-001
AWR_AGENT=codex-primary
AWR_MODEL=your-current-model
AWR_NOTES=$(mktemp -d "${TMPDIR:-/tmp}/awr-codex.XXXXXX")
awrj() { "$AWR_BIN" --project "$AWR_PROJECT" --json "$@"; }

awrj session list --active
awrj ready
```

If a session already exists for the current work, inspect it with `session show`
and use its AWR session ID. For new work without a session, start and claim it:

```sh
AWR_REV=$(awrj status | jq -er '.project_revision')
awrj session start --work "$AWR_WORK" --agent "$AWR_AGENT" \
  --provider openai --model "$AWR_MODEL" --claim --ttl-ms 3600000 \
  --expected-revision "$AWR_REV" > "$AWR_NOTES/start.json"
AWR_SESSION=$(jq -er '.session.id' "$AWR_NOTES/start.json")
```

The returned claim is runtime ownership. It does not rewrite the source work's
status or owner. Use the appropriate AWR work transition before progress; inspect
conflicts rather than acquiring a competing claim. The AWR session ID is distinct
from the Codex conversation ID.

Read L0 for orientation, then L1 for execution:

```sh
awrj context bootstrap --session "$AWR_SESSION" --budget 1000 \
  > "$AWR_NOTES/bootstrap.json"
jq -e '.context.complete' "$AWR_NOTES/bootstrap.json"

awrj context compile --work "$AWR_WORK" --session "$AWR_SESSION" \
  --budget 5000 > "$AWR_NOTES/context.json"
jq -e '.completeness.complete and (.work_context != null)' "$AWR_NOTES/context.json"
```

Read the actual context, required facts and gaps, not just the boolean printed by
`jq`. L0 explicitly reports `execution_context_complete: false`. Supply concrete
`--path`, `--tag` or `--goal` inputs when the rules need them. `--source-sha` can
bind evidence currency to the full commit under review.

If hard facts exceed a budget, the command fails with `BudgetExceeded`. Inspect
its `required` count and explicitly increase the budget or clarify scope. Do not
delete hard facts to make the call pass. For example, this repository's P8-002
bootstrap needed 1,555 tokens on 2026-09-08; an explicit `--budget 2000` succeeded.
That observation does not satisfy the V1 1,000-token benchmark.

With a connected MCP server, the L1 read can instead use `awr_context_compile`
with these arguments, substituting the real AWR session ID:

```json
{"session":"<awr-session-id>","budget":5000}
```

Check `completeness.complete`, `work_context.context_hash` and its gap/omission
metadata. The five MCP read tools verify source freshness without refreshing the
persistent index. On `SourceStale`, inspect the source change, run the CLI's
`source reindex`, then read again. CLI context reads can refresh that index.
Read-only refresh differences and changed-source conflicts are documented in the
MCP reference. A transport's output limit is separate from AWR's token budget;
incomplete or visibly truncated delivery must be recovered before execution.

## Checkpoint before handoff or planned compaction

Save the hash of the last context actually used, a factual digest, the exact next
action and every unresolved loop. Replace the illustrative text below with the
current work's actual progress. Fetch the current revision after any source/work
mutations, but do not replace the last-used context hash with an invented hash.

```sh
AWR_CONTEXT_HASH=$(jq -er '.work_context.context_hash' "$AWR_NOTES/context.json")
AWR_REV=$(awrj status | jq -er '.project_revision')
awrj session checkpoint --session "$AWR_SESSION" \
  --context-hash "$AWR_CONTEXT_HASH" \
  --digest "Implemented the selected change; review results are still pending." \
  --next-action "Inspect the review result and address the remaining issue." \
  --open-loop "Independent review is unfinished." \
  --expected-revision "$AWR_REV" > "$AWR_NOTES/checkpoint.json"
awrj session show "$AWR_SESSION"
```

Checkpointing automatically records observed source/runtime changes. The supplied
digest and hash remain caller assertions; JSON reports `context_hash_verified:
false`. A successful save advances the project revision twice. Always use the
returned/current revision rather than adding one yourself. Incomplete save
attempts are retained for inspection and are never valid recovery checkpoints.

## Resume the work

For an actual handoff or new execution session, explicitly name the predecessor:

```sh
AWR_REV=$(awrj status | jq -er '.project_revision')
awrj session resume --from-session "$AWR_SESSION" \
  --agent "$AWR_AGENT" --provider openai --model "$AWR_MODEL" \
  --budget 5000 --expected-revision "$AWR_REV" > "$AWR_NOTES/resume.json"
jq -e '.context_ready' "$AWR_NOTES/resume.json"
AWR_PREDECESSOR=$AWR_SESSION
AWR_SESSION=$(jq -er '.resumed.session.id' "$AWR_NOTES/resume.json")
awrj session show "$AWR_SESSION"
awrj context compile --session "$AWR_SESSION" --budget 5000 \
  > "$AWR_NOTES/context.json"
jq -e '.completeness.complete and (.work_context != null)' "$AWR_NOTES/context.json"
```

Read the recovered context and verify the inherited checkpoint, next action,
unresolved loops and current source changes. Resume creates a new AWR session;
it does not start or switch a Codex conversation or model. Default resume
transfers a still-live claim with its original expiration, without extending its
TTL. After an expired/released claim, explicitly acquire a new claim when ready;
do not infer ownership from a predecessor's history.

If the same AWR session remains active after Codex compaction, reload context for
that session. A new AWR session is not required for every compact. Use resume when
there is a real session handoff. After a crash or lost response, inspect
`session show <predecessor>` and its successor before retrying. Even a nonzero
resume response can contain `resumed.session.id` with `context_ready: false`:
that successor already exists. Fix its context inputs and compile for it instead
of repeating the transition. No checkpoint means source/session-start recovery
with explicit missing-memory gaps.

When stopping work, checkpoint first, then close the session with the observed
revision and an appropriate outcome:

```sh
AWR_REV=$(awrj status | jq -er '.project_revision')
awrj session end --session "$AWR_SESSION" --outcome incomplete \
  --expected-revision "$AWR_REV"
```

An ended session releases its claims. It does not complete the source work;
`work complete` still requires the task's evidence and acceptance bindings.

## Automatic hooks: capability and verification boundary

Current official Codex documentation describes `SessionStart` with
`startup|resume|clear|compact`, `PreCompact` with `manual|auto`, and `SessionEnd`.
Project hooks require a trusted project layer and review/trust of the exact hook
definition. Use `/hooks` to inspect the receiving client's active definitions.
Multiple matching command hooks may run concurrently. `SessionEnd` is advisory,
has a short timeout, and does not fire immediately just because the user switches
away from a conversation. [Official hook documentation](https://learn.chatgpt.com/docs/hooks)

This integration supplies manual checkpoint/resume, an
[agent-instruction snippet](../../examples/codex/AGENTS.snippet.md), and a
[project-local lifecycle adapter](../TAKEOVER.md#client-checkpoints). Use `awr client install`
to preview its exact configuration before `--accept`. The receiver maps each native
conversation to an AWR session and checkpoints persisted continuity on lifecycle events.
Installation preserves existing hooks and never approves their trust automatically.
Verify actual trigger delivery in the receiving client; configuration presence and
synthetic receiver checks alone are insufficient evidence of native activation.

Verification is dated **2026-09-08**:

| Layer | Evidence boundary |
| --- | --- |
| Local Codex CLI | `codex-cli 0.153.4`; `mcp add/get` help inspected and the stdio template parsed with command-line overrides, without persisting configuration. |
| AWR CLI lifecycle | [Executable fixture walkthrough](../../examples/codex/README.md), plus manual bootstrap/context/checkpoint/resume on this repository's development work. |
| AWR stdio protocol | Eight-tool transport and domain checks in P8-001; developer stdio self-use is recorded separately from native Codex activation. |
| Native Codex MCP activation | Not verified by this delivery; requires a live `/mcp` connection and an actual tool call in the receiving client. |
| Automatic Codex hooks | Project adapter and receiver implemented with local shell/protocol checks; native activation still requires exact hook trust and actual client trigger evidence. |
| Real business acceptance/release | Not established by configuration, lifecycle demos or development self-use. |

The versioned verification and source/remote bindings are maintained in
[the work ledger](../../ledger/work-ledger.yaml), not inferred from this guide.
