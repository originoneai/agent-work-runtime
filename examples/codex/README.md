# Codex integration examples

Use the [integration guide](../../docs/integrations/codex.md) for the workflow on
your actual project. These files support that guide:

- `config.toml.example`: project-bound stdio MCP configuration. Replace absolute
  paths and merge the table into an existing trusted project configuration.
- `AGENTS.snippet.md`: instructions to adapt into the project's existing agent
  guidance. Copying it does not install lifecycle hooks.
- `lifecycle.py`: an executable CLI walkthrough using a fresh copy of
  [the basic fixture](../basic/) with this directory's `work-ledger.yaml`, which
  declares the phase required by bootstrap. It retains a receipt for every command.

From the AWR repository root, with Python 3.9 or later and the Rust toolchain:

```sh
cargo build --locked -p awr-cli -p awr-mcp
python3 examples/codex/lifecycle.py \
  --awr target/debug/awr \
  --output .local/codex-lifecycle-example
```

Choose a new output directory for each run. An existing directory is rejected;
the script never overwrites or cleans an earlier run. The example executes
init preview/accept, session start with a claim, bootstrap, L1 compile,
checkpoint, resume, recovered-context inspection, session end and Doctor. It
checks that the checkpoint's next action/open loop survive and that resume
preserves the claim expiration. The copied source files remain unchanged; the
example ends as incomplete and releases the claim.

On failure, inspect the numbered JSON receipt and the retained project's
`session list`, `session show` and event history. The script does not retry a
mutation or discard a partly completed run. Runtime files and receipts are local
inspection material, not source-ledger completion evidence.

This script invokes AWR, not Codex or a model. Provider/model values are recorded
metadata. Running it from a Codex terminal establishes a terminal workflow;
native MCP discovery, lifecycle hooks and real business acceptance require their
own evidence. See the guide's dated verification boundary.
