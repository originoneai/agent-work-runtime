#!/usr/bin/env python3
"""Release-channel and artifact-integrity checks without registry writes."""
from contextlib import redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import assemble_release
from build_packages import python_version
import publish_npm


class ReleaseChecks(unittest.TestCase):
    def test_stable_and_prerelease_version_mapping(self):
        for original, expected in {
            "0.2.0": "0.2.0", "0.3.0-dev": "0.3.0.dev0",
            "0.3.0-alpha.2": "0.3.0a2", "0.3.0-beta.3": "0.3.0b3", "0.3.0-rc.1": "0.3.0rc1",
        }.items():
            self.assertEqual(python_version(original), expected)
        for invalid in ["latest", "0.2", "0.2.0+dirty", "0.2.0-preview"]:
            with self.assertRaises(ValueError):
                python_version(invalid)

    def package_set(self, root, version):
        npm = root / "npm"
        npm.mkdir()
        artifacts = {}
        tag = "next" if "-" in version else "latest"
        for suffix in ["", "-darwin-arm64", "-linux-x64-gnu", "-win32-x64"]:
            path = npm / ("package" + suffix + ".tgz")
            metadata = json.dumps({"name": "@originoneai/agent-work-runtime" + suffix,
                                   "version": version, "publishConfig": {"tag": tag}}).encode()
            with tarfile.open(path, "w:gz") as archive:
                info = tarfile.TarInfo("package/package.json")
                info.size = len(metadata)
                archive.addfile(info, io.BytesIO(metadata))
            artifacts[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
        manifest = {"version": version, "channel": tag, "platforms": sorted(assemble_release.PLATFORMS), "artifacts": artifacts}
        (root / "release-manifest.json").write_text(json.dumps(manifest))
        return manifest

    def test_publish_stable_latest_and_prerelease_next_with_wrapper_last(self):
        for version, tag in [("0.2.0", "latest"), ("0.3.0-rc.1", "next")]:
            with self.subTest(version=version), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                self.package_set(root, version)
                with patch.object(sys, "argv", ["publish_npm.py", str(root)]), patch("publish_npm.subprocess.run") as run, redirect_stdout(io.StringIO()):
                    publish_npm.main()
                commands = [call.args[0] for call in run.call_args_list]
                self.assertEqual(len(commands), 4)
                self.assertTrue(all(c[c.index("--tag")+1] == tag for c in commands))
                self.assertEqual(Path(commands[-1][2]).name, "package.tgz")

    def test_wrong_channel_and_tampered_archive_never_publish(self):
        for fault in ["channel", "artifact"]:
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                manifest = self.package_set(root, "0.2.0")
                if fault == "channel":
                    manifest["channel"] = "next"
                    (root / "release-manifest.json").write_text(json.dumps(manifest))
                else:
                    with (root / "npm/package.tgz").open("ab") as stream:
                        stream.write(b"changed")
                with patch.object(sys, "argv", ["publish_npm.py", str(root)]), patch("publish_npm.subprocess.run") as run:
                    with self.assertRaises(AssertionError):
                        publish_npm.main()
                    run.assert_not_called()

    def assembled_inputs(self, root):
        source = "1" * 40
        for platform in sorted(assemble_release.PLATFORMS):
            base = root / platform
            (base / "publish").mkdir(parents=True)
            artifacts = {}
            for name, content in [(platform+".whl", platform.encode()), (platform+".tgz", platform.encode()), ("wrapper.tgz", b"same-wrapper")]:
                (base / "publish" / name).write_bytes(content)
                artifacts[name] = hashlib.sha256(content).hexdigest()
            manifest = {"platform": platform, "version": "0.2.0", "python_version": "0.2.0", "source_sha": source,
                        "source_tree_clean": True, "artifacts": artifacts}
            (base / "manifest.json").write_text(json.dumps(manifest))
            checks = {"version_and_help": True, "exit_code_and_stderr": True,
                      "unicode_space_project_init_and_status": True, "task_context_and_intake": True,
                      "mcp_stdio_tools": [f"tool-{i}" for i in range(8)]}
            receipt = {"source_sha": source, "platform": platform, "artifact_sha256": artifacts, "npm": checks, "python": checks}
            (base / "installation-checks.json").write_text(json.dumps(receipt))
        return source

    def test_assembly_requires_matching_installation_and_source_receipts(self):
        for fault in [None, "source", "installation", "bytes"]:
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                source = self.assembled_inputs(root / "inputs")
                platform = root / "inputs/darwin-arm64"
                if fault == "bytes":
                    (platform / "publish/darwin-arm64.whl").write_bytes(b"different")
                elif fault:
                    path = platform / ("manifest.json" if fault == "source" else "installation-checks.json")
                    data = json.loads(path.read_text())
                    if fault == "source":
                        data["source_sha"] = "2" * 40
                    else:
                        data["python"]["task_context_and_intake"] = False
                    path.write_text(json.dumps(data))
                with patch.object(sys, "argv", ["assemble_release.py", str(root/"inputs"), "--output", str(root/"release"), "--expected-sha", source]), redirect_stdout(io.StringIO()):
                    if fault:
                        with self.assertRaises(AssertionError):
                            assemble_release.main()
                        self.assertFalse((root / "release").exists())
                    else:
                        assemble_release.main()
                        result = json.loads((root / "release/release-manifest.json").read_text())
                        self.assertEqual((result["version"], result["python_version"], result["channel"]), ("0.2.0", "0.2.0", "latest"))
                        self.assertEqual(len(result["artifacts"]), 7)


if __name__ == "__main__":
    unittest.main()
