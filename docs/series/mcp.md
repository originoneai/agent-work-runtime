# The MCP service

AWR exposes its work ledger through MCP (Model Context Protocol), the standard
protocol that lets an agent host — such as a coding assistant or desktop AI
client — discover and call tools. The MCP service is how your agent reads
project status, compiles context, records evidence, and moves work forward
without shelling out to the CLI itself. There are two ways to run it:

- **stdio** — a local process your client spawns directly, one project at a
  time. This is the classic setup and remains available.
- **Shared HTTP** — a single Streamable HTTP endpoint serving multiple projects
  and multiple independent clients. Both the npm and PyPI packages include this
  capability.

This article focuses on the shared HTTP service, which is what you run when
more than one person or agent needs the same projects. For what agents do with
these tools once connected, see [Agents](agents.md). For the human-facing
equivalent of the same operations, see [The AWR CLI](cli.md).

## How the shared service works

You initialize each project on the server with `awr init`, then register its
canonical absolute root together with the `project_id` returned by:

```sh
awr --json status
```

AWR verifies that identity at startup and on every request. Source files and
each project's `.awr` database stay separate — the service does not clone
repositories, mount client filesystems, or merge project ledgers. Clients need
network access to the service, not a local AWR executable, and project files
must be readable on the server: a path on a client laptop does not become
readable remotely. There is no shared "current project" — every project tool
takes an explicit `project` argument, and there is no tool for opening an
arbitrary server path.

## Configuring the service

Create an operator-owned TOML file outside the public repository:

```toml
version = 1
allowed_hosts = ["awr.internal.example"]

[[projects]]
key = "billing"
root = "/srv/projects/billing"
project_id = "<actual project ID>"

[[projects]]
key = "support"
root = "/srv/projects/support"
project_id = "<actual project ID>"

[[clients]]
id = "engineering"
token_env = "AWR_ENGINEERING_TOKEN"
write = ["billing", "support"]

[[clients]]
id = "reviewer"
token_env = "AWR_REVIEWER_TOKEN"
read = ["billing"]
```

Each client entry names an environment variable that holds its bearer
credential. Provide distinct, randomly generated credentials of at least 32
characters. A `write` grant includes read access; a `read` grant cannot mutate
projects. Keep client IDs stable across credential rotation — conversation and
request bindings attach to the client ID, and changing it creates a distinct
scope. Configuration changes take effect only after a service restart.

## Running the service

```sh
awr-mcp --registry /etc/awr/service.toml --listen 127.0.0.1:8080
```

Every HTTP request is authenticated. The built-in mechanism is static bearer
authentication — not an OAuth authorization server or an enterprise identity
provider. For remote access, terminate HTTPS at an authenticated deployment
boundary and restrict direct access to the backend. Additional access controls:

- Origins are denied unless explicitly listed in `allowed_origins`; native
  clients normally omit the `Origin` header.
- The SDK checks `allowed_hosts`, defaulting to loopback hosts when none are
  configured.
- The service validates Origin but does not implement browser CORS preflight,
  so a browser frontend needs an appropriate gateway.

On Unix, SIGTERM and Ctrl-C gracefully stop accepting requests and drain active
HTTP work. Disconnecting a client or restarting the service never ends an AWR
work session — protocol connections and persistent sessions are separate
identities. AWR does not install a system daemon — use your deployment's
process supervisor for startup and restart. For local, single-client use, the
stdio command remains:

```sh
awr-mcp --project /absolute/project
```

## Connecting a client

Configure each MCP client for Streamable HTTP with the same service URL using
the `/mcp` path, plus its own bearer credential in an `Authorization: Bearer …`
header delivered through that client's credential mechanism. Once connected,
call `awr_projects_list` to discover the project keys your credential is
authorized for. Every project tool then requires `project`:

```json
{"project": "billing", "work": "INVOICE-001"}
```

## The tools

The shared HTTP service exposes 21 tools in total (20 over stdio, which omits
the project-listing tool). They fall into three groups.

### Work and context tools

These mirror the everyday CLI actions:

