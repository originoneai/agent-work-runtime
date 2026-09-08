# Independent business fixtures

This pack implements AWR-QA-002: eight independent **definitions and business
input projects** under the existing V1 scenario contract. It is not an E4 run.
The source ledger remains the only scenario-status authority.

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

Read [the actual acceptance procedure](../../../docs/acceptance/README.md),
[coverage](../../../docs/acceptance/coverage.md) and `gate-contract.json` before
attempting E4. Larger real-project and performance acceptance remain separate
ledger work.
