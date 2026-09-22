# Agent review of suspected source content

AWR performs deterministic screening. An Agent or owner inspects the original source locally and decides whether a suspected occurrence is public, should be redacted, or belongs outside AWR. AWR verifies and archives the explicit public decisions. It does not call a model, modify the source, authenticate the named reviewer, or claim to identify every unlabelled secret.

Recognizable credentials, credential-bearing URLs, Bearer/Basic credentials and private-key headers are nonreviewable. A public decision cannot override them. Other detections remain blocked until every current finding has a valid public decision, or the source is corrected and scanned again. There is no global whitelist or scanner-disable option.

## Review and continue

```sh
awr intake review --source docs/protocol.md --write-draft /tmp/protocol-review.json
# Inspect docs/protocol.md locally; edit only the review metadata and decisions.
awr intake review --from-review /tmp/protocol-review.json
awr init --accept
# For an existing project, use source reindex instead of init.
```

The JSON draft contains `project_root`, review `version`, `assessment`, `reviewer`, `reviewed_at`, and `decisions`. Keep the generated assessment unchanged. Set `reviewed_at` to Unix milliseconds and `reviewer` to the accountable Agent/owner identity. Add one entry for every finding:

```json
{"finding_id":"<copy the generated finding ID>","reason":"Explain why this exact occurrence is public."}
```

Use a specific, value-free reason. The report contains category, reviewability, source coordinates, optional decoded JSON/YAML container or Markdown text-fragment ordinals (`decoded_format` identifies the representation), and fingerprints; it does not contain matched values, keys or snippets. A structured location refers to the decoded value, not a guessed source line. Each source is limited to 4 MiB and 512 findings; larger reports fail rather than silently authorizing a truncated finding set. Adapter read limits still apply independently.

If the content is actually sensitive, change the authoritative file yourself to remove it or use explicit placeholders such as `${VAR}` or `[redacted]`, then scan again. AWR never copies a suspected value into the review draft for convenience.

## What a receipt authorizes

A receipt binds the canonical project root, exact file locator, full source-byte SHA-256, detector policy version, finding identities and explicit decisions. Missing, duplicate, stale, cross-project or modified assessments reject. Changing even unrelated source bytes invalidates the old receipt. A different policy requires a new review.

Receipts live under `.awr/content-reviews/` and contain no source body. Publication is atomic and does not overwrite an existing receipt. Repeating exactly the same acceptance is idempotent. A conflicting receipt for the same version is rejected. A crash before publication leaves no usable receipt; a completed receipt can be reused when Init or indexing is retried. Archiving a receipt alone does not initialize the database or mark work complete.

The source adapter rechecks the receipt against its retained bytes before parsing and projection. Projection and its receipt binding commit together in SQLite; the binding includes source fingerprint, adapter/configuration and receipt digest. A failed projection does not grant permission to old or unrelated facts.

Typed source reads, search and context consume the bound source proof. Suspected output fields must correspond to their actual current persisted entity. Rendered context uses each selected chunk's entity provenance and rescans the full output, rejecting credentials and findings that cross the approved chunk boundaries. Reformatting that changes a detected clause is conservatively withheld instead of silently expanding the approval. In this version, multiline private-prompt blocks, flow containers extended by rendering, and JSON values embedded in Markdown comments may still need source restructuring before context rendering succeeds; an archived receipt is not a promise that every adapter transformation is supported. Markdown paragraphs and table cells have explicit decoded-text coverage.

The initial review workflow supports local file sources inside the project, including authored `.awr/intake/` sources. External-root and Git sources retain strict screening. Receipts do not authorize direct runtime/event/checkpoint input, artifact bodies, source mutations, manifests, or another project's content. Editing reviewed sources requires reviewing the new bytes; approval is never a write permission.

## Initialization diagnostics

When source screening blocks `init --write-draft`, the requested path receives a diagnostic-only JSON file with `status: content_review_required`, safe source location and the next action. The command still fails and creates no project database. The diagnostic has no generated source bodies and is not accepted by `init --from-draft`. Existing files are never overwritten. Complete the source review or repair, then generate a fresh intake draft.

## Validation scope

Synthetic regression tests exercise the deterministic scanner, stale/missing/duplicate decisions, nonreviewable credentials, decoded escapes, multiline values, source/project isolation, transaction rollback, database reopening, native CLI intake and MCP context transport. These tests do not establish that an Agent's semantic judgment was correct; the original file remains the authority and the review attribution is caller-supplied.
