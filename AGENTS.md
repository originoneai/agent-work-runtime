# Working on AWR

Read README.md and CONTRIBUTING.md before changing the project.

- Preserve unrelated edits and keep each change focused. Stage explicit paths.
- Authoritative user sources remain authoritative; SQLite projections must not silently replace them.
- Run correctness checks relevant to the change. Distinguish unit tests, fixture checks, native client verification and business acceptance.
- Keep personal plans, development ledgers, handoffs, raw execution records and local AWR state in ignored local paths. Do not add them to Git, PR descriptions or release assets.
- Public examples, synthetic test fixtures and reproducible aggregate benchmark results belong in the repository.
- Run `python3 scripts/check_public_tree.py` before submitting a PR. It checks the Git index, so local-only material can stay on disk.
- Follow the user's and host's delegation rules. Do not start other agents without authorization.
