# Operations audit (AWR-TMCP-040)

## Purpose

Trace permission, planning, and delivery changes with durable ops-audit records
bound to the same PostgreSQL transaction as the business write, event, and
receipt. Support scoped history locate/export for members and project admins.

## What is recorded (success path)

Each committed ops-audit row includes:

| Field | Meaning |
| --- | --- |
| person / actor / client | Responsible person when known, plus Agent/client identity |
| action | TMCP business action (e.g. `access.manage_project`, `planning.publish`, `delivery.finalize`) |
| target | Kind + id (`member`, `candidate`, `delivery`, `work`, `request`, …) |
| request_id | Stable idempotency / locate key when the entrypoint supplies one |
| auth / source versions | Membership version, permission policy version, source/contract digest |
| digest + summary | Redacted diff/summary hash — **no** raw secrets, chat text, or tool I/O |
| result + time | `committed` / `succeeded` and server timestamp |

Categories in v1: `access`, `planning`, `delivery`.

## Atomicity

New business writes for these categories insert the ops-audit row **inside the
same transaction** as domain mutations and related events/receipts. The service
must not commit business state and then omit audit.

Planning MCP entrypoints that pass a `PlanningCommandBind` also persist the
`planning_command_receipts` row in that same transaction.

## Deny channel

Permission failures are recorded on `awr_team.ops_audit_denies`:

- Separate transaction — **never** mutates business tables
- Reason codes are redacted (no bearer/secret material)
- Soft capacity: retain at most `DENY_CAPACITY_PER_PROJECT` (1000) rows per
  project; oldest rows are pruned on insert

A deny record does not change authorization outcome.

## Read / count / export authorization

Query ops (also available through `awr_team_query`):

- `audit.history` — locate records (filters: work / change / member / request / category)
- `audit.count` — authorized counts only (no out-of-scope totals)
- `audit.export` — same scope as history, marked as export

| Caller | Visible rows |
| --- | --- |
| `audit.read_project` (project_admin template) | Project-wide (optional member filter) |
| Other authenticated members | Own `actor_id` records only; cannot count/export another member’s history |

## Explicit non-claims

- **Not** full chat text, arbitrary tool input/output, or token billing
  collection. Those remain out of scope here; WS-041 covers usage metering.
- PostgreSQL ops audit does **not** claim protection against database-owner
  tampering, WORM / enterprise non-repudiation, or external evidence custody.

## Schema

Migration `20260923000031_ops_audit.sql` (schema version **31**):

- `awr_team.ops_audit_records`
- `awr_team.ops_audit_denies`

Both tables use tenant/project RLS consistent with other Team tables.