| Tool | What it does |
| --- | --- |
| `awr_project_status` | Current continuation, claimable work, waits, blockers, history summary |
| `awr_work_ready` | Ready items with diagnostics and claim hints |
| `awr_work_get` | A work item's tasks, acceptance, dependencies, decisions, evidence |
| `awr_context_compile` | Compile the context packet for a work item or branch |
| `awr_work_transition` | Progress, block, unblock, cancel, reopen, or complete work |
| `awr_event_append` | Append an event to the ledger |
| `awr_evidence_record` | Record evidence bound to work and acceptance items |
| `awr_search` | Search across the project ledger |

### Session and continuation tools

Sessions bind a conversation to a unit of work. Supply a stable `conversation`
identifier from your host — not an HTTP connection identifier. The same
conversation string under another project or client is a separate binding.

| Tool | What it does |
| --- | --- |
| `awr_session_start` | Start a work-bound session, optionally claiming the work |
| `awr_session_get` | Inspect binding, session, claims, checkpoint, interrupted saves |
| `awr_session_list` | Page through this client's session history |
| `awr_session_checkpoint` | Persist the consumed context hash, digest, next action, open loops |
| `awr_session_claim` | Acquire or release the session's claim |
| `awr_session_end` | Explicitly end or interrupt a session and release its claims |
| `awr_session_resume` | Create a successor session with inherited checkpoint and claims |
| `awr_session_wait` | Save a checkpoint and record a persistent question for the user |
| `awr_session_reply` | Deliver the user's answer to a recorded wait |
| `awr_operation_get` | Inspect the recorded outcome of a write by request ID |
| `awr_operation_recover` | Record a committed outcome for an interrupted write when proof exists |
| `awr_source_reindex` | Refresh the project's projections from its authoritative sources |

A pending wait blocks work transitions and resume until the host records a
reply. The reply clears that block but does not change work status or schedule
the host's next turn — your host inspects the session, compiles fresh context,
and decides whether to continue or create a successor.

## Writes, revisions, and uncertain outcomes

Every shared HTTP write requires two things:

- a client-generated, stable `request_id` (up to 256 bytes),
- the `expected_revision` you last observed.

```json
{
  "project": "billing",
  "request_id": "host-turn-42-start",
  "expected_revision": 120,
  "work": "INVOICE-001",
  "conversation": "invoice-review",
  "agent": "billing-assistant",
  "provider": "example-provider",
  "model": "example-model",
  "claim": true
}
```

Repeating exactly the same request ID and arguments returns the recorded result
without executing the action again, so a retry after a timeout is safe. Changed
arguments require a new ID. Save the returned `project_revision` for your next
write, and never compute the next revision by adding one: the request journal
and the domain operation both consume revisions.

After a timeout or disconnect, call `awr_operation_get` with the same project
and request ID. An unfinished request reports `write_outcome: unknown`; an
interrupted write may already have committed, so never infer failure from a
closed HTTP response stream. `awr_operation_recover` can record a committed
outcome only when an unambiguous terminal event is bound to the request — it
never re-invokes the original tool. Reads can run concurrently; writes are
serialized per project, and different projects have independent locks. A stale
write returns a conflict — read the current state and reconsider before
retrying.

## Workstream reads (in development)

The development source adds a registry `version = 2` that grants clients
read-only access to explicitly listed, immutable workstreams for projects using
the YAML workstream ledger. These are operator-owned permissions: project-level
`read`/`write` grants alone do not authorize any isolated workstream.

```toml
[[clients.workstreams]]
project = "billing"
workstream_id = "<actual immutable workstream ID>"
authority_version = 1
```

Authorized clients use the shared-only `awr_workstream` tool, starting with
`action: "capabilities"` and `action: "list"` to discover what they may read.
Be aware of the current limits: this extension grants reads only — no
mutations, content-file reads, or Team PostgreSQL operations — and projects
that enable it reject the legacy shared tools, including all writes, until an
authorized implementation is available. Enablement and reindex are local
operator actions at this stage.

## Where to go next

- [Agents](agents.md) — how an agent uses sessions, claims, and checkpoints
  over these tools.
- [The AWR CLI](cli.md) — the same domain operations from the command line,
  for operators and debugging.
- [Troubleshooting](troubleshooting.md) — revision conflicts, stale sources,
  and recovery.
