#!/usr/bin/env python3
"""Collect one fully checked native build per supported platform for publication."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

PLATFORMS = {"darwin-arm64", "linux-x64-gnu", "win32-x64"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("input", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expected-sha", required=True)
    args = parser.parse_args()
    manifests = list(args.input.rglob("manifest.json"))
    assert len(manifests) == 3, "three independent platform build manifests are required"
    selected = {}
    versions = set()
    files = {}
    for path in manifests:
        manifest = json.loads(path.read_text())
        platform = manifest["platform"]
        assert platform in PLATFORMS and platform not in selected, "duplicate or unsupported platform"
        assert manifest["source_sha"] == args.expected_sha and manifest["source_tree_clean"], "build source identity mismatch"
        assert "-" in manifest["version"], "this workflow publishes development previews only"
        receipt = json.loads((path.parent / "installation-checks.json").read_text())
        assert receipt["source_sha"] == args.expected_sha and receipt["platform"] == platform
        assert receipt["artifact_sha256"] == manifest["artifacts"], "installation proof covers different artifacts"
        for ecosystem in ("npm", "python"):
            check = receipt[ecosystem]
            assert check["version_and_help"] is True and check["exit_code_and_stderr"] is True
            assert check["unicode_space_project_init_and_status"] is True and len(set(check["mcp_stdio_tools"])) == 8
        versions.add(manifest["version"])
        for name, expected in manifest["artifacts"].items():
            assert Path(name).name == name and name.endswith((".whl", ".tgz"))
            source = path.parent / "publish" / name
            actual = hashlib.sha256(source.read_bytes()).hexdigest()
            assert actual == expected, f"artifact hash mismatch: {name}"
            if name in files:
                assert files[name][1] == actual, "platform-independent npm wrapper differs across builds"
            files[name] = (source, actual)
        selected[platform] = manifest
    assert set(selected) == PLATFORMS and len(versions) == 1
    assert sum(name.endswith(".whl") for name in files) == 3
    assert sum(name.endswith(".tgz") for name in files) == 4
    args.output.mkdir(parents=True, exist_ok=False)
    for ecosystem in ("npm", "python"):
        (args.output / ecosystem).mkdir()
    for name, (source, _) in files.items():
        ecosystem = "python" if name.endswith(".whl") else "npm"
        shutil.copy2(source, args.output / ecosystem / name)
    receipt = {"version": versions.pop(), "source_sha": args.expected_sha,
               "platforms": sorted(selected), "artifacts": {name: sha for name, (_, sha) in files.items()}}
    (args.output / "release-manifest.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt))


if __name__ == "__main__":
    main()
