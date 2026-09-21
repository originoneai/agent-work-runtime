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

Historical `real_agent_accepted` records (42 cases) and their evidence references
are preserved independently of automated coverage. This correction does not
rerun or revalidate those Agent sessions. The remaining 27 records are
`automated_evidence_pending`, not an assertion that they cannot be replayed.
The matrix guard checks references and accounting, not business acceptance.

A live dual-client run used **Kimi Code CLI 2.0.0** and **ZCode CLI 0.16.5**
on one dedicated Team project (`p11-live` / `work-p11`): claim, implementation,
handoff, verification, human review, completion. An independent oracle checked
one receipt, the successor holder, and a single execution. That run is recorded
as `live_agent_run`, separately from the later per-case records. Neither the
matrix nor its structural guard authorizes a release tag.

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
