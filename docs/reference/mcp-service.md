# Shared MCP service

The source build supports a single Streamable HTTP endpoint for multiple projects
and independent MCP clients. This capability is not in the published 0.3.2 packages.
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

Call `awr_projects_list` to discover authorized project keys. Every project tool
then requires `project`, for example:

```json
{"project":"billing","work":"INVOICE-001"}
```

There is no shared current project and no tool for opening an arbitrary server
path. Configuration changes take effect after a service restart. Disconnecting a
client or restarting the HTTP service does not end an AWR work session. Protocol
connections and persistent AWR sessions are separate identities.

Reads can run concurrently. Writes are serialized per project, with existing
database revision and source-fingerprint checks for other processes. Different
projects have independent operation locks. A stale write returns a conflict;
read the current state and reconsider the intended change before retrying. An
interrupted write may already have committed; never infer failure from a closed
HTTP response stream.
