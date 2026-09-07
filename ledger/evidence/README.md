# Evidence format

Evidence records are versioned JSON. A completed work item references one or more existing records in its `evidence` list. Planning validation checks metadata and file presence; it cannot prove that an asserted check actually ran or replace independent review.

Required common fields:

- `kind`: preparation, implementation, validation, release, or business_acceptance.
- `repository`: the canonical repository URL.
- `source_commit`: the full commit SHA whose artifacts/checks are evidenced.
- `checked_at`: timestamp of the actual verification.
- `remote_receipt`: matching commit, canonical commit URL and actual verification timestamp.
- `checks`: named checks with `passed: true` and a concrete result.
- `artifacts`: existing repository-relative evidence/delivery files. Large or private raw output stays outside public Git; commit a redacted summary with hashes when appropriate.
- `work_items`: mapping from covered work ID to an `acceptance` list with exact criterion text and its actual pass result.

A later ledger commit can mark work completed by referencing an earlier, already-pushed implementation commit. Evidence must never invent a hash for the commit that contains itself.

Business evidence additionally binds `scenario_id`, distinct `executor` and `reviewer`, every required gate, an independent delivery commit and the client's transcript/artifact references. Gate flags alone do not prove the business loop; the underlying records must support them.

Metric evidence additionally stores measured values under `metrics`, with the fixture, toolchain/tokenizer, hardware, method and raw report references.

All unexecuted work retains `evidence: []`. Do not create placeholder passing evidence to satisfy the checker.
