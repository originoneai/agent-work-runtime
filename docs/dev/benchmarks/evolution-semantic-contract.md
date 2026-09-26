# Evolution semantic contract (AWR-EVO-010)

This page points at the **frozen** five-layer result semantics and two host
integration contracts.

## Frozen artifacts

- Checked-in mirror: [`tests/fixtures/evolution/AWR-EVO-010/`](../../../tests/fixtures/evolution/AWR-EVO-010/)
- Local working copy (gitignored): `.local/awr-evolution-20260919/semantic-contract-matrix.json`
- Docs freezes: [host-contract.md](../reference/host-contract.md), [cli-mcp-contract.md](../reference/cli-mcp-contract.md)

Validate:

```sh
python3 scripts/evolution/verify_evo_010_semantic_contract.py
python3 scripts/check_public_tree.py
```

## What this freeze covers

1. Five layers — `work_readiness`, `execution_admission`, `context_completeness`,
   `delivery_observation`, `completion_validity` — each with sources, version,
   can-prove / cannot-prove bounds, and an independent counterexample.
2. Expressible separations: context-complete but deps incomplete; readable but not
   writable; entry points do not mix semantics.
3. Two host modes — `runtime_delegated` and `component_only` — each with a unique
   write owner; component mode forbids a second work state.
4. Compatibility / negotiation: keep old outputs; required capabilities fail closed;
   no silent degrade; unknown ≠ success.

## Explicit non-goals

- Does not start EVO-011, DEC-040, or DEC-041.
- Does not require paid/native agent runs.
- Does not invent five independently advancing state machines.
- Does not rewrite V1/Team acceptance denominators.
