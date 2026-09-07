# Markdown rules adapter

`markdown-rules-v1` retains every nonempty source section as a traceable Rule. It preserves the title and body plus the source anchor, line range and both fingerprints. It is read-only.

Rule semantics must be explicit. A heading can supply metadata:

```markdown
## Source authority {#source-authority severity=hard scope=project value=*}

Keep the project files authoritative.
```

Supported severity values are `hard`, `soft`, `info`; scope types are `project`, `path`, `tag`, `work_item`, `agent`, with a nonempty explicit `value`. The same keys can be set as source-wide defaults under `[sources.options]`; heading attributes take precedence. Defaults are parsing configuration, not copied rule body.

Missing or unsupported metadata yields `None` and an `unresolved` diagnostic, also reported in batch warnings. No fallback silently weakens a rule to soft/info or broadens its scope. Natural-language wording alone does not determine severity. Heading-only rule text remains present rather than being discarded.

The current project's unannotated RULES.md intentionally produces unresolved metadata until an explicit authority mapping is configured for runtime use. Complete rule selection and context completeness checks are tracked in later ledger items.
