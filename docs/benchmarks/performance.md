# Local CLI latency benchmark

`tests/benchmarks/performance/contract.json` defines the P9-005 protocol against
the same frozen real historical sample used for the large-ledger and context
benchmarks. All original files remain read-only; each operation uses its own
disposable project. The ledger is the only completion/metric authority.

Use an optimized, locked release build. The timed interval includes a fresh CLI
process, the installed `rtk proxy` wrapper, source freshness checks, execution,
stdout capture and process exit. Nothing is subtracted. JSON decoding of the
captured result and correctness checks are outside the interval. Each sample
must return the expected source facts; a fast empty or failed response does not
qualify. The ready query's actual candidate count is reported even when empty.

After initialization and baseline status/full-work identity checks, report the
first operation measurement separately, perform three
additional warmups, then retain all 30 sequential measured calls. The p95 is
nearest-rank: sorted sample `ceil(0.95 * N) - 1`. Contract 1.0.1 requires each
of status, work show, ready, context compile, FTS and incremental reindex to stay
below 1,000 ms. Any admission overflow remains visible and prevents a pass. No outlier
is discarded, and no repeated run replaces a recorded failure.

File and SQLite pages become warm through normal preceding operations. Every
call still starts a new process and initializes its own tokenizer when needed.
The first observed call is not proof of a physically cold filesystem: OS caches
are not flushed, and host services, power settings and background work are not
changed for the benchmark. Setup-call durations are also reported. Hardware, OS build, Rust/Cargo, wrapper version,
source bytes and database row counts are recorded. This local host result does
not certify all supported platforms or maximum design capacity.

Incremental reindex processes 34 distinct real edits to one selected work's
`next_action` on its disposable copy: first call, warmups and measured samples.
Each edit starts from the original canonical document and adds a unique iteration
note. Writing that source and verifying the resulting projection are untimed.
Each resulting work revision must advance exactly once, all other task revisions
remain unchanged, and the final source differs only in that selected next action.
An unchanged no-op reindex cannot satisfy this operation.

After committing the benchmark inputs, run:

```sh
cargo build --locked --release --workspace --bins
.venv/bin/python -m unittest discover -s tests/benchmarks/performance -p 'test_*.py'
.venv/bin/python tests/benchmarks/performance/verify.py \
  --awr target/release/awr --prepared .local/large-ledger/prepared-v6 \
  --output .local/performance-benchmark/run-v1
```

Choose a new output directory for every run. Private `report.json` binds every
raw command receipt and timing; `public-summary.json` includes all measured and
warmup durations, hardware, counts and hashes. Source/backend/model commands
from the original project are never executed. These measurements do not grant
MCP latency, E4, model-client activation or release credit.

## User-directed V1 acceptance adjustment

The original 1.0.0 run measured p95 values of 50.2355 / 43.3085 / 48.834833 /
118.48125 / 43.391208 / 143.9205 ms, in the operation order above. It met three
of the original six strict targets and remains recorded as such.

On 2026-09-09 the user accepted this millisecond-scale response time for V1.
The active scope contract and this benchmark advance to 1.0.1: all six V1
latency gates are `<1000 ms`. The original 20/20/50/100/100/500 ms numbers remain
explicit optimization targets, with their own pass/fail fields. No runtime
optimization was applied in response to this adjustment. Workload, sampling,
startup accounting, correctness checks and all other V1 gates stay unchanged.
The old run is retained in `ledger/evidence/AWR-P9-005/baseline.json`.

## Measured V1 admission result

On 2026-09-09, source `20d3cad172f8c0bfd54f59a561c8e3775febddd2`
passed all seven gates through 256 CLI calls on an Apple M3 Max / macOS 26.5.2
host. The sample contains 150 work items, 38 ready candidates and 249,449 ledger
bytes. Each row below contains 30 retained primary measurements.

| Operation | p95 (ms) | V1 limit (ms) |
| --- | ---: | ---: |
| Status | 44.916042 | <1000 |
| Work show | 56.282834 | <1000 |
| Ready | 50.211834 | <1000 |
| Context compile | 121.225958 | <1000 |
| FTS search | 33.817084 | <1000 |
| Incremental reindex | 135.136250 | <1000 |

`ledger/evidence/AWR-P9-005/benchmark.json` retains every timing, the build and
source hashes, hardware details, correctness gates and original optimization
target results. The ledger records completion only after the remote evidence
receipt is bound. This result establishes local CLI latency for this workload;
actual-client business acceptance remains a separate gate.
