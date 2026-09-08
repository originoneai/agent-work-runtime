#!/usr/bin/env python3
"""Execute the fixed mutation recovery matrix, including real process death and CLI recovery."""
import argparse
from datetime import datetime
import hashlib
import json
import os
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
    assert not path.exists(), "Use a new receipt; retain past failures."
    path.relative_to(ROOT)
    path.parent.mkdir(parents=True, exist_ok=True)
    contract_path = Path(__file__).with_name("contract.json")
    contract = json.loads(contract_path.read_text())
    expected = {item["id"] for item in contract["cases"]}
    assert len(expected) == len(contract["cases"]) == contract["target_cases"] == 20
    report = {
        "work_item": contract["work_item"], "contract_version": contract["version"],
        "contract_sha256": digest(contract_path), "contract_target": len(expected),
        "evidence_level": contract["evidence_level"],
        "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "passed": False, "item_completed": False, "e4_completed": 0, "stages": [],
        "inputs": {str(p.relative_to(ROOT)): digest(p) for p in [
            Path(__file__), Path(__file__).with_name("engine.rs"), contract_path,
            ROOT / "crates/awr-runtime/src/mutation_apply.rs", ROOT / "Cargo.lock"]},
    }

    def save():
        path.write_text(json.dumps(report, indent=2) + "\n")

    def execute(stage, args, conditions, environment=None):
        command = ["rtk", "proxy", *args]
        log = path.with_name(path.stem + "-" + stage + ".log")
        assert not log.exists()
        print("Verifying " + stage, flush=True)
        try:
            result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                                    timeout=300, env=environment)
            output, code = result.stdout + result.stderr, result.returncode
        except subprocess.TimeoutExpired as failure:
            def text(value):
                return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
            output = text(failure.stdout) + text(failure.stderr) + "\nVerification timed out.\n"
            code = 124
        log.write_text(output)
        observed = set(re.findall(r"AWR_RECOVERY_CASE ([a-z_]+)", output))
        passing = code == 0 and observed == conditions
        item = {"stage": stage, "command": command, "exit_code": code, "passed": passing,
                "verified_conditions": sorted(observed) if passing else [],
                "observed_markers": sorted(observed), "expected_conditions": sorted(conditions),
                "log": str(log.relative_to(ROOT)), "log_sha256": digest(log)}
        if stage == "cli":
            item["binary_sha256"] = digest(ROOT / "target/debug/awr")
        report["stages"].append(item)
        save()
        return passing

    save()
    cli_case = {"recovery_once_and_cli_receipts"}
    execute("engine", ["cargo", "test", "-p", "awr-runtime", "--lib",
                       "recovery_tests::case_", "--locked", "--", "--nocapture"], expected - cli_case)
    if execute("build", ["cargo", "build", "-p", "awr-cli", "-p", "awr-mcp", "--locked"], set()):
        environment = os.environ.copy()
        environment["AWR_RECOVERY_CLI"] = str(ROOT / "target/debug/awr")
        execute("cli", ["cargo", "test", "-p", "awr-runtime", "--lib",
                        "mutation_apply::recovery_tests::cli_recovery_after_process_death_binds_original_attempt_and_resolves_once",
                        "--locked", "--", "--exact", "--ignored", "--nocapture"], cli_case, environment)
        report["binary_sha256"] = {name: digest(ROOT / "target/debug" / name) for name in ["awr", "awr-mcp"]}
    observed = {case for stage in report["stages"] for case in stage["verified_conditions"]}
    report.update(passed=len(report["stages"]) == 3 and all(s["passed"] for s in report["stages"]) and observed == expected,
                  verified_conditions=sorted(observed), verified_count=len(observed),
                  missing_conditions=sorted(expected - observed), unknown_conditions=sorted(observed - expected))
    save()
    print(json.dumps({key: report[key] for key in ["passed", "verified_count", "contract_target", "missing_conditions", "item_completed", "e4_completed"]}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
