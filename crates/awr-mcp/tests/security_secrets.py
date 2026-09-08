#!/usr/bin/env python3
"""Synthetic secret boundaries through the built CLI and real MCP stdio. No model calls."""
import argparse
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest

from cli_parity import Client, MANIFEST, WORK

SENTINEL = "fixture-value-only-never-real"
TOOLS = ["awr_project_status", "awr_work_ready", "awr_work_get", "awr_search",
         "awr_context_compile", "awr_work_transition", "awr_event_append", "awr_evidence_record"]


class SecretTransports(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="awr-secret-transport-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "work.yaml").write_text(WORK)
        (self.root / "rules.md").write_text("# Authority {#authority severity=hard scope=project value=*}\n\nPreserve exact acceptance.\n")
        (self.root / "goal.md").write_text("# Deliver useful analysis\n\nDeliver a reviewed report.\n")
        (self.root / "project.toml").write_text(MANIFEST)
        self.cli_ok("init", "--manifest", "project.toml", "--accept")
        self.cli_ok("status")
        self.client = Client(MCP, self.root)
        self.addCleanup(self.client.close)

    def cli(self, *args, plain=False):
        command = [str(AWR), "--project", str(self.root)] + ([] if plain else ["--json"]) + list(args)
        return subprocess.run(command, cwd=self.root, capture_output=True, text=True, timeout=30)

    def cli_ok(self, *args):
        result = self.cli(*args)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def snapshot(self):
        with sqlite3.connect(self.root / ".awr/state.db") as db:
            tables = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
            return {name: db.execute('SELECT * FROM "' + name.replace('"', '""') + '"').fetchall()
                    for name in tables}

    def revision(self):
        with sqlite3.connect(self.root / ".awr/state.db") as db:
            return db.execute("SELECT project_revision FROM projects").fetchone()[0]

    def no_leak(self, text):
        self.assertNotIn(SENTINEL, text)

    def tool_error(self, name, arguments):
        before = self.snapshot()
        response = self.client.rpc("tools/call", {"name": name, "arguments": arguments})
        self.no_leak(json.dumps(response))
        result = response["result"]
        self.assertTrue(result["isError"])
        structured = result["structuredContent"]
        texts = [json.loads(c["text"]) for c in result["content"] if c["type"] == "text"]
        self.assertEqual(texts, [structured])
        self.assertEqual(self.snapshot(), before)
        self.client.log.flush()
        self.client.log.seek(0)
        self.no_leak(self.client.log.read().decode())
        return structured

    def test_untrusted_fields_and_enum_errors_never_echo_through_cli_or_mcp(self):
        before = self.snapshot()
        evidence = {"external_key": "E", "evidence_type": "report", "level": "implemented",
                    "summary": "Reviewed the report", "locator": "report.txt", "scope": ["W"]}
        for document in [dict(evidence, **{SENTINEL: "ordinary value"}), dict(evidence, level=SENTINEL)]:
            (self.root / "evidence.json").write_text(json.dumps(document))
            for plain in [False, True]:
                result = self.cli("evidence", "add", "--input", "evidence.json", "--expected-revision", str(self.revision()), plain=plain)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.no_leak(result.stderr)
                self.assertEqual(self.snapshot(), before)
        for name in TOOLS:
            result = self.tool_error(name, {SENTINEL: "ordinary value"})
            self.assertEqual(result["code"], "InvalidInput")
        result = self.tool_error("awr_work_transition", {"action": SENTINEL})
        self.assertEqual(result["code"], "InvalidInput")
        print("AWR_PAYLOAD_CASE untrusted_field_diagnostic_no_echo", flush=True)

    def test_cli_and_all_mcp_tools_refuse_recognizable_or_labelled_secrets_without_writes(self):
        for name in TOOLS:
            result = self.tool_error(name, {"pa\u0073sword": SENTINEL})
            self.assertEqual(result["code"], "RuleViolation")
        for field in ["body", "stdout", "stderr", "command"]:
            self.tool_error("awr_event_append", {"expected_revision": self.revision(), "event_type": "work.observed",
                            "summary": "Reviewed results", "payload": {field: "password: " + SENTINEL}})
        before = self.snapshot()
        (self.root / "event.json").write_text(json.dumps({"body": "private_prompt: " + SENTINEL}))
        for plain in [False, True]:
            result = self.cli("event", "append", "--type", "work.observed", "--summary", "Reviewed results",
                              "--payload", "event.json", "--expected-revision", str(self.revision()), plain=plain)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
            self.no_leak(result.stderr)
            self.assertEqual(self.snapshot(), before)
        # Detect before Clap can echo an invalid argument value in its diagnostic.
        result = self.cli("context", "compile", "--budget", "sk-" + "a" * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("sk-", result.stderr)
        self.assertEqual(self.snapshot(), before)

    def test_read_tools_withhold_legacy_required_content_and_safe_data_remains_usable(self):
        response = self.client.rpc("tools/call", {"name": "awr_event_append", "arguments": {
            "expected_revision": self.revision(), "event_type": "work.observed",
            "summary": "Review password protection and token budgets", "payload": {"body": "API_KEY=${EXAMPLE_API_KEY}"}}})
        self.assertFalse(response["result"].get("isError", False))
        self.no_leak(json.dumps(response))
        with sqlite3.connect(self.root / ".awr/state.db") as db:
            db.execute("UPDATE work_items SET next_action=?,payload_json=json_set(payload_json,'$.next_action',?) WHERE external_key='W'",
                       ("private_prompt: " + SENTINEL, "private_prompt: " + SENTINEL))
        before = self.snapshot()
        for name, arguments in [("awr_context_compile", {"work": "W", "detached": True, "budget": 10000}),
                                ("awr_work_get", {"work": "W"})]:
            self.tool_error(name, arguments)
        for command in ["compile", "bootstrap"]:
            result = self.cli("context", command, "--work", "W", "--budget", "10000")
            self.assertNotEqual(result.returncode, 0)
            self.no_leak(result.stdout + result.stderr)
            self.assertEqual(result.stdout, "")
        self.assertEqual(self.snapshot(), before)
        self.client.log.flush()
        self.client.log.seek(0)
        self.no_leak(self.client.log.read().decode())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--awr", type=Path, required=True)
    parser.add_argument("--mcp", type=Path, required=True)
    args, rest = parser.parse_known_args()
    AWR, MCP = args.awr.resolve(), args.mcp.resolve()
    unittest.main(argv=[__file__] + rest)
