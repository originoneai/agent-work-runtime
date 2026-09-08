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
nearest-rank: sorted sample `ceil(0.95 * N) - 1`. Compare it strictly against
20/20/50/100/100/500 ms for status, work show, ready, context compile, FTS and
incremental reindex. Any overflow remains visible and prevents a pass. No outlier
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
