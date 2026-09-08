#!/usr/bin/env python3
"""Re-execute every SEC-002 condition under one contract; never inherit old counts."""
import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    path = args.report.resolve()
    assert not path.exists(), "Use a new receipt; retain past failures."
    path.parent.mkdir(parents=True, exist_ok=True)
    contract_path = ROOT / "tests/security/payloads/contract.json"
    contract = json.loads(contract_path.read_text())
    contract_hash = hashlib.sha256(contract_path.read_bytes()).hexdigest()
    expected = {c["id"] for c in contract["cases"]}
    assert len(expected) == len(contract["cases"]) == contract["target_cases"] == 32
    report = {"work_item": contract["work_item"], "contract_version": contract["version"],
              "contract_sha256": contract_hash, "contract_target": len(expected),
              "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"),
              "passed": False, "item_completed": False, "e4_completed": 0, "stages": []}
    def save():
        path.write_text(json.dumps(report, indent=2) + "\n")
    save()
    command = ["rtk", "proxy", "cargo", "build", "-p", "awr-cli", "-p", "awr-mcp", "--locked"]
    build = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
    build_log = path.with_name(path.stem + "-build.log")
    assert not build_log.exists()
    build_log.write_text(build.stdout + build.stderr)
    report["build"] = {"command": command, "exit_code": build.returncode, "log": str(build_log.relative_to(ROOT)),
                       "log_sha256": hashlib.sha256(build_log.read_bytes()).hexdigest()}
    save()
    if build.returncode:
        return 1
    observed = set()
    for stage, script in [("sources", "verify_source_bounds.py"), ("events", "verify_events.py"),
                          ("artifacts", "verify_artifacts.py"), ("secrets", "verify_secrets.py")]:
        output = path.with_name(path.stem + "-" + stage + ".json")
        print("Verifying " + stage, flush=True)
        result = subprocess.run(["rtk", "proxy", sys.executable, str(contract_path.with_name(script)),
                                 "--report", str(output)], cwd=ROOT, timeout=900)
        item = json.loads(output.read_text()) if output.exists() else {}
        passing = result.returncode == 0 and item.get("stage_passed") is True and item.get("contract_sha256") == contract_hash
        cases = set(item.get("verified_conditions", [])) if passing else set()
        assert not (cases & observed), "Stage conditions must be disjoint."
        observed |= cases
        report["stages"].append({"stage": stage, "passed": passing, "verified_conditions": sorted(cases),
                                  "report": str(output.relative_to(ROOT)),
                                  "sha256": hashlib.sha256(output.read_bytes()).hexdigest() if output.exists() else None})
        save()
    report.update(passed=all(s["passed"] for s in report["stages"]) and observed == expected,
                  verified_conditions=sorted(observed), missing_conditions=sorted(expected - observed),
                  unknown_conditions=sorted(observed - expected), verified_count=len(observed))
    report["binary_sha256"] = {name: hashlib.sha256((ROOT / "target/debug" / name).read_bytes()).hexdigest() for name in ["awr", "awr-mcp"]}
    save()
    print(json.dumps({k: report[k] for k in ["passed", "verified_count", "contract_target", "missing_conditions", "item_completed", "e4_completed"]}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
