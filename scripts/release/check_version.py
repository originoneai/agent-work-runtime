#!/usr/bin/env python3
"""Reject mixed binary/package versions before the build or any publication."""
import json
import os
from pathlib import Path
import tomllib
from build_packages import ROOT, python_version


def check():
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    normalized = python_version(version)
    expected = os.environ.get("EXPECTED_RELEASE_VERSION")
    assert not expected or expected == version, "requested version differs from the source tree"
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    packages = [p for p in lock["package"] if p["name"].startswith("awr-")]
    assert len(packages) == 7 and all(p["version"] == version for p in packages)
    npm = json.loads((ROOT / "packaging/npm/package.json").read_text())
    assert npm["version"] == version
    assert len(npm["optionalDependencies"]) == 3 and set(npm["optionalDependencies"].values()) == {version}
    tag = "next" if "-" in version else "latest"
    assert npm["publishConfig"]["tag"] == tag
    print(json.dumps({"version": version, "python_version": normalized, "npm_tag": tag}))


if __name__ == "__main__":
    check()
