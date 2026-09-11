# Shared MCP service

AWR 0.3.3 supports a single Streamable HTTP endpoint for multiple projects
and independent MCP clients. Both npm and PyPI packages include this capability.
The existing `awr-mcp --project /absolute/project` stdio command remains available.

Initialize each project on the server using `awr init`. Register its canonical
absolute root and the `project_id` returned by `awr --json status`. AWR verifies
that identity at startup and when handling requests. Source files and each
project's `.awr` database stay separate; the service does not clone repositories,
mount client filesystems, or merge their ledgers.

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

Provide distinct, randomly generated bearer credentials through those environment
variables (at least 32 characters), then run:

```sh
awr-mcp --registry /etc/awr/service.toml --listen 127.0.0.1:8080
```

Connect clients to `/mcp` with their `Authorization: Bearer …` header. For remote
access, terminate HTTPS at an authenticated deployment boundary and restrict direct
access to the backend. The built-in mechanism is static bearer authentication, not
an OAuth authorization server or an enterprise identity provider. Every HTTP
request is authenticated. `write` includes read access; `read` grants cannot mutate
projects. Origins are denied unless explicitly listed in `allowed_origins`;
native clients normally omit Origin. The SDK checks `allowed_hosts`, defaulting
to loopback hosts when none are configured.

Configure each MCP client for Streamable HTTP, the same service URL, and its own
bearer credential using that client's credential mechanism. Clients need network
access to the service, not a local AWR executable or a separate server process per
project. Project files must be available to the server; a path on a client laptop
does not become readable remotely. The service validates Origin but does not
implement browser CORS preflight; a browser frontend needs an appropriate gateway.

Call `awr_projects_list` to discover authorized project keys. Every project tool
then requires `project`, for example:

```json
{"project":"billing","work":"INVOICE-001"}
```

There is no shared current project and no tool for opening an arbitrary server
path. Configuration changes take effect after a service restart. Disconnecting a
client or restarting the HTTP service does not end an AWR work session. Protocol
connections and persistent AWR sessions are separate identities.

Keep client IDs stable across credential rotation to retain conversation and
request bindings. Changing the client ID creates a distinct scope. On Unix,
SIGTERM and Ctrl-C gracefully stop accepting requests and drain active HTTP work;
they do not end persistent AWR sessions. Use the deployment's process supervisor
for service startup/restart. AWR does not install a system daemon automatically.

Reads can run concurrently. Writes are serialized per project, with existing
database revision and source-fingerprint checks for other processes. Different
projects have independent operation locks. A stale write returns a conflict;
read the current state and reconsider the intended change before retrying. An
interrupted write may already have committed; never infer failure from a closed
HTTP response stream.

## Work session lifecycle

The lifecycle tools are available over HTTP and stdio. HTTP binds the authenticated
client identity; stdio uses the local `stdio` identity. Supply a stable `conversation`
identifier from the host, not an HTTP connection identifier. The same conversation
string in another project or authenticated client has a separate binding.

| Tool | Purpose |
| --- | --- |
| `awr_session_start` | Start a work-bound session and optionally claim its work; bind the conversation atomically. |
| `awr_session_get` | Inspect binding, session, claims, checkpoint, interrupted saves and successor. |
| `awr_session_list` | Page through this client's session history with `next_before_revision`. |
| `awr_session_checkpoint` | Persist the actual consumed context hash, digest, next action and open loops. |
| `awr_session_claim` | Acquire or release the selected session's claim. |
| `awr_session_end` | Explicitly end/interrupt a session and release its claims. |
| `awr_session_resume` | Create a successor with fresh context and inherited checkpoint/claims. |

Start with a current project revision and explicit `work`, `conversation`, `agent`,
`provider`, `model` and optional `claim: true`. An existing matching binding is
returned with `binding_reused: true`; this does not reacquire a claim, change its
TTL or reactivate a closed session. Inspect its current status and claims. Reusing
a conversation for different work or identity is rejected.

Session inspection, context compilation, checkpoints, claims, work transitions
and event appends accept a conversation selector. If both `session` and
`conversation` are provided, they must agree. Explicit resume requires the
predecessor `session` and target `conversation` (which may stay the same). Shared
context calls without a session must explicitly set `work` and `detached: true`.
The service never guesses from another client's active conversation.

Resume follows the existing domain rules, including current work-branch selection
and context completeness. A returned `isError` may include a committed successor
whose final context needs attention; inspect that successor before trying again.
Session reads and cleanup remain available when source files are unavailable.

## Waiting and continuation

`awr_session_wait` requires the selected session, question, actual consumed
`context_hash`, progress `digest`, `next_action`, optional `open_loops`, and write
identities described below. It saves a checkpoint before recording a persistent
wait. The host collects the user's answer and calls `awr_session_reply` with the
wait ID and `reply`. Set `cancel: true` with an explicit reason to cancel a wait.
The saved reply remains readable after service restart or session closure.

A pending wait blocks MCP work transitions and resume. Recording a reply clears
that block; it does not change work status, run a model, or schedule the host's
next turn. The host inspects the session and compiles current context, then either
continues the active session or explicitly creates a successor. Claim leases are
not automatically renewed during a wait.

Session inspection and context responses include a `continuity`/wait envelope.
The host must consume it alongside the L1 context packet. Replies in this envelope
are separate from the packet's token budget and hash. History includes up to 100
recent waits along at most 32 predecessor links; it is not an unlimited transcript.

`awr_source_reindex` explicitly refreshes the selected project's projections from
its configured authoritative sources. It requires write access and a current
revision, does not modify source files, and reports partial failures. Ordinary
reads remain read-only. An interrupted multi-source reindex may require source
inspection and another explicit refresh; it has no single atomic commit receipt.

## Write identity and unknown outcomes

Every shared HTTP write requires a client-generated, stable `request_id` (up to
256 bytes) and `expected_revision`. Stdio accepts request IDs optionally, preserving
existing local clients. Scope is project plus authenticated client plus request ID.

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

The example is an `awr_session_start` call. Save the returned `project_revision`
for subsequent writes. The request journal and domain operation both consume
revisions, so never calculate the next revision by adding one. Domain and response
receipt revisions are exposed separately. Failed domain operations may also have
recorded request receipts and consume revisions.

Repeating exactly the same request ID and arguments returns the recorded result
without executing the domain action again. Changed arguments, including a changed
expected revision, require a new ID. A new ID is appropriate only after inspecting
the previous outcome and deciding to perform a new operation. The current revision
returned on replay does not make the original result or context current.

After a timeout or disconnect, call `awr_operation_get` with the same project and
request ID. A recorded result includes its original error/success status. An
unfinished request reports `write_outcome: unknown` and correlated domain receipt
references. Neither retry nor inspection automatically replays an unknown action.

`awr_operation_recover` takes that request ID and a current expected revision. It
records a committed outcome only when an unambiguous terminal domain event is
bound to the request. Recovery never invokes the original tool. Its response is a
receipt-based outcome, not a reconstruction of the original context or response;
inspect session/work state and compile fresh context before continuing. If proof
is absent, partial or exceeds the receipt inspection bound, the request stays
unknown. Absence of a response receipt does not prove absence of a side effect.

These guarantees cover AWR's recorded domain actions. They do not make unrelated
host tools, third-party API calls or filesystem changes exactly-once operations.
