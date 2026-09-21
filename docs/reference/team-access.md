# Team command validation and remote profiles

The public Team command path does **not** yet implement authenticated HTTP/MCP
transport, credential loading, queuing or domain dispatch. `awr team command`
and `awr-server command` return `Unsupported` for a valid command; they never
report `accepted=true`, replay status or cached results. Missing/offline remotes
cannot fall back to personal SQLite authority. `awr team capabilities` describes
local protocol metadata, explicitly reports `local=true`, `submitted=false` and
`command_transport=false`, and does not contact a server.

The shared pure parser currently recognizes `capabilities` and `work.claim`.
Only capabilities are locally executable. Claim has a validation schema, not a
remote dispatch target. Other operations, including `execution.cancel` and
`source.activate`, return `Unsupported` online and offline. PostgreSQL store APIs
are separate from this wire entry. Future adapters must register an operation's
schema, read/write policy and actual dispatch before announcing support.

Received envelopes must be JSON objects with explicit `protocol_version`,
nonempty `request_id` and `op`. Receivers never invent these fields. Only version
1 (integer or canonical decimal string) is supported; conversion never truncates.
`args` must be an object. Claim requires nonempty string `work_id`, `scope_id`,
`session_id`, `expected_contract_hash`, and an unsigned `expected_work_version`
(integer or canonical decimal string; decimal strings preserve all 64 bits).
Unknown argument fields are rejected. Business preconditions remain the domain
store's responsibility. Capabilities permit only empty args, or omitted args.

For local integration testing, the server skeleton offers an explicit validator:

```sh
awr-server validate-command --op work.claim \
  --tenant-id tenant-a --project-id project-a --actor-id actor-a --client-id client-a \
  --body '{"protocol_version":1,"request_id":"example","op":"work.claim","args":{"work_id":"work-a","scope_id":"main","session_id":"session-a","expected_work_version":"1","expected_contract_hash":"example-hash"}}'
```

This performs no database/network operation and returns `validation_only=true`,
`submitted=false`, `authenticated=false`, and the separately supplied context.
Those local flags are **test context, not authentication credentials**. Request
identity declarations cannot override any tenant, project, actor or client field.
`awr_team::execute` also checks retained declarations against its supplied context;
a future authenticated adapter must obtain that context independently of the body.
The `http`, `mcp`, `cli` labels in pure-function tests are not three live transports.
Personal `awr-mcp` has no Team command dispatch tool; its shared MCP service remains
independent of this Team skeleton.

`awr remote` stores an endpoint, project key and credential environment *name*,
using TOML serialization. Profiles are validated on both save and load. Endpoints
must be absolute HTTPS URLs, or HTTP URLs on localhost/loopback IPv4/IPv6. Database
schemes, user/password information, query strings and fragments are rejected;
credentials belong in the referenced environment, not the URL. Output independently
replaces invalid endpoints with a placeholder, and TOML parse errors never echo
configuration contents. Valid Unicode project keys roundtrip through add/inspect.
Profiles must explicitly declare supported `protocol_version = 1`.

These command-validation checks do not establish authenticated network isolation,
PostgreSQL authorization, business acceptance or release readiness.
