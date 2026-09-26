# Calibrated acceptance ETA and stage checkpoints (WS-043)

WS-043 estimates the **next acceptable outcome** for a delivery or stage
checkpoint. Forecasts are stored separately from WS-041 measured usage/time
observations and are **append-only**: historical rows are never rewritten.

## Surfaces

- Core: `awr_core::workstream_eta` — critical-path / concurrency schedule,
  forecast records, calibration gate, reestimate links, sample/acceptance
  isolation.
- Store (schema 14 / `eta_checkpoints`): append-only `eta_forecasts`, isolated
  `eta_sample_ledger` and `eta_acceptance_data`, idempotent ingest receipts.
- Runtime: `estimate_and_persist`, `reestimate_and_persist`, query helpers, and
  `observation_handoff_for_estimate` (consumes WS-041 handoff without treating
  cumulative duration as remaining ETA).

## Invariants

1. Forecast records capture target, `generated_at`, task-graph version,
   execution strategy, sample/method versions, intervals, assumptions, and
   unknowns. Persistence refuses in-place rewrites.
2. Scheduling uses dependency critical path, real concurrency limit, and
   available executors. Parallel task durations are never summed into the
   checkpoint ready time. A card waits only on its dependency closure, not on
   every mainline.
3. Effective execution, dependency/human wait, and calendar acceptance window
   are separate components. Network/model queue exclusions require an
   observable `evidence_ref`; missing components stay `Unknown`.
4. Cold start may be `Provisional { source }` or `Unestimable { reason }`.
   `Calibrated` intervals require the frozen sample + holdout policy gate.
   Reports coverage (bps), interval width, absolute error, and missing rate.
   LLM narrative may be recorded as an assumption, never as a precise promise.
5. New dependency, rework, executor change, or capacity change triggers a new
   forecast that supersedes the prior id with before/after reasons. Historical
   samples stay isolated from acceptance data.

Fixtures: `tests/fixtures/workstreams/eta-checkpoints/`.
