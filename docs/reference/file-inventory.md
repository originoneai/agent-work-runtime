# File inventory for host source freshness

`awr source inventory` lets a host compare project files that are outside AWR's
registered source mappings. It is read-only and does not open or create an AWR
database. It does not replace `source scan` or `source reindex` for registered
sources.

Select project-relative files or directories, and explicitly exclude generated
or sensitive paths before saving the baseline:

```sh
awr --project /path/to/project --json source inventory \
  --include docs --include tooling/awr \
  --exclude-glob 'docs/awr/generated/**' \
  --exclude-glob '**/.env*' > /path/to/project/.awr/source-inventory.json
```

Compare the same selection with the saved baseline:

```sh
awr --project /path/to/project --json source inventory \
  --include docs --include tooling/awr \
  --exclude-glob 'docs/awr/generated/**' \
  --exclude-glob '**/.env*' \
  --baseline .awr/source-inventory.json
```

The snapshot contains relative paths, per-file SHA-256 hashes, and a digest of
the selection and file map. A host should store it locally and protect the saved
baseline from unrelated edits. Comparison verifies the baseline digest and rejects
a changed selection policy. The JSON result has `fresh`, `total_changes`, up to
12 relative `changes` with `added`, `missing`, or `changed` kinds, and
`omitted_changes`. The comparison result contains no file content or hashes.
Hosts must treat `fresh: false` as stale even though the command itself succeeds.

The inventory always ignores `.DS_Store`, `.git`, and `.awr` at any depth.
Exclusion globs are case-insensitive; use them to keep secrets, build outputs,
and generated files out of the snapshot. Symbolic links are rejected rather than
followed. The command limits the selection to 64 includes, 64 exclusions,
20,000 files, and 1 GiB of file content. A host must still guard its runtime
binding, generated projections, and code inventory according to its own contract.

This command is available in development on `main`; the published 0.5.0 binary
does not include it.
