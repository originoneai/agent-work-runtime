# Connect your agent

AWR does not run your agent. The agent client (Codex, Claude Code, Kimi, Cursor, Grok) does the work
with its own tools; AWR supplies current project facts, work context, claims, and recoverable
session memory for the same initialized project. Provider and model labels in AWR session records
are metadata only — they never launch or configure the client.

Every supported client follows the same pattern:

1. Build both executables from an AWR checkout:

```sh
cargo build --locked -p awr-cli -p awr-mcp
```

2. Initialize the target project from its reviewed source manifest (see
   [Quickstart](quickstart.md)), then register the `awr-mcp` server with the client, pointing
   `--project` at the project's absolute path. Each server binds to one canonical project root;
   give servers for different projects distinct names.
3. Verify the connection — a config file on disk is not a live connection — and confirm project
   identity with a status read before any work.
4. Work the shared session lifecycle: start or resume a session, read context, checkpoint before
   handoff, end when you stop. See [CLI](cli.md) and [MCP tools](mcp.md).

The sections below give each client's merge paths, configuration, and caveats. Where a client
documents automatic lifecycle hooks, treat them as unverified until you have seen a real trigger and
receipt; the manual checkpoint/resume flow always works.

## Codex

Codex loads project configuration only for trusted projects; its CLI, desktop, and IDE clients share
MCP configuration on the same host. Pick one of two options: merge the MCP configuration template
into the project's `.codex/config.toml` (replacing its two paths, preserving existing
configuration), or register at user level:

```sh
codex mcp add awr -- /absolute/path/to/awr-mcp \
  --project /absolute/path/to/initialized/project
codex mcp get awr --json
```

In the client's `/mcp` view, confirm a connected `awr` server and verify the project identity before
work; a configured server is not proof of an active connection. Expected read tools:
`awr_project_status`, `awr_work_ready`, `awr_work_get`, `awr_context_compile`, `awr_search`.
Mutation tools: `awr_work_transition`, `awr_event_append`, `awr_evidence_record`. The server does
not require a model API key.

Codex is the one client with an installer for automatic lifecycle hooks: use `awr client install` to
preview the exact configuration before `--accept`. Installation preserves existing hooks and never
approves their trust automatically. Codex documents `SessionStart` (`startup|resume|clear|compact`),
`PreCompact` (`manual|auto`), and `SessionEnd` (advisory, short timeout). Verify actual trigger
delivery in the client — until then, checkpoint manually before handoff or compaction.

## Claude Code

Claude Code connects through a named controlled adapter (adapter id `claude_code`). It is not
auto-startable: AWR will not launch Claude Code for you, and it will not stop it on its own — those
steps stay with you. What the adapter does support is reading status, reconnect/resume, and result
forensics.

Operator path:

1. Start Claude Code yourself.
2. Bind it to AWR with the generic client and the native conversation ID:

```sh
awrj client bind --client generic --external-session claude:<native-id> \
  --work "$AWR_WORK" --session "$AWR_SESSION"
```

3. When you need AWR observations, report phases through the shared CLI's external execution
   report.
4. On retry, reconnect the same execution identity before starting anything new.

## Kimi

This guide targets Kimi Code 0.41.0 (inspected 2026-09-08); older `kimi-cli` releases use different
configuration paths and flags, so check `kimi --version` and `kimi --help` first. Use Kimi's
terminal tool to call the AWR CLI, and optionally connect the same project over stdio MCP by merging
this server into the project's `.kimi-code/mcp.json` (user level: `~/.kimi-code/mcp.json`),
preserving other entries:

```json
{
  "mcpServers": {
    "awr": { "command": "/absolute/path/to/awr-mcp",
      "args": ["--project", "/absolute/path/to/initialized/project"],
      "cwd": "/absolute/path/to/initialized/project" }
  }
}
```

Project configuration requires workspace trust. Use `/mcp-config` for configuration and `/mcp` to
inspect connection status; servers added by editing configuration join newly created sessions.
Confirm project identity with `awr_project_status`. The provider label for Kimi session metadata is
`moonshot`.

Kimi's own `kimi --continue`, `kimi --session <kimi-conversation-id>` and `/compact` operate on its
conversation; their IDs are separate from AWR IDs. After a compact, compile context for the same
active AWR session — an explicit AWR resume is only for a real handoff. Kimi documents
`SessionStart`, `SessionEnd`, `PreCompact`, and `PostCompact` hooks, but this integration installs
no automatic adapter; use the manual checkpoint process.

