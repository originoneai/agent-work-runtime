# Working on this business project

This directory is an independent project with its own Git repository, source manifest, and work ledger. Use this project's `project.toml`, `GOALS.md`, `PLAN.md`, `RULES.md`, and `work-ledger.yaml` as the business sources. Parent repository ledgers belong to a different project. Higher-priority host instructions still apply.

Use AWR to manage the work and preserve recoverable progress. Its executable is `/Users/mac/Documents/originone/agent-work-running/target/release/awr`. Every shell command starts with `rtk`; use `rtk proxy` when complete command output is needed. Pass this project's absolute path with `--project` and request JSON receipts where supported. Inspect command help when needed. If `.awr/` is absent, preview the existing manifest with `init --manifest project.toml`, inspect the result, then initialize with `--accept`; this source mapping is authorized for this project.

Read current AWR status and selectable work. Start or resume the actual work session and claim the selected item before changing business state. Use bootstrap for orientation and complete execution context for business work. Base decisions on the current source revision, preserve provenance and missing facts, and refresh AWR source/context state when project sources change. Do not substitute cached context for newer source files.

Progress work through AWR and save real CLI receipts in `.work-receipts/`. Create the actual business files listed in `DELIVERABLES.md`, attach matching artifact or evidence references where the CLI requires them, and checkpoint meaningful progress and unresolved work at each handoff. Inspect existing receipts before retrying an interrupted mutation. Do not directly write the AWR database or fabricate sessions, tool calls, evidence, review, or completion.

Independent review belongs to a different participant. The executor may prepare work for review but must not write the independent review record or claim that review passed. Final delivery happens only after the review is received and addressed.

Stay inside this business project. The sibling `control/` directory, parent scenario specifications, later prompts, unpublished inputs, and other projects are operator material outside your scope. Normal CLI help and `/Users/mac/Documents/originone/agent-work-running/docs/integrations/codex.md` are available for operating AWR. Do not inspect credential files or change global client settings, rules, or hooks. Do not launch subagents or other model clients; the operator coordinates participants.
