# YAML ledger adapter

`yaml-ledger-v1` indexes authoritative YAML. The read path is [yaml_ledger.rs](../../crates/awr-source/src/yaml_ledger.rs); approved exact record updates use [yaml_mutation.rs](../../crates/awr-source/src/yaml_mutation.rs). Parsing uses [serde_yaml_ng 0.10](https://docs.rs/serde_yaml_ng/0.10.0/serde_yaml_ng/) and write boundaries use [yaml-rust2 0.12 parser events](https://docs.rs/yaml-rust2/0.12.0/yaml_rust2/parser/enum.Event.html).

- `work_items`, `milestones` and `goals` accept lists with `id`/`external_key`, or maps keyed by the external key. Duplicate or conflicting keys fail explicitly.
- Work status retains its exact `raw_status`; unknown values remain `unknown` with a diagnostic. Missing status never becomes ready. `required_for_v1`, owner, priority, acceptance, next action, blocker, tags and paths/deliverables are retained.
- Milestones project to `Plan` with `kind: milestone`; their status, scope, summary and acceptance remain available. Work `milestone` creates a `part_of` relationship. `goal`/`goals` create `supports` relationships.
- `depends_on`/`dependencies` accept a list of work keys, or `{id/key, required}` records. Missing external targets are retained for graph diagnostics; they are not fabricated as completed work.
- `evidence` accepts locator strings or `{locator/path, summary}` records. These are unverified references (`source_reference`, level `unknown`), linked to the work item. Reading a reference does not prove its contents or promote a validation level.
- Every object and relationship retains an exact JSON pointer into the source and the indexed fingerprint/revision. Repeat indexing of an unchanged, fresh source is a no-op; previous entity IDs survive changes.

Custom field mapping options are currently rejected explicitly. Source runtime state, claims and events cannot be imported as work history. User-facing source commands arrive with the source CLI ledger item; the development example can index a local ledger now:

```sh
cargo run -p awr-source --example index_yaml -- . ledger/work-ledger.yaml .local/intake.db
```

Create the output directory first. The example reads the named project files and stores derived facts in the provided database. It does not modify source files.

## Approved local file updates

Use `awr proposal create`, `submit`, `approve`, then `apply` as described in the [CLI reference](../../README.md). The immutable binding includes the exact JSON pointer and original file fingerprint. A patch names source fields directly; aliases are not translated silently and conflicting aliases fail validation.

| Target | Supported field replacements |
| --- | --- |
| Goal | `title`, `status`, `priority`, `summary`, `success_criteria` / `acceptance` |
| Milestone / Plan | `title` / `name`, `status`, `summary`, `scope`, `acceptance` / `success_criteria` |
| Work | `title`, `kind`, `priority`, `required` / `required_for_v1`, `summary`, `next_action`, `score`, `tags`, `paths` / `deliverables`, `acceptance`, `milestone`, `depends_on` / `dependencies`, `goal` / `goals` |
| Structured source evidence | `summary` only; evidence identity, provenance and verification level are preserved |

The writer supports ordinary list and keyed-map records, including flow mappings, Unicode and CRLF files. It serializes the selected mapping as JSON flow syntax, which is valid YAML. Whitespace, comments and scalar formatting inside that record can change; bytes outside the record remain unchanged. Standalone trailing comments remain outside the replacement. The complete resulting document must have exactly the proposed semantic changes and must reparse through the same projection adapter before any write is attempted.

Git sources, scalar evidence, aliased targets, anchored/tagged target nodes, complex mapping keys and unsupported fields return `proposal_required`. Generic work field updates cannot change status, owner, blocker, verification or evidence lists. Validated `work progress/block/unblock/cancel/reopen` proposals can change their exact authorized status/blocker/next-action fields (and progress summary); the immutable transition and current runtime/domain conditions are checked before writing. `work complete` additionally binds every acceptance criterion to current registered evidence and checks the actual version 1 report files. It preserves existing evidence references and arbitrary verification metadata, appends selected report locators, and sets `verification.evidence_level` to the minimum supported level across selected records. If a top-level `evidence_level` already exists, both aliases are updated consistently. A manually constructed completion patch must match these preserving changes exactly; source-reference namespace collisions fail before writing. The completed projection is verified before recording `work.completed` and releasing the creating session's claim. See the [completion input and report contract](../../README.md) for report fields and limits. Invalid values and no-op patches fail explicitly. Project runtime files under `.awr` cannot be mutation targets. Source and recovery snapshots are capped at 16 MiB.

Application keeps `before.yaml`, `after.yaml` and `plan.json` under `.awr/mutations/<write-plan-id>/`, with snapshot sizes, fingerprints and target fact hash in the immutable journal. It checks the current file immediately before atomic replacement, preserves file permissions, reparses and commits the source projection, and checks actual bytes again before finalization. AWR writers share a per-source advisory lock. Interrupted attempts can be continued with `proposal recover`; a current file matching neither snapshot is retained untouched for manual reconciliation. These component checks do not represent full fault-campaign or real-client acceptance.
