# Usage and time observation handoff (WS-041 → WS-043)

WS-041 records **historical** usage and wall-clock observations. WS-043 may
consume them as covered history for forecasting, but must not treat any
cumulative-duration field as estimated remaining time.

## Surfaces

- Core: `awr_core::workstream_usage` — dedup, cost totals, counter deltas,
  allocations, coverage reports, corrections, `usage_observation_handoff`,
  `refuse_eta_from_cumulative_duration`.
- Store (schema 13 / `usage_time`): persisted receipts, corrections, allocation
  records, counter snapshots, execution intervals, idempotent ingest receipts.
- Runtime: attested ingest/query APIs and `usage_observation_for_ws043`.

## Invariants for WS-043

1. `UsageTimeObservationHandoff.is_historical_observation` is always true.
2. `is_not_estimated_remaining_time` is always true; refuse ETA labels on
   cumulative duration via `refuse_eta_from_cumulative_duration`.
3. Coverage (`call_coverage` / `time_coverage`) is optional only when the
   expected denominator is known; unknown expected never invents 100% coverage.
4. Actual, API-equivalent, and unknown costs remain separate from coverage.
5. Parallel `observed_wall_clock_ms` is not equal to `observed_execution_ms`
   when intervals overlap.

This note is a handoff contract, not a forecasting implementation.

See also `eta-checkpoints.md` for the WS-043 forecasting surface that
consumes this handoff.
