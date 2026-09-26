# Optional summary-view workflow comparison

45 synthetic runs compare the original primitive workflow, shared preparation and shared preparation with summary views on the same native program bytes and source contracts. This measures presentation savings separately from reducing tool calls. All six behavioral checks passed, including required facts, completion rejection, task identity and single execution.

[Aggregate and program binding](workflow-summary-macos-arm64.json). Runtime source: `2d37ef1c5388156a0b0528228fc73a65cc954545`. Environment: macOS arm64, three repetitions per workflow and strategy.

| Workflow | Calls in each prepared variant | Full prepared text bytes | Summary text bytes | Reduction |
| --- | ---: | ---: | ---: | ---: |
| lightweight | 16 | 58541 | 53964 | 7.82% |
| upgrade | 25 | 90633 | 84500 | 6.77% |
| wait | 22 | 90994 | 82637 | 9.18% |
| dependencies | 31 | 119841 | 110259 | 8.00% |
| unknown_result | 18 | 62479 | 57896 | 7.34% |

Values are medians of complete workflows. Summary views reduce returned text by 6.8–9.2% compared with shared preparation using full responses. They do not further reduce calls. Text bytes are cumulative UTF-8 tool text plus CLI stdout, not model tokens or per-call averages.

The aggregate also reports disjoint cost segments. The lightweight summary fixture uses 16 tool calls total: one for project initialization, four independent guard/verification calls, and eleven normal-workflow calls. Connection setup/catalog traffic remains separately recorded, and all-inclusive totals are retained. This fixture still constructs a fresh project and connection; it does not establish the cost of an already-connected production agent.

Reproduce with the existing [workflow runner](../../../tests/benchmarks/workflow/run.py), adding `--include-summary` to the command in [the original benchmark](workflow.md). Existing default strategies and historical results remain unchanged. Raw logs stay in a new ignored output directory. No real-model token, billing or enterprise-client acceptance claim is made.
