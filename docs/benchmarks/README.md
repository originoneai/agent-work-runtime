# Reproduce the public context benchmark

The [summary](summary.json) and [chart](context-tokens.svg) describe a deterministic
**synthetic support-tracker project**, not the AWR development ledger or a customer's
private project. The [fixture generator](../../tests/benchmarks/public/fixture.py)
creates 150 reasonably sized tasks, 39 active and 111 completed, six goals, six
milestones and two hard rules. Thirteen active tasks have unresolved dependencies.
No transcripts, private source documents or repeated log padding are included.

## Measurement

Read each canonical Markdown/YAML file once in sorted order, joining complete UTF-8
contents with one LF. This full-source baseline excludes configuration and runtime
files. Compile L0 (1,000-token budget) and L1 (5,000) for **every active task**, using
the same corpus. Report maximum packet sizes and minimum savings, not a favorable
subset. Count native CLI JSON separately from `rendered_context`.

Python `tiktoken==0.14.0` independently checks Rust's reported rendered counts using
`o200k_base` and `encode_ordinary`. The existing independent source-fact oracle
checks identities, status, next action, each acceptance criterion, blockers, the
exact applicable hard-rule set, and unresolved required dependencies. Every check
must pass. These assertions measure selected source facts, not model comprehension.

Timing uses the first active task, a release binary, three warmups and 30 sequential
calls per operation. Nearest-rank p95 includes native process startup, source refresh,
stdout capture and exit; parsing and file writes are outside the timed interval.
Filesystem/SQLite caches are warm. The summary records CPU, OS, toolchain, input
hashes and binary hash. It does not measure concurrent-agent scaling.

## Run it

Use Python 3.11+ with a virtual environment. From the repository root:

```sh
cargo build --release --locked -p awr-cli
python3 -m venv .venv
.venv/bin/python -m pip install -r tests/benchmarks/public/requirements.txt
.venv/bin/python tests/benchmarks/public/run.py \
  --awr target/release/awr --output .local/public-benchmark-001
.venv/bin/python tests/benchmarks/public/plot.py \
  .local/public-benchmark-001/summary.json \
  --output .local/public-benchmark-001/context-tokens
```

On Windows use `.venv/Scripts/python.exe` and `target/release/awr.exe`. Use a new
output directory each time. Raw CLI outputs and timing samples stay in that local
directory. Only the reviewed aggregate summary and figures are published.

The source commit in the summary identifies the benchmark inputs and unchanged
runtime source; the subsequent documentation commit adds its results. Runtime IDs
and absolute source locators affect tokenization, so reruns can differ slightly.

## Interpret the comparison

- Full-source reading is a simple baseline, not an optimized search system or a
  named competitor. Savings depend on project size, task links and context budget.
- The headline covers **rendered L1 input**. Feeding the entire CLI JSON response to
  an agent saves less. MCP clients can introduce additional framing or duplication.
- Chat history, system prompts, model output and provider billing are unmeasured.
  There are zero model calls in this benchmark; no answer-quality or business-success
  claim follows from its exact-fact checks.
- Older optional private-workload protocols under `tests/long-ledger`,
  `tests/compact-recovery` and `tests/benchmarks` need their own locally supplied
  source snapshots. They are not prerequisites for reproducing this public result.
