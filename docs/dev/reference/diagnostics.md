# Source diagnostics and evidence presentation

YAML syntax and typed ledger-field failures return the existing `InvalidInput`
code with structured `details`:

```json
{
  "location": {
    "locator": "file:///project/work.yaml",
    "pointer": "/work_items/0/title",
    "line": 3,
    "column": 10
  },
  "rule": "ledger.string",
  "repair": "Use a quoted string or a YAML block scalar (|) for multiline text."
}
```

Line and column are one-based. Field pointers use the original source vocabulary,
including configured Chinese names, and JSON Pointer escaping. Syntax failures
may only have parser coordinates. Unsupported location lookup, such as YAML
aliases, retains the field pointer with null coordinates; it does not reject an
otherwise valid source. Diagnostics do not include the rejected value or a raw
source excerpt. Repair text describes the expected structure and does not edit it.

For example, `title: [Draft, Review]` is a list where a string is required. Use
`title: "Draft, Review"` if that is the intended title. Long descriptions can use
YAML block scalars; valid Chinese text and paths with spaces remain supported.
Keep the work record focused on intent, acceptance and current progress; reference
design documents and reports when their bodies are not needed in every query.

`awr source reindex` prints operation success, projection completeness, source
location, rule and repair from the same index report serialized by `--json` and
returned by MCP `awr_source_reindex`. A successful `source scan` can still have an
incomplete projection: scanning observes sources without importing their changed
facts. Check both fields. Compare alternate output formats from equivalent
starting snapshots because reindexing itself updates source state.

`work show` and MCP `awr_work_get` include `evidence_groups` alongside the existing
flat `evidence` list. Each group has one exact locator and separate records with
their own source SHA, digest, scope, branch, evidence level, currency and missing
bindings. Human output prints these groups. A source reference and a verified
report at the same path remain distinct; grouping never transfers verification,
resolves path aliases or changes completion checks.
