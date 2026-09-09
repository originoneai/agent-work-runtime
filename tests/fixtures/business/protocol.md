# Real-client fixture protocol

These versioned synthetic scenarios are test inputs, not execution records.
Prepare an isolated copy for each run and keep every receipt under `.local/`.
Use an actual supported client and natural business language. Evaluator IDs,
expected answers, state-transition commands and review rubrics must stay out of
client prompts. Check every submitted input with `check_client_input.py`.

Each scenario requires initial work, real tool or role execution, new artifacts,
two substantive user followups, a reviewer distinct from the producers, final
delivery and linked receipts. The exact required evidence and insufficient
substitutes are defined in `gate-contract.json`. Preserve failures, source changes
and revisions. Never reuse a previous run's result or count a prepared fixture,
API probe, self-review or skipped gate as actual business completion.

Run the verifier to check definitions and local intake. For a real-client run,
keep prompts, later inputs and evaluator material outside the client project
until their round. Bind final receipts to reviewed source and artifact hashes.
Public aggregate reports must omit private transcripts and project materials.
