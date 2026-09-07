# YAML ledger adapter

`yaml-ledger-v1` reads authoritative YAML without writing it. Implementation: [yaml_ledger.rs](../../crates/awr-source/src/yaml_ledger.rs). It uses [serde_yaml_ng 0.10](https://docs.rs/serde_yaml_ng/0.10.0/serde_yaml_ng/).

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
