# Context token and hard-fact benchmark

`tests/benchmarks/context/contract.json` freezes the P9-004 protocol. The source
ledger remains the status authority. This benchmark uses the same admitted real
historical source snapshot as [P9-001](large-ledger.md), without reducing its
150 work items, accepted rules or source history. Private source files and raw
receipts stay under `.local/`.

The native Rust budget uses `tiktoken-rs 0.12.0` with `o200k_base`. Its legacy
`token_estimate` field is an exact encoding count. A separate Python
`tiktoken 0.14.0` process verifies the rendered bytes with `encode_ordinary`.
The encoding and ordinary-text method are documented in the
[official tiktoken repository](https://github.com/openai/tiktoken); the Python
package is pinned from [PyPI](https://pypi.org/project/tiktoken/0.14.0/).
These are encoding measurements, not model billing or model-memory experiments.

The four metrics apply to `rendered_context`. Complete CLI JSON bytes and tokens
are reported separately: that envelope duplicates text with structured facts,
identities and diagnostics. Applications must pass the rendered context to the
model to use this budget. The benchmark makes no claim that the complete JSON
response, MCP framing or an entire conversation fits the rendered-text budget.

The baseline reads each canonical Markdown/YAML authority file once, sorted by
relative path and joined by one LF. It excludes the manifest, runtime data and
raw originals that would duplicate canonical facts. The numerator and denominator
use the same exact tokenizer. Three fresh independent projects expose variation
from generated IDs; every result counts. A fourth isolated copy changes one
existing completed prerequisite to blocked, with an explicit blocker and next
action. That probe exercises nonempty unresolved dependencies and does not
change the historical project or count as business acceptance.

The oracle is frozen before execution from the canonical work and explicitly
mapped original rule spans. It checks the seven hard-fact categories separately,
including every acceptance entry, complete rule heading/body, and unresolved
prerequisite status, next action and blocker. Native object identity and source
revision/fingerprint bindings are checked as well. Any failed fact, incomplete
context, count mismatch or threshold failure fails the run. One-token requests
must return an explicit `BudgetExceeded` error with no partial success on stdout.

L0 shares source-version headings between rules and preserves exact pointers and
verbatim rule text. Its complete structured envelope still contains rule IDs and
source fingerprints/locators; L1 carries the source inventory in rendered text.
The bootstrap hash format advances to `awr.bootstrap.v2` to bind this rendering.
No rule is summarized or truncated to make the budget pass.

After committing the protocol and source changes, build and run in a new output
directory, substituting the private prepared sample and cache paths:

```sh
cargo build --locked -p awr-cli --bin awr
uv pip install --python .venv/bin/python -r tests/benchmarks/context/requirements.txt
.venv/bin/python -m unittest discover -s tests/benchmarks/context -p 'test_*.py'
.venv/bin/python tests/benchmarks/context/verify.py \
  --awr target/debug/awr --prepared .local/large-ledger/prepared-v6 \
  --output .local/context-benchmark/run-v1 \
  --tokenizer-cache .local/context-benchmark/tiktoken-cache
```

The first dictionary load may download the public tokenizer vocabulary. Private
source text is tokenized locally. `report.json` retains private raw evidence;
`public-summary.json` contains aggregate measurements, exact assertion outcomes
and hash bindings. Reusing an output directory is rejected. A failed run remains
failed; a new source change or protocol correction needs a new commit and run.

Use the maximum L0/L1 counts and minimum compression ratio across all four
cases. Hard-fact recall must be 100% in every category and case. The 50–100x
compression range is a stretch target. These results do not grant any latency,
E4, hook activation, model-client integration or release credit.
