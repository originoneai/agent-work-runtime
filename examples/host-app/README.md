# No-UI host example

This standard-library Python 3.11+ example calls a pinned native AWR executable by
absolute path and argv. It uses no shell, model, daemon, installed hooks or private
SQLite API. Python runs the example host; it is not an AWR end-user dependency.

Supply the checksum and version from your trusted build/release receipt. The example
does not treat a checksum computed from an unknown download as release verification.
Keep the executable in a host-controlled location; checksum checks are not an OS
execution sandbox or protection against simultaneous replacement by a privileged user.

```text
python3 examples/host-app/host.py --binary /absolute/path/to/awr --sha256 <trusted-sha256> --version <version> demo --directory '/private/tmp/示例 项目'
python3 examples/host-app/host.py --binary /absolute/path/to/awr --sha256 <trusted-sha256> --version <version> inspect --project '/absolute/project path' --receipts '/private/path/new-receipts'
```

`demo` requires a new directory and copies a synthetic 25-work fixture. It previews
and accepts exact intake effects, traverses multiple catalog pages, binds a generic
conversation, checks compiled context, reports external progress, saves/reads a
checkpoint, deduplicates a repeated callback, inspects recovery and explicitly binds
a successor conversation. It checks original ledger bytes and ends fixture sessions.
The synthetic driver is not Kimi/Grok, a model receipt or complete business acceptance.

`inspect` traverses the current project's complete work catalog and returns aggregate
counts. It never edits authoritative source files; query-driven projection refresh
can write AWR runtime state. It does not create a client binding or claim.

Raw stdout, stderr, invocation and outcome receipts stay in the caller's NEW private
directory (POSIX 0700 with files 0600). On Windows, place it beneath a directory with
appropriate user ACLs. Never register this directory as a source or publish it. JSON
input uses protected files or stdin. Each command is invoked once; nonzero exits keep
partial results and typed errors. A timeout, host crash or lost response means an
unknown write outcome: inspect the pending receipt and the documented AWR status
operation before deciding whether to retry. No automatic write retry occurs.

`Host.catalog` fails the whole traversal on source failure, revision mismatch or
missing/duplicate objects. Keep the last complete UI snapshot separately; do not
present partial traversal as a confirmed empty project. Field shapes remain the
command-specific contract in [host-contract](../../docs/reference/host-contract.md).

`cargo test -p awr-cli --test host_app` exercises the example on a temporary Chinese
path containing spaces, with no TTY. It requires Python 3.11+ on the development/CI
machine. The test also checks binary pin rejection, partial errors, and unknown
timeout outcomes without replaying commands.
