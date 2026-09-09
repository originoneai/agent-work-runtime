# Project intake and work continuity

This addition covers four separately verified capabilities: project intake, client checkpoints, execution registration, and recovery inspection. It extends the existing AWR work runtime; it does not transfer arbitrary process memory or reconstruct unrecorded conversations.

## Intake

Run `awr --project /absolute/project init` to inspect the inventory and proposed source mapping. Nothing is initialized until `--accept` is supplied. Conventional YAML sources are retained; Markdown task tables and checklists can be projected with the read-only `markdown-ledger-v1` adapter. Existing Markdown remains the authority and is edited in its original file, then reindexed.

Missing goals, plans, rules and task ledgers are proposed under `.awr/intake/`. These are source files, not disposable runtime state. The first work item establishes the project baseline. Plan headings create planned review tasks; they do not assert that implementation is missing or completed. Existing project files are never overwritten.

For a blank project, state its purpose with `awr init --goal "Deliver a document portal" --accept`. For a project with complex requirements, save a JSON draft with `awr init --write-draft /outside/project/intake.json`. An agent or owner can use the inventory and original documents to replace the generated work with concrete actions, dependencies and acceptance criteria. Apply the reviewed draft with `awr init --from-draft /outside/project/intake.json --accept`. A changed input inventory requires a new review. Generated paths are limited to the four owned intake source files.

The scanner records filenames, sizes, document hashes and observed Git state. It skips hidden/vendor/build directories and symlinks and has explicit resource limits. It does not infer business completion from code filenames or Git commits. Arbitrary spreadsheet/Word/PDF ledgers still require conversion or an explicit adapter; unrecognized states remain unknown.

## Client checkpoints, executions and recovery

Implementation and validation of these three additions are tracked by AWR-TAKE-002 through AWR-TAKE-004. Their command reference and evidence will be added when delivered. Existing manual checkpoint/resume remains available.
