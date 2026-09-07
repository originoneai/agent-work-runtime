# Markdown heading adapter

`markdown-heading-v1` projects `goal` or `plan` sources using [pulldown-cmark](https://docs.rs/pulldown-cmark/0.13.4/pulldown_cmark/). ATX and Setext headings, Unicode, inline formatting, CRLF, and explicit `{#anchor}` identifiers are supported. Headings inside code, quotes or lists do not split the authoritative sections.

Each heading owns the content up to the next top-level heading, retaining its title and original body, one-based inclusive line range, source fingerprint, section fingerprint and anchor. Prelude text is retained as a `preamble` section. The body becomes Goal/Plan summary; arbitrary prose is not guessed to be structured acceptance criteria. Missing status remains `unknown`.

Keys default to `<registered source locator>#<anchor>`; `sources.options.key_prefix` can provide a project-facing prefix. Duplicate generated anchors gain `-2`, `-3`, etc.; use explicit anchors to keep identity stable when reorganizing duplicate headings. Duplicate explicit anchors fail.

Source options can specify `status` and `priority`; matching heading attributes override the defaults. The source remains read-only and mutation planning returns `MutationUnsupported`.

The developer `index_source` example accepts a root, manifest path, source index, and output database. It supports content reindexing; tracking manifest option changes in an existing database belongs to the incremental indexing item. Use a fresh scratch database for configuration experiments until that orchestration is delivered.
