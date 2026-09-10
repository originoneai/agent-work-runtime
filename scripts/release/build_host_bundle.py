#!/usr/bin/env python3
"""Build a pinned native host payload; only the build machine needs development tools."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import tomllib
import zipfile

from build_packages import ROOT, TARGETS, digest, license_bundle, run, write_json
from check_version import check


def clean():
    if run(["git", "status", "--porcelain"]):
        raise ValueError("commit reviewed inputs before building a host payload")


def native(binary, *args):
    # Absolute executable path, no inherited PATH, interpreter or model configuration.
    result = subprocess.run([str(binary), *args], env={}, stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, timeout=30)
    if result.returncode:
        raise ValueError(f"native startup failed: {binary.name}: {result.stderr}")
    return result.stdout.strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--batch", required=True, help="immutable build batch identifier")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9.-]{1,40}", args.batch):
        raise ValueError("batch must contain 1..40 ASCII identifier characters")
    info = TARGETS.get(f"{platform.system()}-{platform.machine()}")
    if not info:
        raise ValueError("unsupported native build host")
    target, rust_target, _, npm_os, _ = info
    clean()
    check()
    sha = run(["git", "rev-parse", "HEAD"])
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    identity = f"awr-host-{version}-{target}-{sha[:12]}-{args.batch}"
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    payload = out / identity
    (payload / "bin").mkdir(parents=True)
    env = dict(os.environ)
    if platform.system() == "Darwin":
        env["MACOSX_DEPLOYMENT_TARGET"] = "15.0"
    target_dir = ROOT / ".local/host-dist-build"
    run(["cargo", "build", "--locked", "--release", "-p", "awr-cli", "-p", "awr-mcp",
         "--target-dir", target_dir], env=env)
    suffix = ".exe" if npm_os == "win32" else ""
    versions = {}
    for name in ("awr", "awr-mcp"):
        dest = payload / "bin" / (name + suffix)
        shutil.copy2(target_dir / "release" / dest.name, dest)
        versions[name] = native(dest, "--version")
        if version not in versions[name]:
            raise ValueError("native payload version differs from committed source")
    capabilities = json.loads(native(payload / "bin" / ("awr" + suffix), "--json", "capabilities",
                                    "--require", "mutation.multi_file", "--require", "mutation.batch.ledger",
                                    "--require", "completion.user_confirmation"))
    notices, inventory = license_bundle(rust_target)
    shutil.copyfile(ROOT / "LICENSE", payload / "LICENSE")
    (payload / "THIRD_PARTY_LICENSES.txt").write_text(notices, encoding="utf-8")
    write_json(payload / "capabilities.json", capabilities)
    (payload / "README.txt").write_text(
        "AWR native host payload\nInvoke bin/awr and bin/awr-mcp by absolute path.\n"
        "No Node, Python, Rust, package installer or PATH configuration is needed at runtime.\n"
        "Git is only needed for Git-bound source and execution features. SQLite is bundled.\n"
        "Verify SHA256SUMS and build.json before embedding. The host owns application signing.\n"
        "Keep matched program, runtime database/history, configuration, journals, artifacts and source backups.\n"
        "Consult the source repository docs/release/DISTRIBUTIONS.md for upgrade and rollback.\n",
        encoding="utf-8")
    metadata = dict(version=version, source_sha=sha, identity=identity, build_batch=args.batch,
                    platform=target, rust_target=rust_target, minimum_os="macOS 15" if npm_os == "darwin" else None,
                    schema_version=capabilities["database"]["schema_version"], host_protocol=capabilities["protocol"],
                    created_at=datetime.now(timezone.utc).isoformat(), source_tree_clean=True,
                    binary_sha256={p.name: digest(p) for p in sorted((payload / "bin").iterdir())},
                    program_versions=versions, startup_environment="empty_environment_absolute_native_executable",
                    runtime_language_dependency=False, application_signing="host_responsibility",
                    license="Apache-2.0", dependency_notices=inventory,
                    evidence_boundary="native host payload; application integration and signed distribution are separate")
    if run(["git", "rev-parse", "HEAD"]) != sha:
        raise ValueError("source HEAD changed during build")
    clean()
    write_json(payload / "build.json", metadata)
    checksums = {str(p.relative_to(payload)): digest(p) for p in sorted(payload.rglob("*")) if p.is_file()}
    (payload / "SHA256SUMS").write_text("".join(f"{sha}  {name}\n" for name, sha in checksums.items()), encoding="utf-8")
    if npm_os == "win32":
        archive = out / (identity + ".zip")
        with zipfile.ZipFile(archive, "x", zipfile.ZIP_DEFLATED) as z:
            for p in sorted(payload.rglob("*")):
                if p.is_file():
                    z.write(p, p.relative_to(out))
    else:
        archive = out / (identity + ".tar.gz")
        with tarfile.open(archive, "x:gz") as t:
            t.add(payload, arcname=identity)
    manifest = dict(version=1, identity=identity, source_sha=sha, payload=str(payload),
                    archive=archive.name, archive_sha256=digest(archive), files=checksums,
                    startup_checked=True, platform=target)
    write_json(out / "host-manifest.json", manifest)
    print(json.dumps(manifest))


if __name__ == "__main__":
    main()
