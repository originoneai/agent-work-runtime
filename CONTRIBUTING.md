# Contributing

Build with the Rust toolchain pinned in `rust-toolchain.toml`:

```sh
cargo build --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo fmt --all --check
python3 scripts/check_public_tree.py
```

Use Python 3.11+ and `pip install -r requirements-dev.txt` for Python fixture checks.
The concentrated runner also requires `rtk` and is `python3 tests/regression/verify.py --output .local/regression-001`.
Use a new output directory for each run. Test specifications under `tests/` describe
checks; they do not claim that any release or real-client acceptance has passed.

Send a focused PR describing the behavior change and relevant verification.
Use synthetic inputs for reproductions. Preserve original source files unless a
source mutation was explicitly requested, and cover stale revisions and interrupted
writes when changing persistence behavior.

Internal development plans, work ledgers, review notes, transcripts and raw run
records stay local in `.local/` or the ignored root directories. Public test data,
user documentation and reproducible aggregate results are welcome. The public-tree
check prevents known internal paths from entering the Git index, including files
added with `git add -f`; review still needs to catch private material under new names.

Use [GitHub issues](https://github.com/originoneai/agent-work-runtime/issues) for
bugs and proposals. Never include credentials or private project sources. By
contributing, you agree to license your contributions under Apache-2.0.
