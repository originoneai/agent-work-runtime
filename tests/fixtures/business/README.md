# Independent business fixtures

This pack contains eight independent synthetic business projects defined by
`scenarios.json`. It contains no actual client execution records.

Each scenario has a distinct namespace, source project, work graph, participant
slots, three canonical business artifact requirements, independent review and
delivery requirements, two natural followups, and separately held author changes.
The client project contains initial business material only. Prompts, later changes
and review criteria stay outside it until the corresponding business round.
No output, session, claim, checkpoint, completion evidence or fake review is seeded.

Use Python 3.11 or newer with PyYAML, Git, `rtk`, and the built AWR executable:

```sh
rtk proxy .venv/bin/python tests/fixtures/business/verify.py --render
rtk proxy .venv/bin/python tests/fixtures/business/verify.py --output .local/business-intake-001
```

The verifier rejects missing business gates, missing followups/artifacts,
non-independent reviewer requirements, reused namespace definitions, leaked
evaluator input and cyclic work. It prepares all eight projects, invokes actual
AWR intake/context/Doctor commands, publishes both sets of source-author inputs,
checks changed hard rules, refuses reused runs and out-of-order publication, and
demonstrates that an unmerged author edit cannot be overwritten. These are local
fixture checks; no real user followup or independent business review is claimed.

To prepare **a new real-client run**, use a different directory and run ID:

```sh
rtk proxy .venv/bin/python tests/fixtures/business/prepare.py create \
  --scenario AWR-SC-001 --output .local/business-live-001 --run-id business-live-001
```

The scenario ID is for the operator. Submit only the natural prompt from that
scenario's `prompts/` directory to the actual client, with the copied `project/`
as its workspace. Do not paste evaluator JSON, expected criteria, internal IDs or
the verification runner's commands into the client prompt.

Before submitting **every** actual client input, validate a UTF-8 file containing
exactly the text that will be sent. Canonical prompts, preludes, technical
supplements, rework requests and final-delivery requests all use the same policy:

```sh
rtk proxy .venv/bin/python tests/fixtures/business/check_client_input.py \
  --input .local/exact-client-input.md \
  --kind technical-supplement \
  --receipt .local/exact-client-input-preflight.json
```

The preflight derives current scenario and work identifiers from the fixture
contract and its work graphs. It also rejects native object IDs, AWR/CLI session
and work-state commands or parameters, expected-answer wording, forced OK-only
responses, and test markers. A natural request to use the AWR MCP service is
allowed: `MCP` is a client-visible service choice, not by itself a test command.
A rejected input exits nonzero. The JSON receipt binds the input hash to the
contract, graphs and policy rules; it does not invoke a model, submit the input,
modify business state, complete a business turn or grant E4 credit.

This deterministic preflight is necessary but cannot prove that prose is natural
or find every form of answer pollution. An independent reviewer must still review
the exact text. An extra or technical input remains supplemental evidence and
must not be counted as a canonical business round merely because it passes.

When the first result is retained and the client is paused, the fixture author can
publish the next input, then submit the corresponding natural user followup:

```sh
rtk proxy .venv/bin/python tests/fixtures/business/prepare.py publish-round \
  --output .local/business-live-001 --round 1
```

Publication checks the unchanged fixture definition, target fingerprints, round
order and canonical root. It does not invoke AWR, apply runtime transitions,
submit a user message or produce a business result. If interrupted during a
publication, retain the `publishing` marker and inspect each receipt and file;
there is no blind retry mode. Do not publish while another client is editing the
same inputs. Unexpected user edits require review before a new author change.

The restricted-material scenario creates a public log larger than 3 MiB outside
the client project and publishes it in the first followup. Its separate synthetic
restricted note never enters that project. Log rows represent events, with many
events per batch; a report must not confuse line counts and processed batches.

Read [the actual acceptance procedure](protocol.md),
[coverage](coverage.md) and `gate-contract.json` before
attempting E4. Performance measurements are separate from business acceptance.
