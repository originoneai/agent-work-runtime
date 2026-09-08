#!/usr/bin/env python3
"""Execute the current Doctor and database-recovery contract in isolated projects."""
import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    path = parser.parse_args().report.resolve()
    path.relative_to(ROOT)
    assert not path.exists(), "Retain old receipts and choose a new path."
    path.parent.mkdir(parents=True, exist_ok=True)
    contract_path = Path(__file__).with_name("contract.json")
    contract = json.loads(contract_path.read_text())
    expected = {c["id"] for c in contract["conditions"]}
    assert len(expected) == len(contract["conditions"]) == contract["target_conditions"] == 24
    inputs = ["Cargo.lock", "crates/awr-store/src/lib.rs", "crates/awr-store/src/schema.rs",
              "crates/awr-store/src/reconcile.rs", "crates/awr-runtime/src/doctor.rs",
              "crates/awr-cli/src/doctor.rs", "crates/awr-cli/Cargo.toml",
              "crates/awr-store/migrations/001_catalog.sql", "crates/awr-store/migrations/002_domain.sql",
              "crates/awr-store/migrations/003_search.sql", "tests/recovery/doctor/contract.json",
              "tests/recovery/doctor/database.rs", "tests/recovery/doctor/verify.py"]
    report = {"work_item": contract["work_item"], "contract_version": contract["version"],
              "contract_sha256": digest(contract_path), "contract_target": 24,
              "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"),
              "evidence_level": contract["evidence_level"], "passed": False,
              "item_completed": False, "e4_completed": 0, "stages": [],
              "input_sha256": {name: digest(ROOT / name) for name in inputs}}

    def save():
        path.write_text(json.dumps(report, indent=2) + "\n")

    def execute(stage, args, conditions, exact_tests=None):
        log = path.with_name(path.stem + "-" + stage + ".log")
        assert not log.exists()
        command = ["rtk", "proxy", *args]
        print("Verifying " + stage, flush=True)
        try:
            result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
            output, code = result.stdout + result.stderr, result.returncode
        except subprocess.TimeoutExpired as failure:
            def text(value):
                return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
            output, code = text(failure.stdout) + text(failure.stderr) + "\nTimed out.\n", 124
        log.write_text(output)
        tests = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", output)))
        markers = set(re.findall(r"AWR_DATABASE_CASE ([a-z_]+)", output))
        passed = code == 0 and markers == conditions
        if exact_tests is not None:
            passed = passed and tests == exact_tests
        elif "test" in args:
            passed = passed and tests > 0
        report["stages"].append({"stage": stage, "command": command, "exit_code": code,
            "passed": passed, "test_functions": tests, "expected_conditions": sorted(conditions),
            "observed_conditions": sorted(markers), "verified_conditions": sorted(markers) if passed else [],
            "log": str(log.relative_to(ROOT)), "log_sha256": digest(log)})
        save()

    save()
    execute("database", ["cargo", "test", "-p", "awr-cli", "--test", "doctor_recovery", "case_",
                         "--locked", "--", "--nocapture"], expected, 20)
    execute("store", ["cargo", "test", "-p", "awr-store", "--locked"], set())
    execute("cli", ["cargo", "test", "-p", "awr-cli", "--test", "doctor_cli", "--test", "source_cli",
                    "--test", "session_cli", "--test", "output_cli", "--test", "bootstrap_cli", "--test", "records_cli",
                    "--test", "proposal_cli", "--test", "work_action_cli", "--test", "work_complete_cli", "--locked"], set())
    execute("mcp", ["cargo", "test", "-p", "awr-mcp", "--test", "stdio", "--locked"], set())
    execute("build", ["cargo", "build", "-p", "awr-cli", "-p", "awr-mcp", "--locked"], set())
    execute("check", ["cargo", "check", "--workspace", "--all-targets", "--locked"], set())
    execute("format", ["cargo", "fmt", "--all", "--check"], set())
    execute("diff", ["git", "diff", "--check"], set())
    observed = {condition for stage in report["stages"] for condition in stage["verified_conditions"]}
    unchanged = all(digest(ROOT / name) == value for name, value in report["input_sha256"].items())
    report.update(passed=all(stage["passed"] for stage in report["stages"]) and observed == expected and unchanged,
                  inputs_unchanged=unchanged, verified_count=len(observed), verified_conditions=sorted(observed),
                  missing_conditions=sorted(expected - observed), unknown_conditions=sorted(observed - expected),
                  test_functions_passed=sum(stage["test_functions"] for stage in report["stages"] if stage["passed"]),
                  binary_sha256={name: digest(ROOT / "target/debug" / name) for name in ["awr", "awr-mcp"]})
    save()
    print(json.dumps({key: report[key] for key in ["passed", "verified_count", "contract_target", "test_functions_passed", "missing_conditions", "item_completed", "e4_completed"]}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
