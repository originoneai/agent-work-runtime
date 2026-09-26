# Assessment counterexample corpus and compare budgets (DEC-013)

This page freezes the **pre-registered** offline evaluation inputs for
model-free engineering assessment. It reuses the EVO-001 pre-registration
style (freeze corpus identity, expect/forbid, metrics, and budgets **before**
the first candidate run) but **counts separately** from EVO. It does **not**
require EVO paid/dual-host experiments first, and it does **not** build a
second evaluation platform.

Machine companions:

- Corpus: [`tests/fixtures/assessment/counterexamples/`](../../../tests/fixtures/assessment/counterexamples/)
- Compare harness: [`tests/benchmarks/assessment/`](../../../tests/benchmarks/assessment/)
- Reused contracts: `tests/fixtures/assessment/{contracts,signals,envelope}/`

## 1. Acceptance mapping

| # | Acceptance (Chinese) | Where frozen |
| --- | --- | --- |
| 1 | 冻结覆盖矩阵、fixture 身份、预期/禁用行为及比较脚本；首次候选运行前固定性能预算和统计口径。 | `counterexamples/coverage-matrix.json`, per-fixture `expect`/`forbidden`, `benchmarks/assessment/{contract,budgets,compare}.py` |
| 2 | 确定性结构正确性、运行开销、建议有效性分开；模型成功率、节约金额和语义理解能力未测不推导。 | contract `metric_families` + `unmeasured_not_derived`; compare refuses merged scores / derived fields |
| 3 | 涵盖错误重试、撤权、错任务、证据撤回、缺字段、投递未知和时间预算；关键硬约束失败不以平均数抵消。 | critical families on C02/C03/C04/C08/C12/C14/C15; compare rejects `claimed_pass_via_average` |

## 2. Corpus (C01–C20 + boundary positives)

Twenty synthetic counterexample classes from ACCEPTANCE.md, plus five positive
boundary fixtures (P01–P05). Each fixture declares:

- stable `case` id and `slug` (fixture identity)
- `expect` and `forbidden` behaviors
- optional `hard_constraint` + `critical_family` (zero tolerance)

Critical families (violations must not be offset by averages):

- `error_retry` (C03)
- `revoke` (C08)
- `wrong_task` (C02)
- `evidence_withdrawal` (C14)
- `missing_fields` (C04)
- `delivery_unknown` (C15)
- `time_budget` (C12)

Level is **L1 synthetic**. This corpus does not substitute L4 real-business
acceptance.

## 3. Baseline vs candidate

| Arm | Setting | Notes |
| --- | --- | --- |
| Baseline | same source SHA + fixture + policy + as_of; `assessment_explain=off` | Existing tip behavior without new explain |
| Candidate | same identity; `assessment_explain=on`; **read-only** | No model/network; no claim/completion side effects |

Recorded cost dimensions (separate): `collect`, `judge`, `output`.

## 4. Metric families (kept separate)

1. **structural_correctness** — deterministic fixture expect/forbid / hard gates
2. **runtime_overhead** — collect/judge/output latency & bytes vs frozen budgets
3. **advisory_effectiveness** — advisory code/reason match on fixtures only

**Not derived** from unmeasured data:

- model success rate
- dollar savings
- semantic understanding

## 5. Statistics 口径 (frozen before first candidate)

- Percentile: nearest-rank
- p95: sort ascending; index = ceil(0.95×N)−1
- Require **N ≥ 30** before labeling p95 (single samples must not be called p95)
- Timeouts, failures, and overruns stay in the denominator
- Cold and warm reported separately; warmups do not count as samples

## 6. Performance budgets

`tests/benchmarks/assessment/budgets.json` freezes **slots and rules** before the
first candidate run. Absolute millisecond ceilings are bound from a measured
baseline (`status`: `frozen_slots_pending_baseline_binding` →
`frozen_with_baseline`). This page does **not** invent fixed millisecond gains.

Hard rules:

- candidate tool-call delta default max = 0 vs baseline
- output return-bytes absolute ceiling = 262144 until revised with evidence
- hard-constraint failures fail the compare even if soft averages look good
- `compare.py` does not label a pair passed while budget status is still
  `frozen_slots_pending_baseline_binding`
- a missing cost field stays missing; it is not treated as zero and cannot
  look like a millisecond improvement

## 7. How to verify (offline)

```sh
python3 tests/fixtures/assessment/counterexamples/verify.py
python3 tests/benchmarks/assessment/verify.py
python3 -m unittest tests.benchmarks.assessment.test_compare
python3 tests/fixtures/assessment/envelope/verify.py
python3 tests/fixtures/assessment/signals/verify.py
python3 tests/fixtures/assessment/contracts/verify.py
```

Compare two receipts:

```sh
python3 tests/benchmarks/assessment/compare.py \
  --baseline path/to/baseline_receipt.json \
  --candidate path/to/candidate_receipt.json
```

