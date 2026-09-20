#!/usr/bin/env python3
"""Guard the isolated 0.5.0 source and its explicit manual publication identity."""
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
MEMBERS = {f"awr-{name}" for name in ("core", "workspace", "store", "source", "context", "runtime", "cli", "mcp")}


def validate_scope(manifest, lock, tracked):
    members = manifest["workspace"]["members"]
    if set(members) != {f"crates/{name}" for name in MEMBERS} or len(members) != len(MEMBERS):
        raise ValueError("0.5.0 requires exactly the eight reviewed workspace crates")
    packages = lock["package"]
    local = {p["name"] for p in packages if not p.get("source")}
    if local != MEMBERS:
        raise ValueError("unexpected local Cargo dependency")
    if any(p["name"].startswith("awr-team") or p["name"].startswith("sqlx") or p["name"] in
           {"tokio-postgres", "postgres", "deadpool-postgres"} for p in packages):
        raise ValueError("Team dependency is outside the 0.5.0 release")
    if any(path.startswith(("crates/awr-team", "migrations/team/")) for path in tracked):
        raise ValueError("Team source is outside the 0.5.0 release")
    if manifest["workspace"]["package"]["version"] != "0.5.0":
        raise ValueError("this release guard is specific to 0.5.0")


def validate_identity(env, sha, version):
    expected = env.get("EXPECTED_SOURCE_SHA", "")
    if expected and (not re.fullmatch(r"[0-9a-f]{40}", expected) or expected != sha):
        raise ValueError("reviewed source SHA does not match checkout")
    if env.get("GITHUB_SHA") and env["GITHUB_SHA"] != sha:
        raise ValueError("workflow source SHA does not match checkout")
    if env.get("RELEASE_PUBLISH", "").lower() == "true":
        if (env.get("GITHUB_EVENT_NAME") != "workflow_dispatch" or
                env.get("GITHUB_REF") != "refs/heads/release/0.5.0" or
                env.get("EXPECTED_RELEASE_VERSION") != version or version != "0.5.0" or not expected):
            raise ValueError("publication requires manual release/0.5.0, version 0.5.0 and the full reviewed SHA")


def main():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT, text=True).split("\0")
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    validate_scope(manifest, lock, tracked)
    validate_identity(os.environ, sha, manifest["workspace"]["package"]["version"])
    print(json.dumps({"version": "0.5.0", "source_sha": sha, "scope_checked": True,
                      "workspace_members": sorted(MEMBERS), "publication_requested": os.environ.get("RELEASE_PUBLISH") == "true"}))


if __name__ == "__main__":
    main()
