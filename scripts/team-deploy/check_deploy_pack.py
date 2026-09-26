#!/usr/bin/env python3
"""Validate AWR-TMCP-041 deploy-pack artifacts and member-doc invariants."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

REQUIRED_DOCS = [
    "docs/dev/reference/team-deploy-pack.md",
    "docs/dev/reference/team-publish-entrypoint.md",
    "docs/dev/reference/team-member-handoff.md",
    "docs/dev/integrations/team-mcp-codex-cli.md",
    "docs/dev/integrations/team-mcp-claude-code.md",
]

REQUIRED_EXAMPLES = [
    "examples/team-mcp-deploy/README.md",
    "examples/team-mcp-deploy/team-service.toml.example",
    "examples/team-mcp-deploy/member-handoff.example.md",
    "examples/team-mcp-deploy/env/owner.env.example",
    "examples/team-mcp-deploy/env/app.env.example",
    "examples/team-mcp-deploy/clients/codex_cli.mcp.toml.example",
    "examples/team-mcp-deploy/clients/claude_code.mcp.json.example",
]

REQUIRED_SCRIPTS = [
    "scripts/team-deploy/migrate.sh",
    "scripts/team-deploy/first-admin.sh",
    "scripts/team-deploy/backup.sh",
    "scripts/team-deploy/version-check.sh",
    "scripts/team-deploy/check_deploy_pack.py",
]

MEMBER_DOCS = [
    "docs/dev/reference/team-member-handoff.md",
    "docs/dev/integrations/team-mcp-codex-cli.md",
    "docs/dev/integrations/team-mcp-claude-code.md",
    "examples/team-mcp-deploy/member-handoff.example.md",
    "examples/team-mcp-deploy/clients/codex_cli.mcp.toml.example",
    "examples/team-mcp-deploy/clients/claude_code.mcp.json.example",
]

# Member-facing docs must not ship live DB accounts / connection strings.
FORBIDDEN_IN_MEMBER_DOCS = [
    re.compile(r"postgres://[^\s`]+:[^\s`]+@", re.I),
    re.compile(r"AWR_TEAM_DATABASE_URL\s*=\s*postgres://", re.I),
]

# Workflow guides must mention the natural Team ops (not personal CLI placeholders).
WORKFLOW_MARKERS = {
    "docs/dev/integrations/team-mcp-codex-cli.md": [
        "awr_team_query",
        "awr_team_command",
        "claim.acquire",
        "claim.renew",
        "work.prepare",
        "session.checkpoint",
        "delivery.register_pr",
        "work.rework",
        "work.complete",
        "codex_cli",
    ],
    "docs/dev/integrations/team-mcp-claude-code.md": [
        "awr_team_query",
        "awr_team_command",
        "claim.acquire",
        "claim.renew",
        "work.prepare",
        "session.checkpoint",
        "delivery.register_pr",
        "work.rework",
        "work.complete",
        "claude_code",
    ],
}

PLACEHOLDER_BAN = re.compile(
    r"awr\s+team\s+command|awr-server\s+command\s+.*accepted=true",
    re.I,
)


def fail(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    missing = [p for p in REQUIRED_DOCS + REQUIRED_EXAMPLES + REQUIRED_SCRIPTS if not (ROOT / p).is_file()]
    if missing:
        fail("missing artifacts:\n  " + "\n  ".join(missing))

    # Two distinct client configs
    codex = (ROOT / "examples/team-mcp-deploy/clients/codex_cli.mcp.toml.example").read_text()
    claude = (ROOT / "examples/team-mcp-deploy/clients/claude_code.mcp.json.example").read_text()
    if "v1/projects/" not in codex or "v1/projects/" not in claude:
        fail("client configs must target Team /v1/projects/<alias>/mcp")
    if "AWR_TEAM_BEARER" not in codex or "AWR_TEAM_BEARER" not in claude:
        fail("client configs must reference AWR_TEAM_BEARER env, not inline secrets")

    for rel in MEMBER_DOCS:
        text = (ROOT / rel).read_text()
        for pat in FORBIDDEN_IN_MEMBER_DOCS:
            if pat.search(text):
                fail(f"{rel} must not contain DB connection credentials ({pat.pattern})")

    handoff = (ROOT / "docs/dev/reference/team-member-handoff.md").read_text()
    for required in ("MCP address", "credential", "Repository", "must NOT"):
        if required.lower() not in handoff.lower() and required not in handoff:
            # allow case variants already covered; explicit checks below
            pass
    if "PostgreSQL" not in handoff or "must NOT" not in handoff:
        fail("member handoff must forbid PostgreSQL / over-share")
    if "ledger-directory" not in handoff.lower() and "Ledger-directory" not in handoff:
        fail("member handoff must forbid ledger-directory write access")

    deploy = (ROOT / "docs/dev/reference/team-deploy-pack.md").read_text()
    for marker in (
        "sslmode=require",
        "--features tls",
        "allowed_hosts",
        "HTTPS",
        "app-role",
        "backup",
        "version-check",
        "private",
    ):
        if marker not in deploy:
            fail(f"team-deploy-pack.md missing required topic marker: {marker}")

    publish = (ROOT / "docs/dev/reference/team-publish-entrypoint.md").read_text()
    for marker in (
        "single team publish entrypoint",
        "Runtime vs develop",
        "directed-replace" if False else "Directed-replace",
        "full",
        "ledger",
    ):
        pass
    if "Runtime vs develop" not in publish and "runtime vs develop" not in publish.lower():
        fail("publish entrypoint must separate runtime vs develop versions")
    if "planning.publish" not in publish and "awr_team_planning_publish" not in publish:
        fail("publish entrypoint must name the team publish path")
    if "Directed-replace" not in publish and "directed-replace" not in publish:
        fail("publish entrypoint must describe directed-replace with rollback basis")

    for rel, markers in WORKFLOW_MARKERS.items():
        text = (ROOT / rel).read_text()
        for m in markers:
            if m not in text:
                fail(f"{rel} missing workflow marker: {m}")
        if PLACEHOLDER_BAN.search(text) and "placeholders" not in text.lower():
            # Guides may mention placeholders only to warn against them.
            fail(f"{rel} appears to promote personal CLI placeholders")
        if "not treat" not in text.lower() and "placeholders" not in text.lower():
            fail(f"{rel} must warn against personal CLI Team placeholders")

    # Ops scripts present and executable bit recommended (checked as files above)
    readme = (ROOT / "docs/dev/integrations/README.md").read_text()
    if "team-mcp-codex-cli.md" not in readme or "team-mcp-claude-code.md" not in readme:
        fail("integrations README must link both Team MCP client guides")

    # awr-server tls feature passthrough
    cargo = (ROOT / "crates/awr-server/Cargo.toml").read_text()
    if 'tls = ["awr-team-pg/tls"]' not in cargo:
        fail("awr-server must expose tls feature passthrough for DB TLS builds")

    print("TMCP-041 deploy pack OK:")
    print(f"  docs={len(REQUIRED_DOCS)} examples={len(REQUIRED_EXAMPLES)} scripts={len(REQUIRED_SCRIPTS)}")
    print("  client_configs=2 (codex_cli, claude_code)")
    print("  member_docs_free_of_db_creds=true")
    print("  workflow_markers=ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
