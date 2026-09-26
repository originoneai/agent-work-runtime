# Claude Code named adapter (WS-024)

> **Layer: named controlled adapter (not auto-startable).** Use the
> [L0 session workflow](session-workflow.md) to bind and report. Capability
> details: [named agent host](named-agent-host.md).

## What AWR declares

| Capability | Supported |
| --- | --- |
| start | no → human continuation |
| status_read | yes |
| stop_confirmation | no → human continuation |
| reconnect_resume | yes |
| result_forensics | yes |

## Operator path

1. Start Claude Code yourself (AWR will not launch it).
2. Bind with `--client generic --external-session claude:<native-id>`.
3. Report phases with L0 `ExternalExecutionReport` when you need AWR observations.
4. On retry, reconnect the same execution identity before starting anything new.

Adapter id: `claude_code`.
