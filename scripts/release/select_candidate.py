#!/usr/bin/env python3
"""Select a successful GitHub build without rebuilding its verified package bytes."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    candidate = json.loads(args.candidate.read_text())
    repository = candidate["repository"]
    assert repository == "originoneai/agent-work-runtime"
    assert os.environ.get("GITHUB_REPOSITORY", repository) == repository
    assert "-" in candidate["version"], "development preview required"
    assert args.tag == "pypi-preview-v" + candidate["python_version"], "tag does not match candidate version"
    assert re.fullmatch(r"[0-9a-f]{40}", candidate["source_sha"])
    run_id = candidate["build_run_id"]
    assert isinstance(run_id, int) and run_id > 0
    run = json.loads(subprocess.check_output(
        ["gh", "api", f"repos/{repository}/actions/runs/{run_id}"], text=True))
    assert run["status"] == "completed" and run["conclusion"] == "success", "candidate build has not passed"
    assert run["head_sha"] == candidate["source_sha"], "candidate build commit differs"
    assert run["path"] == ".github/workflows/distributions.yml"
    assert run["event"] in ("push", "workflow_dispatch"), "pull request artifacts cannot be promoted"
    if args.github_output:
        args.github_output.write_text(f"run_id={run_id}\nsource_sha={candidate['source_sha']}\n", encoding="utf-8")
    print(json.dumps({"run_id": run_id, "source_sha": candidate["source_sha"], "tag": args.tag}))


if __name__ == "__main__":
    main()
