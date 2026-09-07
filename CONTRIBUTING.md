# Contributing

AWR is at the planning baseline. Begin with the [V1 contract](contracts/awr-v1.json), [work ledger](ledger/work-ledger.yaml), [rules](docs/RULES.md), and [kickoff](docs/KICKOFF.md).

Choose a dependency-ready item, record ownership, and keep changes scoped to its acceptance criteria. Include the concrete behavior, applicable checks and remaining limitations in the change description. Broad security and regression work is scheduled after the feature and integration milestones.

Update the authoritative YAML ledger and regenerate its readable index:

```sh
.venv/bin/python scripts/check_ledger.py --render
.venv/bin/python scripts/check_ledger.py
```

Do not mark runtime functionality complete based on planning validation. Completion evidence records acceptance outcomes, artifacts, the source commit and a verified remote receipt. Full business scenarios additionally require a distinguishable independent reviewer and their own delivery commit.

Keep secrets, local runtime databases, private source material and large logs out of commits. Use synthetic or authorized, anonymized fixtures.

Contributions are licensed under [Apache License 2.0](LICENSE).
