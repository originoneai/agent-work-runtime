#!/usr/bin/env python3
"""Execute every current event/claim/branch isolation condition and its related transports."""
import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    path = parser.parse_args().report.resolve()
    assert not path.exists(), "Use a new receipt and retain past failures."
    path.relative_to(ROOT)
    path.parent.mkdir(parents=True, exist_ok=True)
    contract_path = Path(__file__).with_name("contract.json")
    contract = json.loads(contract_path.read_text())
    expected = {case["id"] for case in contract["cases"]}
    assert len(expected) == len(contract["cases"]) == contract["target_cases"] == 20
    supplement = contract["required_transport_supplement"]
    assert supplement["file"] == "tests/concurrency/branch_selectors.py"
    assert supplement["test_functions"] == 3 and supplement["case"] in expected
    report = {
        "work_item": contract["work_item"], "contract_version": contract["version"],
        "contract_sha256": digest(contract_path), "contract_target": 20,
        "required_transport_supplement": supplement,
        "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "evidence_level": contract["evidence_level"], "passed": False,
        "item_completed": False, "e4_completed": 0, "stages": [],
        "input_sha256": {name: digest(ROOT / name) for name in [
            "crates/awr-store/src/lib.rs", "crates/awr-store/Cargo.toml", "Cargo.lock",
            "crates/awr-store/src/transaction.rs", "crates/awr-runtime/src/lib.rs",
            "crates/awr-cli/src/event_append.rs", "crates/awr-mcp/src/operations.rs",
            "tests/concurrency/contract.json", "tests/concurrency/event_guards.rs",
            "tests/concurrency/isolation.rs", "tests/concurrency/verify.py",
            "tests/concurrency/branch_selectors.py", "crates/awr-mcp/tests/cli_parity.py",
            "crates/awr-store/tests/support/mod.rs"]},
    }

    def save():
        path.write_text(json.dumps(report, indent=2) + "\n")

    def execute(stage, arguments, conditions, exact_tests=None):
        command = ["rtk", "proxy", *arguments]
        log = path.with_name(path.stem + "-" + stage + ".log")
        assert not log.exists()
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
        if stage == "branch_selectors":
            tests = sum(map(int, re.findall(r"Ran (\d+) tests? in", output)))
        observed = set(re.findall(r"AWR_ISOLATION_CASE ([a-z_]+)", output))
        api_errors = set(re.findall(r"error\[(E\d+)\]", output))
        if stage == "public_api" and tests == 2 and api_errors == {"E0616", "E0624"}:
            observed.add("event_opaque_storage_api")
        passing = code == 0 and observed == conditions
        if exact_tests is not None:
            passing = passing and tests == exact_tests
        elif "test" in arguments:
            passing = passing and tests > 0
        report["stages"].append({
            "stage": stage, "command": command, "exit_code": code, "passed": passing,
            "test_functions": tests, "verified_conditions": sorted(observed) if passing else [],
            "observed_markers": sorted(observed), "expected_conditions": sorted(conditions),
            "compiler_error_codes": sorted(api_errors),
            "log": str(log.relative_to(ROOT)), "log_sha256": digest(log),
        })
        save()

    save()
    execute("event_rows", ["cargo", "test", "-p", "awr-store", "--lib", "isolation_event_tests::",
                            "--locked", "--", "--nocapture"], {"event_row_immutability"}, 1)
    execute("public_api", ["cargo", "test", "-p", "awr-store", "--doc", "Store",
                            "--locked", "--", "--nocapture"], {"event_opaque_storage_api"}, 2)
    execute("domain", ["cargo", "test", "-p", "awr-store", "--test", "concurrency_isolation",
                        "case_", "--locked", "--", "--nocapture"],
            expected - {"event_row_immutability", "event_opaque_storage_api"}, 12)
    execute("related_store", ["cargo", "test", "-p", "awr-store", "--test", "events",
                               "--test", "sessions", "--test", "reconcile", "--test", "checkpoints",
                               "--test", "checkpoint_save", "--test", "search", "--locked"], set())
    execute("cli", ["cargo", "test", "-p", "awr-cli", "--test", "session_cli", "--test", "branch_cli",
                     "--test", "branch_close_cli", "--test", "resume_cli", "--test", "doctor_cli",
                     "--test", "records_cli", "--locked"], set())
    execute("mcp", ["cargo", "test", "-p", "awr-mcp", "--test", "stdio", "--locked"], set())
    execute("build", ["cargo", "build", "-p", "awr-cli", "-p", "awr-mcp", "--locked"], set())
    execute("branch_selectors", [sys.executable, supplement["file"],
                                  "--awr", "target/debug/awr", "--mcp", "target/debug/awr-mcp"],
            set(), supplement["test_functions"])
    observed = {condition for stage in report["stages"] for condition in stage["verified_conditions"]}
    if not report["stages"][-1]["passed"]:
        observed.discard(supplement["case"])
    unchanged = all(digest(ROOT / name) == value for name, value in report["input_sha256"].items())
    report.update(passed=all(stage["passed"] for stage in report["stages"]) and observed == expected and unchanged,
                  inputs_unchanged=unchanged,
                  verified_count=len(observed), verified_conditions=sorted(observed),
                  missing_conditions=sorted(expected - observed), unknown_conditions=sorted(observed - expected),
                  test_functions_passed=sum(stage["test_functions"] for stage in report["stages"] if stage["passed"]),
                  binary_sha256={name: digest(ROOT / "target/debug" / name) for name in ["awr", "awr-mcp"]})
    save()
    print(json.dumps({key: report[key] for key in ["passed", "verified_count", "contract_target", "test_functions_passed", "missing_conditions", "item_completed", "e4_completed"]}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
