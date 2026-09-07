# A small AWR source project

Copy this directory to a scratch project and point `awr --project` at the copy. The following commands are implemented:

```sh
awr --project /path/to/copy init --manifest project.toml
awr --project /path/to/copy init --manifest project.toml --accept
awr --project /path/to/copy source list
awr --project /path/to/copy source scan
awr --project /path/to/copy source reindex
```

The first command previews the explicit authority mapping without changing files. Acceptance creates `.awr/project.toml`, initializes `.awr/state.db`, indexes the sources and adds missing runtime ignore entries. An existing matching manifest is reused without rewriting it. A conflicting manifest is rejected.

Edit the copied work ledger, then scan: the changed source becomes stale/pending while its old projection remains available for diagnosis. Reindex to commit new facts. Repeating reindex with no changes leaves revisions unchanged. `--json` returns structured reports; incomplete source operations exit nonzero.

Only source intake and diagnostics are implemented at this milestone. Work/session/context actions are tracked in the remaining V1 ledger. This example is a demonstration fixture and does not constitute real business acceptance.
