# Evolution plan-validator compatibility (AWR-EVO-002)

This page points at the **checked-in** EVO-002 Team-compatible local planning
validator fixtures and verifier. It does not rewrite V1/Team denominators or
claim Mac full-book PASS when the authoritative ledger is absent.

## Artifacts

- Checked-in mirror: [`tests/fixtures/evolution/AWR-EVO-002/`](../../../tests/fixtures/evolution/AWR-EVO-002/)
- Validator: [`scripts/evolution/plan_validator_compat.py`](../../../scripts/evolution/plan_validator_compat.py)
- Verifier: [`scripts/evolution/verify_evo_002_plan_validator.py`](../../../scripts/evolution/verify_evo_002_plan_validator.py)
- Local working copies (gitignored): `.local/awr-evolution-20260919/plan-validator-compat/`,
  `scripts/check_ledger.py`, `ledger/README.md`

## What this gate covers

1. Fixed reproduction of the prep-period `FAIL: 'work_items'` defect when Team
   work-scope / acceptance-scenario extensions are naively read as `awr-v1`
   `work_items`, plus clear attribution for misplaced `AWR-G-TEAM` without
   auto-migration.
2. Contract type+version discrimination; wrong binding, unknown type/version,
   duplicate scope, and forged completion stay rejected.
3. Old tasks / acceptance records / V1/Team denominators are not rewritten;
   EVO-002 stats are independent.
4. Whole-book local planning validation is actually executed; raw FAIL is
   reported when the Mac-authoritative ledger is not mounted — never claimed as PASS.

## Explicit non-goals

- Does not start EVO-010, DEC-040, or DEC-041.
- Does not open a parallel Team active-work ownership fix (WS-014
  `ece9d3a` ownership slice is reused/verified as already delivered).
- Does not call paid models or start native agents.
- Does not run Mac `awr work complete`.

Validate:

```sh
python3 scripts/evolution/verify_evo_002_plan_validator.py
python3 scripts/check_public_tree.py
```
