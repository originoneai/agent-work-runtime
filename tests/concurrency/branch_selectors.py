#!/usr/bin/env python3
"""Exercise explicit event branch selectors through real CLI and MCP stdio."""
import argparse
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "crates/awr-mcp/tests"))
from cli_parity import Client, MANIFEST, WORK


class BranchSelectors(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="awr-branch-selector-")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        (self.root / "work.yaml").write_text(WORK)
        (self.root / "rules.md").write_text("# Source authority {severity=hard scope=project value=*}\n\nPreserve acceptance.\n")
        (self.root / "goal.md").write_text("# Deliver the reviewed report\n")
        (self.root / "project.toml").write_text(MANIFEST)
        self.ok("init", "--manifest", "project.toml", "--accept")
        self.sessions = {"main": self.session("main-reviewer")}
        for name in ["review-a", "review-b"]:
            self.ok("branch", "create", name, "--actor", "reviewer", "--reason", "Explore the report independently", "--expected-revision", str(self.revision()))
            self.ok("branch", "switch", name, "--actor", "reviewer", "--reason", "Continue the selected report review", "--expected-revision", str(self.revision()))
            self.sessions[name] = self.session(name)
        self.ok("branch", "switch", "main", "--actor", "reviewer", "--reason", "Return to the main report", "--expected-revision", str(self.revision()))
        self.client = Client(MCP, self.root)
        self.addCleanup(self.client.close)

    def cli(self, *args):
        return subprocess.run([str(AWR), "--project", str(self.root), "--json", *args],
                              capture_output=True, text=True, timeout=30)

    def ok(self, *args):
        result = self.cli(*args)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def revision(self):
        return self.ok("branch", "list")["project_revision"]

    def session(self, agent):
        return self.ok("session", "start", "--work", "W", "--agent", agent, "--provider", "fixture",
                       "--model", "no-model-call", "--expected-revision", str(self.revision()))["session"]

    def snapshot(self):
        with sqlite3.connect(f"file:{self.root / '.awr/state.db'}?mode=ro", uri=True) as db:
            tables = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
            rows = {name: sorted(repr(row) for row in db.execute('SELECT * FROM "' + name.replace('"', '""') + '"')) for name in tables}
        return rows, {name: (self.root / name).read_bytes() for name in ["work.yaml", "rules.md", "goal.md", ".awr/project.toml"]}

    def append(self, transport, session, branch, error=False):
        before = self.snapshot()
        arguments = {"expected_revision": self.revision(), "session": session,
                     "event_type": "report.observed", "summary": "Reviewed the report assumptions"}
        if branch is not None:
            arguments["branch"] = branch
        if transport == "cli":
            command = ["event", "append", "--session", session, "--type", arguments["event_type"],
                       "--summary", arguments["summary"], "--expected-revision", str(arguments["expected_revision"])]
            if branch is not None:
                command += ["--branch", branch]
            result = self.cli(*command)
            self.assertEqual(result.returncode, 1 if error else 0, result.stdout + result.stderr)
            value = json.loads(result.stderr if error else result.stdout)
        else:
            response = self.client.rpc("tools/call", {"name": "awr_event_append", "arguments": arguments})["result"]
            self.assertEqual(bool(response.get("isError")), error, response)
            value = response["structuredContent"]
            self.assertEqual(json.loads(response["content"][0]["text"]), value)
        if error:
            self.assertEqual(value["code"], "InvalidInput")
            self.assertEqual(self.snapshot(), before)
        return value

    def test_cli_explicit_main_cannot_be_overridden_by_a_named_session(self):
        self.append("cli", self.sessions["review-a"]["id"], "main", error=True)

    def test_mcp_explicit_main_cannot_be_overridden_by_a_named_session(self):
        self.append("mcp", self.sessions["review-a"]["id"], "main", error=True)

    def test_matching_selectors_and_omitted_hints_keep_expected_provenance(self):
        for transport in ["cli", "mcp"]:
            for branch, owner in self.sessions.items():
                result = self.append(transport, owner["id"], branch)
                self.assertEqual(result["event"]["branch_id"], owner["branch_id"])
                self.assertEqual(result["event"]["session_id"], owner["id"])
            self.append(transport, self.sessions["review-a"]["id"], "review-b", error=True)
            self.append(transport, self.sessions["main"]["id"], "review-a", error=True)
            result = self.append(transport, self.sessions["review-a"]["id"], None)
            self.assertEqual(result["event"]["branch_id"], self.sessions["review-a"]["branch_id"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--awr", type=Path, required=True)
    parser.add_argument("--mcp", type=Path, required=True)
    options, rest = parser.parse_known_args()
    AWR, MCP = options.awr.resolve(), options.mcp.resolve()
    unittest.main(argv=[__file__] + rest)
