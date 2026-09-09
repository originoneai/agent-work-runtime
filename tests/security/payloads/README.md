# Payload and secret boundaries

`contract.json` defines 32 component conditions. Use a new local report path for
every invocation; preserve failed runs. These checks use synthetic values and
disposable projects. They do not establish real-client business acceptance.

```sh
cargo build -p awr-cli -p awr-mcp --locked
python3 tests/security/payloads/verify_all.py --report .local/payload-boundary-checks.json
```

The complete runner rebuilds CLI/MCP, executes all four groups and checks that
all conditions share the same contract hash without duplicate or missing IDs.
You can also run a focused group:

| Runner | Conditions | Boundary |
| --- | ---: | --- |
| `verify_source_bounds.py` | 9 | Markdown 2 MiB / YAML 4 MiB, reads, indexing, proposals and bounded caller budgets |
| `verify_events.py` | 5 | Payload sizes, field allowlists, types, references and rollback through storage and CLI/MCP |
| `verify_artifacts.py` | 2 | 64 MiB import / 16 MiB read, source snapshots, hashes and path redirection |
| `verify_secrets.py` | 16 | Shared recognition, source/runtime writes, old-data output and CLI/MCP transport |

Each focused command takes `--report .local/new-name.json`. The size limit itself
is allowed; one extra byte is rejected. Smaller caller limits remain effective.
Failed refresh retains the old projection with an explicit freshness failure.

Required facts containing forbidden values cannot be dropped to make a context
look complete. Ordinary authentication discussion and empty schema definitions
must remain usable; credential values hidden in examples or nested fields must
still be rejected. See the [event contract](../../../docs/reference/event-payloads.md),
[artifact limits](../../../docs/reference/artifact-boundaries.md) and
[secret policy](../../../docs/reference/secret-boundaries.md).
