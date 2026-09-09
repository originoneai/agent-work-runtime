#!/usr/bin/env python3
"""Reject internal development material in the Git index; preserve local files."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PRIVATE_DIRS = (
    ".local/", ".venv/", ".awr/", "ledger/", "contracts/", "internal/", "planning/",
    "notes/", "records/", "evidence/", "handoffs/", "docs/design/",
    "docs/decisions/", "docs/acceptance/",
)
PRIVATE_FILES = {
    ".awr/project.toml", "work-ledger.yaml", "LEDGER.md", "GOALS.md", "PLAN.md",
    "KICKOFF.md", "RULES.md", "docs/GOALS.md", "docs/PLAN.md", "docs/RULES.md",
    "docs/KICKOFF.md", "scripts/check_ledger.py",
    "docs/benchmarks/compact-recovery.md", "docs/benchmarks/context.md",
    "docs/benchmarks/large-ledger.md", "docs/benchmarks/performance.md",
}


def is_private(path):
    return path in PRIVATE_FILES or path.startswith(PRIVATE_DIRS)


def main():
    paths = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    bad = sorted(p for p in paths if p and is_private(p))
    if bad:
        print("Internal paths are tracked; remove them from the index, not from disk:")
        print("\n".join(bad[:20]))
        print(f"{len(bad)} forbidden path(s)")
        return 1
    print("Public tree OK: internal paths are absent from the Git index.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
