import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from host import CommandFailed, Host, demonstration, digest


class HostTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="awr 宿主 测试 ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        fixture = Path(__file__).resolve().parents[2] / "tests/fixtures/host-app"
        self.project = self.root / "中文 项目"
        shutil.copytree(fixture, self.project)
        self.binary = Path(os.environ["AWR_TEST_BINARY"]).resolve()
        self.host = Host(self.binary, digest(self.binary), self.project, self.root / "私有 回执")

    def test_intake_catalog_progress_checkpoint_and_explicit_successor(self):
        result = demonstration(self.host)
        self.assertEqual(result["work_total"], 25)
        self.assertEqual(result["pages"], 2)
        self.assertTrue(result["source_bytes_unchanged"])
        self.assertNotEqual(result["first_session"], result["successor_session"])
        self.assertFalse((self.project / ".kimi").exists())
        self.assertFalse((self.project / ".codex").exists())

    def test_discovery_and_wrong_pin_do_not_initialize_project(self):
        capability = self.host.discover(os.environ["AWR_TEST_VERSION"], ["project.catalog"])
        self.assertFalse(capability["runtime_write_performed"])
        self.assertFalse((self.project / ".awr").exists())
        with self.assertRaises(ValueError):
            Host(self.binary, "0" * 64, self.project, self.root / "should not exist")
        self.assertFalse((self.root / "should not exist").exists())

    def test_partial_source_failure_preserves_both_streams(self):
        self.host.ok("init", "--manifest", "mapping.toml", "--accept")
        (self.project / "工作 台账.yaml").write_text("work_items: [broken\n", encoding="utf-8")
        with self.assertRaises(CommandFailed) as failure:
            self.host.catalog("work")
        result = failure.exception.result
        self.assertNotEqual(result.exit_code, 0)
        self.assertEqual(result.error["code"], "SourceStale")
        self.assertFalse(result.value["total_is_current"])
        self.assertTrue(result.receipt.is_file())
        self.assertFalse(result.outcome_unknown)

    def test_timeout_is_unknown_and_is_never_retried(self):
        with patch("host.subprocess.run", side_effect=subprocess.TimeoutExpired(
                [str(self.binary)], 1, output=b'{"partial":')) as run:
            result = self.host.call("source", "reindex")
            self.assertEqual(run.call_count, 1)
        self.assertTrue(result.outcome_unknown)
        self.assertIsNone(result.exit_code)
        self.assertEqual(result.stdout, b'{"partial":')
        self.assertNotIn("source_write_performed", result.receipt.read_text())
        with self.assertRaises(CommandFailed):
            result.require()

    def test_inputs_are_not_shell_code_and_files_are_private(self):
        malicious_key = "key $(touch should-not-exist); `touch also-not`"
        result = self.host.input(["execution", "report", "--expected-revision", "0"],
                                 {"request_key": malicious_key})
        self.assertNotEqual(result.exit_code, 0)  # Invalid report, preserved typed error.
        self.assertFalse((self.project / "should-not-exist").exists())
        self.assertFalse((self.project / "also-not").exists())
        if os.name == "posix":
            self.assertEqual(self.host.receipts.stat().st_mode & 0o777, 0o700)
            for path in self.host.receipts.iterdir():
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)


if __name__ == "__main__":
    unittest.main()
