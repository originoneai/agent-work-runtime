#!/usr/bin/env python3
"""Publish already-assembled npm preview artifacts; authentication remains external."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--provenance", action="store_true", help="enable when publishing from the configured CI identity")
    args = parser.parse_args()
    manifest = json.loads((args.directory / "release-manifest.json").read_text())
    version = manifest["version"]
    assert "-" in version, "preview version required"
    packages = []
    for file in (args.directory / "npm").glob("*.tgz"):
        assert hashlib.sha256(file.read_bytes()).hexdigest() == manifest["artifacts"][file.name]
        with tarfile.open(file) as archive:
            package = json.load(archive.extractfile("package/package.json"))
        assert package["version"] == version and package["name"].startswith("@originoneai/agent-work-runtime")
        packages.append((package["name"], file))
    assert len(packages) == 4
    # Installers may fetch the wrapper as soon as it exists, so its dependencies go first.
    packages.sort(key=lambda p: p[0] == "@originoneai/agent-work-runtime")
    for name, file in packages:
        command = ["npm", "publish", str(file), "--access", "public", "--tag", "next",
                   "--registry=https://registry.npmjs.org/"]
        if args.provenance:
            command.append("--provenance")
        subprocess.run(command, check=True)
        print(json.dumps({"published": name, "version": version}))


if __name__ == "__main__":
    main()
