#!/usr/bin/env python3
"""Exercise event boundaries through built CLI and MCP on disposable projects."""
import argparse
import importlib.util
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("event_parity_support", ROOT / "crates/awr-mcp/tests/cli_parity.py")
support = importlib.util.module_from_spec(spec)
spec.loader.exec_module(support)
CAP = 1024 * 1024


class EventTransports(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="awr-event-transports-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        (self.root / "work.yaml").write_text(support.WORK)
        (self.root / "rules.md").write_text("# Authority {severity=hard scope=project value=*}\n\nPreserve source facts.\n")
        (self.root / "goal.md").write_text("# Review customer analysis\n\nDeliver a useful analysis.\n")
        (self.root / "project.toml").write_text(support.MANIFEST)
        self.cli(["init", "--manifest", self.root / "project.toml", "--accept"])
        self.client = support.Client(OPTIONS.mcp, self.root)
        self.addCleanup(self.client.close)

    def cli(self, args, error=False):
        result = subprocess.run([str(OPTIONS.awr), "--project", str(self.root), "--json", *map(str, args)],
                                capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 1 if error else 0, "Unexpected CLI result; input bodies are not diagnostic output")
        body = json.loads(result.stderr if error else result.stdout)
        self.assertFalse(result.stdout if error else result.stderr)
        return body

    def tool(self, args, error=False):
        response = self.client.rpc("tools/call", {"name": "awr_event_append", "arguments": args})
        self.assertNotIn("error", response)
        result = response["result"]
        self.assertEqual(bool(result.get("isError")), error)
        self.assertEqual(json.loads(result["content"][0]["text"]), result["structuredContent"])
        return result["structuredContent"]

    def state(self):
        with sqlite3.connect(f"file:{self.root / '.awr/state.db'}?mode=ro", uri=True) as db:
            tables = [row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
            rows = {name: sorted(repr(row) for row in db.execute('SELECT * FROM "' + name.replace('"', '""') + '"'))
                    for name in tables}
        return rows, {name: (self.root / name).read_bytes()
                      for name in ["work.yaml", "rules.md", "goal.md", ".awr/project.toml"]}

    def revision(self):
        return self.cli(["branch", "list"])["project_revision"]

    def append_cli(self, revision, summary, payload, kind="work.observed", error=False):
        (self.root / "payload.json").write_text(json.dumps(payload, separators=(",", ":"), ensure_ascii=False))
        return self.cli(["event", "append", "--type", kind, "--work", "W", "--summary", summary,
                         "--payload", "payload.json", "--expected-revision", revision], error)

    def reject_both(self, payload, summary="Review observation", kind="work.observed", code="InvalidInput"):
        revision = self.revision()
        before = self.state()
        cli = self.append_cli(revision, summary, payload, kind, error=True)
        self.assertEqual(before, self.state())
        mcp = self.tool({"work": "W", "event_type": kind, "summary": summary,
                         "payload": payload, "expected_revision": revision}, error=True)
        self.assertEqual(before, self.state())
        for response in [cli, mcp]:
            self.assertEqual(response["code"], code)
            self.assertNotIn("UNTRUSTED_NAME_SENTINEL", json.dumps(response))
            self.assertNotIn("UNTRUSTED_VALUE_SENTINEL", json.dumps(response))

    def test_field_validation_and_reference_rejections_are_side_effect_free(self):
        for payload in [
            {"UNTRUSTED_NAME_SENTINEL": "UNTRUSTED_VALUE_SENTINEL"},
            {"private_prompt": "UNTRUSTED_VALUE_SENTINEL"},
            {"body": {"nested": "data"}}, {"status": 1}, {"source_id": 17},
            {"duration_ms": -1}, {"tags": "tag"}, {"metrics": {"elapsed": "slow"}},
        ]:
            self.reject_both(payload)
        self.reject_both({"source_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"}, code="NotFound")
        self.reject_both({}, kind="work.completed")

    def test_byte_and_summary_limits_reject_without_mutations(self):
        self.reject_both({"body": "x" * CAP})
        self.reject_both({}, summary="s" * 8193)
        self.reject_both({}, summary="界" * 2731)
        self.reject_both({}, kind="invalid event type")
        # CLI accepts an exact 1 MiB JSON document; MCP's 1 MiB argument envelope
        # also includes summary/work/revision, so that same payload cannot fit there.
        payload = {"body": "x" * (CAP - len('{"body":""}'))}
        revision = self.revision()
        event = self.append_cli(revision, "Bounded details", payload)
        self.assertEqual(event["event"]["payload"], payload)
        before = self.state()
        result = self.tool({"event_type": "work.observed", "work": "W", "summary": "Bounded details",
                            "payload": payload, "expected_revision": event["project_revision"]}, error=True)
        self.assertEqual(result["code"], "InvalidInput")
        self.assertEqual(before, self.state())

    def test_schema_is_explicit_and_valid_observations_keep_their_data(self):
        response = self.client.rpc("tools/list", {})
        event_tool = next(tool for tool in response["result"]["tools"] if tool["name"] == "awr_event_append")
        schema = event_tool["inputSchema"]["properties"]
        payload_schema = schema["payload"]
        self.assertIs(payload_schema["additionalProperties"], False)
        self.assertNotIn("private_prompt", payload_schema["properties"])
        self.assertEqual(payload_schema["properties"]["metrics"]["additionalProperties"], {"type": "number"})
        self.assertEqual(schema["summary"]["maxLength"], 8192)
        payload = {"status": "reviewed", "body": "Literal observation", "tags": ["review"],
                   "count": 2, "metrics": {"elapsed_ms": 17.5}}
        revision = self.revision()
        cli = self.append_cli(revision, "Review recorded", payload)
        mcp = self.tool({"event_type": "work.observed", "work": "W", "summary": "Review recorded",
                         "payload": payload, "expected_revision": cli["project_revision"]})
        self.assertEqual(cli["event"]["payload"], payload)
        self.assertEqual(mcp["event"]["payload"], payload)
        self.assertEqual(mcp["project_revision"], revision + 2)
        self.assertEqual(self.cli(["work", "show", "W"])["work"]["status"], "ready")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--awr", type=Path, default=ROOT / "target/debug/awr")
    parser.add_argument("--mcp", type=Path, default=ROOT / "target/debug/awr-mcp")
    OPTIONS = parser.parse_args()
    OPTIONS.awr = OPTIONS.awr.resolve(strict=True)
    OPTIONS.mcp = OPTIONS.mcp.resolve(strict=True)
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(EventTransports))
    if result.wasSuccessful():
        print(f"AWR_EVENT_TRANSPORT_CHECKS {result.testsRun}")
    raise SystemExit(0 if result.wasSuccessful() else 1)
