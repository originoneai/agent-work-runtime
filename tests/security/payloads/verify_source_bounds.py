#!/usr/bin/env python3
"""Verify the source-bound subset of SEC-002 without counting its pending safeguards."""
import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import platform
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EXPECTED = {
    "markdown_limit", "yaml_limit", "direct_parser_limit", "source_refresh_limit",
    "git_source_limit", "directory_source_limit", "source_mutation_limit",
    "source_drill_limit", "reader_limit_validation",
}
COMMANDS = [
    ["cargo", "test", "-p", "awr-source", "--test", "security_source_payloads", "--locked", "--", "--nocapture"],
    ["cargo", "test", "-p", "awr-cli", "--test", "security_source_bounds", "--locked", "--", "--nocapture"],
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    assert not args.report.exists(), "Use a new receipt; retain past failures."
    path = Path(__file__).with_name("contract.json")
    contract = json.loads(path.read_text())
    ids = {row["id"] for row in contract["cases"]}
    assert len(ids) == len(contract["cases"]) == contract["target_cases"]
    assert EXPECTED <= ids
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = {"work_item": contract["work_item"], "contract_id": contract["contract_id"],
              "contract_version": contract["version"], "contract_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
              "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"), "platform": platform.platform(),
              "stage": "source_bounds", "stage_target": len(EXPECTED), "contract_target": len(ids),
              "stage_passed": False, "item_completed": False, "e4_completed": 0, "runs": []}
    observed = set()
    for index, command in enumerate(COMMANDS, 1):
        print("Executing " + " ".join(command), flush=True)
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
        log = args.report.with_name(args.report.stem + f"-{index}.log")
        assert not log.exists()
        output = result.stdout + result.stderr
        log.write_text(output)
        totals = re.findall(r"test result: ok\. (\d+) passed; 0 failed; 0 ignored", output)
        passed = result.returncode == 0 and bool(totals) and sum(map(int, totals)) > 0
        emitted = set(re.findall(r"AWR_PAYLOAD_CASE ([a-z][a-z0-9_]*)\b", output))
        if passed:
            observed.update(emitted)
        report["runs"].append({"command": command, "exit_code": result.returncode, "passed": passed,
                               "test_functions_passed": sum(map(int, totals)), "cases_emitted": sorted(emitted),
                               "log": log.name, "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()})
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    report.update(stage_passed=all(r["passed"] for r in report["runs"]) and observed == EXPECTED,
                  verified_conditions=sorted(observed & ids), pending_conditions=sorted(ids - observed),
                  missing_stage_conditions=sorted(EXPECTED - observed), unknown_conditions=sorted(observed - EXPECTED))
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: report[k] for k in ["stage_passed", "stage_target", "contract_target", "item_completed", "missing_stage_conditions", "unknown_conditions"]}))
    return 0 if report["stage_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
