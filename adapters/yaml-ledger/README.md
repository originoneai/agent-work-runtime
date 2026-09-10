# YAML ledger adapter

`yaml-ledger-v1` indexes authoritative YAML. The read path is [yaml_ledger.rs](../../crates/awr-source/src/yaml_ledger.rs); approved exact record updates use [yaml_mutation.rs](../../crates/awr-source/src/yaml_mutation.rs). Parsing uses [serde_yaml_ng 0.10](https://docs.rs/serde_yaml_ng/0.10.0/serde_yaml_ng/) and write boundaries use [yaml-rust2 0.12 parser events](https://docs.rs/yaml-rust2/0.12.0/yaml_rust2/parser/enum.Event.html).

- `work_items`, `milestones` and `goals` accept lists with `id`/`external_key`, or maps keyed by the external key. Duplicate or conflicting keys fail explicitly.
- Work status retains its exact `raw_status`; unknown values remain `unknown` with a diagnostic. Missing status never becomes ready. `required_for_v1`, owner, priority, acceptance, next action, blocker, tags and paths/deliverables are retained.
- Milestones project to `Plan` with `kind: milestone`; their status, scope, summary and acceptance remain available. Work `milestone` creates a `part_of` relationship. `goal`/`goals` create `supports` relationships.
- `depends_on`/`dependencies` accept a list of work keys, or `{id/key, required}` records. Missing external targets are retained for graph diagnostics; they are not fabricated as completed work.
- `evidence` accepts locator strings or `{locator/path, summary}` records. These are unverified references (`source_reference`, level `unknown`), linked to the work item. Reading a reference does not prove its contents or promote a validation level.
- Every object and relationship retains an exact JSON pointer into the source and the indexed fingerprint/revision. Repeat indexing of an unchanged, fresh source is a no-op; previous entity IDs survive changes.

Work fields and statuses can be mapped without editing the authoritative file. In the relevant `[[sources]]` entry:

```toml
[sources.options.field_map]
id = "ticket"
title = "name"
status = "phase"
next_action = "next"
[sources.options.status_map]
Pending = "planned"
Doing = "in_progress"
Done = "completed"
```

`field_map` is canonical field → original YAML key. Supported work fields are `id`, `title`, `status`, `kind`, `owner`, `priority`, `required`, `summary`, `next_action`, `blocker`, `acceptance`, `depends_on`, `goal`, `milestone`, `tags`, and `paths`. Root collections remain `work_items`, `goals`, and `milestones`; this is not an arbitrary schema transformation engine. A mapped field and a competing canonical source key cannot coexist. Existing built-in aliases remain subject to conflict checks. Evidence/verification metadata cannot be renamed through this mapping.

`status_map` is original status → canonical status. Lookup ignores surrounding whitespace and case, while `raw_status` retains the source value. Unknown values stay unknown until explicitly mapped; canonical states cannot be redefined. Initialization flags `--status-map pending=planned` and `--field-map title=name` populate the reviewable source configuration. On initialized projects, edit `.awr/project.toml` and run `awr source reindex`. Options are bound to source revisions and mutation proposals.

Source runtime state, claims and events cannot be imported as work history. The development example can index a local ledger:

```sh
cargo run -p awr-source --example index_yaml -- . tests/fixtures/yaml-ledger/ledger.yaml .local/intake.db
```

Create the output directory first. The example reads the named project files and stores derived facts in the provided database. It does not modify source files.

## Approved local file updates

Use `awr proposal create`, `submit`, `approve`, then `apply` as described in the [CLI reference](../../README.md). The immutable binding includes the exact JSON pointer, file fingerprint and mapping configuration. Work patches name canonical fields; configured mappings write the original keys. Status writes preserve the current spelling when its meaning is unchanged, otherwise use the sole configured spelling for the target state, falling back to a canonical status only when none is configured. Multiple configured target spellings require resolving the mapping or editing the source explicitly. Source reads remain possible in that case. Mappings do not bypass domain actions or completion checks.

| Target | Supported field replacements |
| --- | --- |
| Goal | `title`, `status`, `priority`, `summary`, `success_criteria` / `acceptance` |
| Milestone / Plan | `title` / `name`, `status`, `summary`, `scope`, `acceptance` / `success_criteria` |
| Work | `title`, `kind`, `priority`, `required` / `required_for_v1`, `summary`, `next_action`, `score`, `tags`, `paths` / `deliverables`, `acceptance`, `milestone`, `depends_on` / `dependencies`, `goal` / `goals` |
| Structured source evidence | `summary` only; evidence identity, provenance and verification level are preserved |

The writer supports ordinary list and keyed-map records, including flow mappings,
Unicode and CRLF files. It locates the changed fields using parser events and changes
only their value spans; new fields are appended without reordering existing keys.
Unchanged fields, inline/standalone comments, and surrounding bytes retain their exact
representation. Plain, single-quoted, double-quoted, literal and folded scalars are
supported. Quotes and block style remain when representable; plain strings that would
become another YAML type are quoted, and block chomping follows the new trailing breaks.
Existing indentation and LF/CRLF are retained. Collection replacement is one field edit;
comment-bearing collections are conservatively rejected instead of losing comments.
Multiline plain target scalars, unusual single-quoted whitespace/line breaks and block
headers separated from their key by a comment require explicit manual editing.
Unrelated multiline plain scalars retain their original bytes.

Anchors, aliases, tags, merge/duplicate/complex keys and multiple documents are rejected
for automatic field writes. These forms may remain readable. The complete resulting
document must have exactly the proposed semantic changes and reparse through the same
projection adapter before any write is attempted. There is no whole-record serialization
fallback when field-level preservation cannot be proved.

Git sources, scalar evidence, aliased targets, anchored/tagged target nodes, complex mapping keys and unsupported fields return `proposal_required`. Generic work field updates cannot change status, owner, blocker, verification or evidence lists. Validated `work progress/block/unblock/cancel/reopen` proposals can change their exact authorized status/blocker/next-action fields (and progress summary); the immutable transition and current runtime/domain conditions are checked before writing. `work complete` additionally binds every acceptance criterion to current registered evidence and checks the actual version 1 report files. It preserves existing evidence references and arbitrary verification metadata, appends selected report locators, and sets `verification.evidence_level` to the minimum supported level across selected records. If a top-level `evidence_level` already exists, both aliases are updated consistently. A manually constructed completion patch must match these preserving changes exactly; source-reference namespace collisions fail before writing. The completed projection is verified before recording `work.completed` and releasing the creating session's claim. See the [completion input and report contract](../../README.md) for report fields and limits. Invalid values and no-op patches fail explicitly. Project runtime files under `.awr` cannot be mutation targets. Source and recovery snapshots are capped at 16 MiB.

Application keeps `before.yaml`, `after.yaml` and `plan.json` under `.awr/mutations/<write-plan-id>/`, with snapshot sizes, fingerprints and target fact hash in the immutable journal. It checks the current file immediately before atomic replacement, preserves file permissions, reparses and commits the source projection, and checks actual bytes again before finalization. AWR writers share a per-source advisory lock. Interrupted attempts can be continued with `proposal recover`; a current file matching neither snapshot is retained untouched for manual reconciliation. These component checks do not represent full fault-campaign or real-client acceptance.
