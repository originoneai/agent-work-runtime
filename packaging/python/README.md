# Agent Work Runtime

Persistent work state and minimal context for long-running AI agents.

This is a development preview, not the completed V1 release. The wheel includes
the Rust `awr` CLI and `awr-mcp` stdio server. It provides command-line tools,
not a Python SDK or an AI model service.

After a release is published:

```sh
pip install --pre agent-work-runtime
awr --version
awr --help
awr-mcp --help
```

For an isolated application installation, use `pipx install --pip-args=--pre
agent-work-runtime` or `uv tool install agent-work-runtime --prerelease allow`.

The project must be initialized before running
`awr-mcp --project /absolute/project`. Follow the
[project documentation](https://github.com/originoneai/agent-work-runtime).

The distribution targets macOS 15+ arm64, Linux x64 with glibc 2.39+ (Ubuntu 24.04
baseline), and Windows x64. Python 3.9+ is required for the launcher. Git is required
for Git-bound work branch operations. SQLite is included in the binaries. Supported
platforms install a prebuilt wheel and do not need a Rust compiler.

The wheel includes source identity, binary checksums and third-party license
notices. The core program is licensed under Apache-2.0.