## Cursor

Checked against Cursor 3.18.9 (2026-09-17). Do not run `awr client install` for Cursor
(`Unsupported`), and do not pass `--client cursor` to bind (`InvalidInput`) — use the generic client
identity shown below. Merge the stdio template into the project `.cursor/mcp.json` or the user
`~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "awr": { "type": "stdio", "command": "/absolute/path/to/awr-mcp",
      "args": ["--project", "/absolute/path/to/initialized/project"] }
  }
}
```

Cursor's stdio field table requires `"type": "stdio"` and does not accept `cwd`. AWR only needs an
absolute `command` plus `--project`. Reload the window and check **Output → MCP Logs** on startup
failure; when both config files exist, confirm in **Customize** which one the Agent Window attached.
Two caveats:

**Grouped tools.** In the current source tree, the default `tools/list` returns eight domain tools
(`awr_query`, `awr_context`, `awr_work`, `awr_evidence`, `awr_session`, `awr_continuity`,
`awr_change`, `awr_compaction`) rather than flat tool names. Probe status through `awr_query`:

```json
{"child_tool": "awr_project_status", "arguments": {}}
```

Flat names remain callable but are not in the default catalog; set `AWR_MCP_TOOL_EXPOSURE_MODE=flat`
only if your Cursor build cannot route through domains. Packaged 0.4.0 does not expose grouped
`awr_query` — build from the source tree for this route.

**Cloud Agents** cannot reach a laptop `awr-mcp` or `127.0.0.1`; they need a reachable HTTPS front
door with server-side project roots. AWR's shared HTTP service uses a bearer token (not Cursor
OAuth):

```json
{
  "mcpServers": {
    "awr": { "url": "http://127.0.0.1:8080/mcp",
      "headers": { "Authorization": "Bearer ${env:AWR_ENGINEERING_TOKEN}" } }
  }
}
```

For identity, bind the generic client with the native conversation ID, failing if it is empty so you
never bind the literal `cursor:`:

```sh
: "${HOST_CONVERSATION_ID:?set the native host conversation ID first}"
AWR_EXTERNAL="cursor:${HOST_CONVERSATION_ID}"
awrj client bind --client generic --external-session "$AWR_EXTERNAL" \
  --work "$AWR_WORK" --session "$AWR_SESSION"
```

Cursor documents `sessionStart`, `sessionEnd`, and `preCompact` hooks in `.cursor/hooks.json`, but
there is no installer for that dialect — checkpoint manually until you have a live hook trigger and
checkpoint receipt.

## Grok

Checked against Grok Build 1.0.13 (2026-09-08). Use Grok's terminal tool to call the AWR CLI, or
connect its stdio MCP client from the project directory:

```sh
cd /absolute/path/to/initialized/project
grok mcp add --scope project awr -- /absolute/path/to/awr-mcp \
  --project /absolute/path/to/initialized/project
grok mcp doctor awr --json
```

`add --scope project` writes or updates `.grok/config.toml` (user scope is the default when the flag
is omitted); use a distinct server name if `awr` already refers to another project. The equivalent
table is:

```toml
[mcp_servers.awr]
command = "/absolute/path/to/awr-mcp"
args = ["--project", "/absolute/path/to/initialized/project"]
```

Use `/mcps` in Grok Build to inspect and refresh connections, then call `awr_project_status` and
verify the project identity. Project trust is a separate prerequisite: an untrusted folder leaves
the server unstarted, so review the project in the client's normal trust workflow first. The
provider label for Grok session metadata is `xai`.

Grok's native conversation continuation is separate from AWR session recovery:

```sh
grok --cwd "$AWR_PROJECT" --continue
grok --cwd "$AWR_PROJECT" --resume <grok-conversation-id>
```

Grok's `--session-id` creates a new conversation — it is neither an AWR ID nor a resume flag. After
native compaction, compile context for the same active AWR session; use the explicit AWR resume flow
only for a real handoff.

Grok Build documents session and compact events in its hook system, but no automatic adapter is
installed here; use the manual checkpoint process until you have verified an actual trigger and
receipt. Grok web's custom connectors require a reachable MCP URL — a local executable path cannot
be entered as that URL; AWR's shared HTTP service can serve that role, but deploying it and meeting
the connector's authentication requirements is a separate step this guide does not verify.

## Next steps

- [MCP tools](mcp.md) — the full tool surface your client can call once connected.
- [Daily workflow](daily-workflow.md) — the start-work-checkpoint-resume loop, whichever client you use.