## 8. Boundaries

- Assessment remains read-only; no model or network path
- Missing fields stay missing (never zero-filled); unknown ≠ false
- Reuses DEC-010/011/012 AssessmentEnvelope + FactSnapshot; no second eval stack
- EVO-000 / paid dual-host work is **not** started by this card

## 9. DEC-022 shadow compare / kill-switch (developer)

Offline shadow compare reuses this harness's same-input identity rules and adds
rule-version-attributed diffs plus full retained failure samples. Advice
`disabled` is the kill-switch (prior advice behavior restored; hard protections
remain). See [`docs/reference/assessment.md`](../reference/assessment.md) §17 and
`awr assessment compare` / `awr assessment advice-mode`.

## 10. DEC-060 first-batch independent acceptance gate

This section is the **first-batch delivery note** for the model-free explanation
closed loop. It independently re-checks DEC-010 / 011 / 012 / 013 / 020 / 021 /
022 acceptance evidence and counts **eight** first-batch items separately
(those seven cards plus this gate). It does **not** rebuild a second assessment
stack.

Machine companions:

- Evidence pack: [`tests/fixtures/assessment/mvp-acceptance/`](../../../tests/fixtures/assessment/mvp-acceptance/)
  (checked-in equivalent of planned `.local/awr-decision-20260920/mvp-acceptance/`)
- Gate harness: [`tests/benchmarks/assessment/prove_acceptance_dec060.py`](../../../tests/benchmarks/assessment/prove_acceptance_dec060.py)
- Re-run helper: [`tests/benchmarks/assessment/run_offline_gate.py`](../../../tests/benchmarks/assessment/run_offline_gate.py)

### Acceptance mapping

| # | Acceptance (Chinese) | Where proven |
| --- | --- | --- |
| 1 | 010/011/012/013/020/021/022 的验收证据与本门均逐项核对，首批8项独立计数。 | `mvp-acceptance/{manifest,gate-checklist,prior-acceptance}/`; prove AC1 |
| 2 | 实际断网/无模型配置路径、CLI/MCP 一致性、关键拒绝和禁用回退通过；全部性能原始数据保留。 | `mvp-acceptance/{offline-path,perf-raw,budget-crosscheck}.json`; reuse DEC-021/022 tests; prove AC2 |
| 3 | 测试只证明当前离线解释能力，不代证整个14项、AUTO、Team、原生宿主或已发布版本。 | `mvp-acceptance/non-claims.json` + explicit assertions in prove AC3 |

### Eight first-batch items (independent count)

| # | Work | Role |
| --- | --- | --- |
| 1 | AWR-DEC-010 | Reuse mapping + AssessmentEnvelope contract |
| 2 | AWR-DEC-011 | Bounded FactSnapshot + source-quality labels |
| 3 | AWR-DEC-012 | Typed envelope unknown/conflict semantics |
| 4 | AWR-DEC-013 | Counterexample corpus + compare budgets |
| 5 | AWR-DEC-020 | Explanation chain over existing judgments |
| 6 | AWR-DEC-021 | Optional CLI/MCP explain without breaking legacy |
| 7 | AWR-DEC-022 | Offline replay, shadow compare, advice kill-switch |
| 8 | AWR-DEC-060 | Independent acceptance gate (this note) |

None of these substitutes for the full 14-item suite, AUTO, Team, native host,
or a released version.

### Budgets and ROI

Pre-registered `tests/benchmarks/assessment/budgets.json` remains
`frozen_slots_pending_baseline_binding` at this gate. DEC-060 **retains** raw
performance samples under `mvp-acceptance/perf-raw/` and **refuses** to invent
millisecond gains, dollar savings, or later-version candidacy until baseline
slots are bound with measured evidence.

### Follow-on enhancements and scheduling conditions

See `mvp-acceptance/follow-ons.json`. Condensed:

1. Bind baseline budgets before any later-version overhead pass/fail.
2. DEC-061 covers the full 14-item applicable layers — not claimed here.
3. Context packing / retention explain (DEC-030+) after this gate.
4. AUTO routing requires separate authorization.
5. DEC-040 / DEC-041 / EVO-000 are **not** started by this card.

### How to re-verify (offline)

```sh
python3 tests/benchmarks/assessment/prove_acceptance_dec060.py
python3 tests/benchmarks/assessment/run_offline_gate.py
# optional fuller cargo walk:
python3 tests/benchmarks/assessment/run_offline_gate.py --full
```

### Boundaries (unchanged)

- Assessment read-only; no model or network path
- Missing fields stay missing (never zero-filled); unknown ≠ false
- Reuses DEC-010..022 evidence, fixtures, offline replay, CLI/MCP explain, kill-switch
- Tests prove **current offline explain** only (explicit non-claims in pack)
