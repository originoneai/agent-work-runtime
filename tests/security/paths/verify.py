#!/usr/bin/env python3
"""Execute the versioned path contract against its Rust source/runtime/CLI handlers."""
import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import platform
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
CONTRACT = Path(__file__).with_name("contract.json")
COMMANDS = [
    ["cargo", "test", "-p", "awr-source", "--test", "security_paths", "--locked", "--", "--nocapture"],
    ["cargo", "test", "-p", "awr-runtime", "--lib", "mutation_apply::tests", "--locked", "--", "--nocapture"],
    ["cargo", "test", "-p", "awr-cli", "--test", "security_write", "--locked", "--", "--nocapture"],
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True, help="New JSON receipt path; raw logs are stored next to it")
    args = parser.parse_args()
    assert not args.report.exists(), "Use a new receipt path; retain prior execution evidence."
    contract = json.loads(CONTRACT.read_text())
    rows = contract["cases"]
    ids = [row["id"] for row in rows]
    assert len(ids) == len(set(ids)) == contract["target_cases"]
    assert all(re.fullmatch(r"[a-z][a-z0-9_]*", key) for key in ids)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = {"contract_id": contract["contract_id"], "contract_version": contract["version"],
              "contract_sha256": hashlib.sha256(CONTRACT.read_bytes()).hexdigest(),
              "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"),
              "platform": platform.platform(), "target_cases": len(ids), "runs": [],
              "passed": False, "evidence_level": "local_component_and_cli", "e4_completed": 0}
    observed = set()
    for i, command in enumerate(COMMANDS, 1):
        print("Executing " + " ".join(command), flush=True)
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
        output = result.stdout + result.stderr
        log = args.report.with_name(args.report.stem + f"-{i}.log")
        assert not log.exists(), "Do not overwrite an existing log."
        log.write_text(output)
        cases = sorted(set(re.findall(r"AWR_PATH_CASE ([a-z][a-z0-9_]*)\b", output)))
        totals = re.findall(r"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored", output)
        verified = result.returncode == 0 and bool(totals) and any(int(count) > 0 for count, _ in totals)
        if verified:
            observed.update(cases)
        report["runs"].append({"command": command, "exit_code": result.returncode, "passed": verified,
                               "test_functions_passed": sum(int(count) for count, _ in totals),
                               "cases_emitted": cases, "log": log.name,
                               "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()})
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    missing, unknown = sorted(set(ids) - observed), sorted(observed - set(ids))
    report.update(passed=all(run["passed"] for run in report["runs"]) and not missing and not unknown,
                  covered_cases=sorted(observed & set(ids)), missing_cases=missing, unknown_cases=unknown,
                  coverage=[{**row, "verified": row["id"] in observed} for row in rows])
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "covered": len(report["covered_cases"]),
                      "target": len(ids), "missing": missing, "unknown": unknown}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
