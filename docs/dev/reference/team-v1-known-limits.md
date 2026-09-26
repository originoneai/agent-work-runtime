# Team V1 known limits

The matrix inventories 69 required scenarios; it does **not** establish full
PostgreSQL coverage of all 69. Each automated reference identifies a Cargo
package/target, test function, required features, assertion scope, coverage layer
and reviewed source SHA plus a checked LF-normalized test-file fingerprint. The current mappings are deliberately partial: a
reference resolves to source, but `last_run: null` supplies no case-level run
receipt. `real_postgresql` describes the test layer, not a successful execution.
Counts are derived from individual records and checked by `pg_matrix`.

TC-060 now references event pagination in `pg_read`, which does not establish
snapshot-to-delta continuity. TC-069 references shared admission-function labels,
not three live transports. Authenticated Team HTTP/MCP transport and domain
command dispatch remain unimplemented. These two cases do not contribute to the
implementation count; the other implementation flags remain historical claims,
not proof of complete coverage. Every mapping includes its evidence gap.

Historical `real_agent_accepted` records (42 cases) are preserved independently
of automated coverage. Their structured references resolve through the sealed
`team-v1-historical-agent-evidence-v1.json` index, which binds each case
definition to its retained summary fingerprint. All 42 are
`historical_pending_review`; `current_verified` is zero. Source and driver
versions bound to the run, run identity and time, participant identities,
explicit oracle expectations and an independent oracle identity remain unknown.
Summary hash agreement locates retained bytes but does not revalidate the
reported business result. Eight summaries also reference temporary outputs that
were no longer retained; the index identifies those cases.

This correction did not rerun or revalidate those Agent sessions. The remaining
27 records are `automated_evidence_pending`, not an assertion that they cannot be
replayed. The v1 historical index is sealed: adding or promoting an acceptance
requires a new versioned evidence contract with complete bindings. The matrix
guard checks references and accounting, not business acceptance.

A live dual-client run used **Kimi Code CLI 2.0.0** and **ZCode CLI 0.16.5**
on one dedicated Team project (`p11-live` / `work-p11`): claim, implementation,
handoff, verification, human review, completion. An independent oracle checked
one receipt, the successor holder, and a single execution. That run is recorded
as `live_agent_run`, separately from the later per-case records. The versioned
matrix guard checks this summary independently: it requires the Kimi-to-ZCode
client order, two distinct non-empty actors, the successor as active holder,
exactly one receipt and one execution, a successful oracle, complete locators,
and explicit release limits. It accepts new well-formed identifiers, client
versions and Git SHAs; it does not treat the historical literal values as the
only valid run.

A later read-only inspection observed the retained evidence bundle at
`2026-09-21T14:33:31.256628+00:00`. Its SHA-256 was
`5257cc18c8e0ff13ed683abe569cefb65318c7361e728c8a1252c9beab4a17ec`;
the currently retained driver had SHA-256
`81fa132d0d5ae61fcaaaed39a8f4021649d0b9702c83adc870dd7c7b3c0025d0`.
The bundle's oracle, receipt, execution and Git-head metadata agreed with the
public summary, and its three embedded artifact byte hashes matched. The
retained driver is not immutably bound to the historical execution, no explicit
run timestamp is available, and the full historical chain was not reverified.

This correction did not rerun either live client or query author-private logs.
Summary consistency is computed by the Rust `pg_matrix` guard;
`current_verification.live_rerun` only records that no live rerun occurred in
this correction. Neither the matrix nor its structural guard authorizes a
release tag.

## Capacity and reconnect probes

`pg_capacity` uses the shared fixture: only `AWR_TEAM_TEST_DATABASE_URL` is read,
only loopback targets are allowed, and a newly allocated database belongs to this
test process. It never reads the runtime `AWR_TEAM_DATABASE_URL`. Use disposable
PostgreSQL instances; the fixture creates databases and an application role.

The capacity metric is `combined_workflows_per_second`: each of 20 serial
workflows inserts a work item, starts a session and claims it. It is not isolated
claim latency, enterprise throughput or a production SLA. Setup and version
lookup are outside the timed interval. Successful output is visible with:

```sh
cargo test -p awr-team-pg --features pg-tests --test pg_capacity -- --nocapture
```

To persist JSON, set `AWR_CAPACITY_REPORT` to a new file, `AWR_TEST_SOURCE_SHA`
to the measured 40-character commit, and `AWR_TEST_SOURCE_DIRTY` to `true` or
`false`. Preserve a diff alongside a dirty measurement. The report records the
scope, elapsed seconds, rate, PostgreSQL version, OS/architecture and timestamp;
it excludes connection URLs and credentials and refuses to overwrite a report.
Without a report path, stdout/stderr must be retained explicitly.

`pg_fault` separates healthy reconnection from incompatible-version and missing
migration-record controls. The latter assert schema rejection and unchanged
work-item bytes in the process-owned database; they do not simulate every
network outage or claim production HA.
