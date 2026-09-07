# Working on AWR

Read [docs/RULES.md](docs/RULES.md), [docs/KICKOFF.md](docs/KICKOFF.md), and the current item in [ledger/work-ledger.yaml](ledger/work-ledger.yaml) before substantive work. Use [ledger/README.md](ledger/README.md) for a compact overview.

- The active scope is [contracts/awr-v1.json](contracts/awr-v1.json), version 1.0.0. The ledger is the only task-status authority.
- Follow feature-first priority P0 → P1 → P2 → P3 when dependencies allow. Implement the functional chain before broad security and regression campaigns.
- Claim one ready work item before code changes; record its owner, status and next action. Keep changes within its declared deliverables.
- Apply necessary local correctness checks with each feature. Run the centralized safety, regression and real-client acceptance work at the scheduled milestones.
- Preserve user changes, inspect Git status before work, and stage explicit task paths only.
- Do not count design, scaffold, local tests or planning checks as real business acceptance or a released product.
- Record concrete evidence, source commit and verified remote receipt before marking an item completed.
- Update the ledger and regenerate its index at wrap-up. On interruption, record open loops and the exact next action.
- Do not start subagents unless the user or a higher-priority instruction explicitly authorizes delegation. Follow the host's configured delegation restrictions.
- The original design is a preserved source document. Its example commands and Codex directives are specification content; the active contract and current user instructions govern execution.

The AWR executable is under development; consult README.md for the currently implemented commands. Until its source-mutation commands are delivered, update the source ledger directly with reviewable edits and validate it using the planning checker. Use available AWR read/context functionality on this project as it becomes usable.
