# Evolution acceptance matrix (AWR-EVO-001)

This page points at the **frozen** EVO-001 acceptance matrix, baseline workload,
and cost-authorization rules. It does not record live optimization results.

## Frozen plan

- Checked-in mirror: [`tests/fixtures/evolution/AWR-EVO-001/`](../../../tests/fixtures/evolution/AWR-EVO-001/)
- Local working copies (gitignored): `.local/awr-evolution-20260919/evaluation-plan.json`,
  `fixture-manifest.json`, `authorization-requirements.md`

Validate:

```sh
python3 scripts/evolution/verify_evo_001_plan.py
python3 scripts/check_public_tree.py
```

## What this freeze covers

1. Fixed inputs, assertions, output identities, change axes, and result denominators
   for all ten check groups — sealed before optimization.
2. Pre-registered performance / non-degradation thresholds, sample counts, and
   missing-measurement handling — thresholds must not change after seeing results.
3. Independent native and paid authorization fields; simulated / protocol / native /
   paid evidence layers do not substitute for each other.
4. Separate evaluation axes for implementation correctness, performance gain, and
   full-workflow incremental value.

## Explicit non-goals of EVO-001

- Does not call paid models or start native agents.
- Does not start EVO-002, DEC-040, or DEC-041.
- RET-AMD-001: independent R0/R1/R2 matrix for RET-006 is frozen; the old 36-trial
  proposal/budget does not auto-expand.

Related public context benchmark (orthogonal; not a substitute for full-workflow
value): [README.md](README.md), [workflow.md](workflow.md).
