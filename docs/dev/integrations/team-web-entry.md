# Team Web entry (WS-044)

AWR Team's operator HTTP/MCP service rejects browser `Origin` on the classic
`/v1/projects/*` and MCP surfaces. That remains true. WS-044 adds a **designed
Web entry** under `/v1/web/*` so a self-hosted browser UI can collaborate without
embedding an admin bearer in page JavaScript or "just opening CORS".

## How it differs from MCP bearer auth

| | MCP / classic HTTP | Web entry (`/v1/web/*`) |
|---|---|---|
| Credential in browser | No (native clients) | No (HttpOnly cookie only) |
| Transport proof | `Authorization: Bearer` | Session cookie after one-time exchange |
| `Origin` | Always rejected | Allowlisted via `allowed_web_origins` |
| CSRF guard | N/A (no browser) | Required `X-AWR-Web: 1` |
| Logout / revoke | Credential revoke (access.*) | `/v1/web/logout`, `/v1/web/session/revoke` |
| Writes | Shared command store + idempotent receipts | **Same** store, authz, receipts |

The login body may carry a bearer **once** to the server. The response sets
`awr_web_session` (`HttpOnly; SameSite=Strict; Path=/v1/web`). Subsequent
query/command/access calls use the cookie; the page must clear any bearer field
from the DOM after submit.

## Enable the entry

```toml
version = 1
listen = "127.0.0.1:9908"
allowed_hosts = []
allowed_web_origins = ["http://127.0.0.1:7381"]

[[projects]]
key = "example"
tenant_id = "tenant-a"
project_id = "project-a"
```

Without `allowed_web_origins`, `/v1/web/*` returns `WebEntryDisabled`. Classic
routes still deny any `Origin`.

## Inspector

`tools/inspector` exposes the Team Web loop UI (`#team`): My Projects → parallel
overview with card/list toggle → detail (owner, agent, outcome, blocker, next
step) → collaboration actions (accept responsibility, select authorized agent,
run/pause visibility, respond to blocker, receive handoff, submit review, rework,
accept).

- Demo/tests: `--demo` serves fixtures from
  `tests/fixtures/workstreams/team-web-loop/`.
- Live Team proxy: `--team-url http://127.0.0.1:9908` forwards to the server Web
  entry (cookie session). The proxy rewrites upstream `Path=/v1/web` cookies to
  `Path=/api/team` so the browser sends them to Inspector team routes. Live
  overview/action never serve fixtures; writes go through `/v1/web/projects/{key}/command`.

Member/role/grant changes reuse the existing `access.inspect|preview|apply|outcome`
server path behind the Web entry — no second admin plane and no enterprise org /
cost-center wizards.

## Blocker detail

Visible dependencies show prerequisite outcome, release condition, and check
basis, and drill to accepted upstream. Hidden dependencies only show
non-leaking handling hints. Backend codes and raw receipt references stay in the
detail layer (`<details>`), not the card summary.
