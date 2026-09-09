#!/usr/bin/env python3
"""Run the entire versioned regression gate, retaining every failure and raw receipt.

Use the project's PyYAML-enabled Python on a clean committed checkout. All stages
run serially. This runner never mutates the actual project's AWR database.
"""
import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = Path(__file__).with_name("contract.json")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_command(*arguments):
    return subprocess.check_output(["rtk", "proxy", *arguments], cwd=ROOT, text=True).strip()


def inputs():
    names = read_command("git", "ls-files", "-z").split("\0")
    return {name: digest(ROOT / name) for name in sorted(names) if name}


def tree_hash(values):
    return hashlib.sha256(json.dumps(values, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New directory below .local; never reuse a previous run")
    output = parser.parse_args().output.resolve()
    output.relative_to(ROOT / ".local")
    require(not read_command("git", "status", "--porcelain"), "Commit reviewed changes before running; the report must bind an exact source commit.")
    import yaml  # Required by the existing source-intake fixtures.
    contract = json.loads(CONTRACT.read_text())
    gates = contract["gates"]
    require(len(gates) == len({g["id"] for g in gates}) == contract["target_gates"], "Invalid gate inventory")
    metadata = json.loads(read_command("cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"))
    packages = [p for p in metadata["packages"] if p["id"] in metadata["workspace_members"]]
    require(sorted(p["name"] for p in packages) == contract["workspace_members"], "Update the contract for workspace member changes")
    for gate in gates:
        if gate["kind"] == "contract":
            child = json.loads((ROOT / gate["contract"]).read_text())
            count = child.get("target_cases", child.get("target_conditions"))
            conditions = child.get("cases", child.get("conditions", []))
            require(child["version"] == gate["version"] and count == gate["target_conditions"], "Update pinned specialist contract: " + gate["id"])
            require(len(conditions) == len({item["id"] for item in conditions}) == count, "Invalid specialist condition inventory")
    output.mkdir(parents=True, exist_ok=False)
    report_path = output / "report.json"
    before = inputs()
    report = {
        "contract_id": contract["contract_id"], "contract_version": contract["version"],
        "contract_sha256": digest(CONTRACT), "work_item": contract["work_item"],
        "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "source_commit": read_command("git", "rev-parse", "HEAD"),
        "source_tree": read_command("git", "rev-parse", "HEAD^{tree}"), "clean_source": True,
        "input_sha256": before, "input_tree_sha256": tree_hash(before),
        "environment": {"platform": platform.platform(), "machine": platform.machine(),
                        "rustc": read_command("rustc", "--version"), "cargo": read_command("cargo", "--version"),
                        "python": sys.version.split()[0], "pyyaml": yaml.__version__},
        "workspace_inventory": [{"package": p["name"], "targets": [{"name": t["name"], "kind": t["kind"]} for t in p["targets"]]} for p in packages],
        "target_gates": len(gates), "stages": [], "passed": False,
        "evidence_level": contract["evidence_level"], "e4_completed": 0, "metrics_completed": 0,
        "model_calls": 0, "native_client_activated": False, "release_claim": False,
        "counting": contract["counting"], "limitations": contract["limitations"],
    }

    def save():
        report_path.write_text(json.dumps(report, indent=2) + "\n")

    def receipts(path):
        """Verify and inventory transitive child receipts, including basename logs."""
        found = {}

        def visit(value, parent):
            if isinstance(value, list):
                for item in value:
                    visit(item, parent)
            elif isinstance(value, dict):
                for field, hash_field in (("log", "log_sha256"), ("report", "sha256")):
                    if isinstance(value.get(field), str) and value.get(hash_field):
                        name = value[field]
                        candidate = ROOT / name
                        if not candidate.is_file():
                            candidate = parent.parent / name
                        candidate = candidate.resolve(strict=True)
                        candidate.relative_to(output)
                        require(digest(candidate) == value[hash_field], "Nested receipt changed: " + name)
                        relative = str(candidate.relative_to(ROOT))
                        if relative not in found:
                            found[relative] = value[hash_field]
                            if field == "report":
                                visit(json.loads(candidate.read_text()), candidate)
                for item in value.values():
                    if isinstance(item, (dict, list)):
                        visit(item, parent)

        value = json.loads(path.read_text())
        found[str(path.relative_to(ROOT))] = digest(path)
        visit(value, path)
        return value, found

    replacements = {"python": sys.executable, "output": str(output),
                    "awr": str(ROOT / "target/debug/awr"), "mcp": str(ROOT / "target/debug/awr-mcp")}
    save()
    for gate in gates:
        name = gate["id"]
        if gate.get("requires_build") and not report["stages"][0]["passed"]:
            report["stages"].append({"id": name, "passed": False, "skipped": "Build failed; do not exercise stale binaries."})
            save()
            continue
        command = ["rtk", "proxy", *(a.format_map(replacements) for a in gate["command"])]
        log = output / (name + ".log")
        print("Running " + name, flush=True)
        started = time.monotonic()
        with log.open("x") as stream:
            process = subprocess.Popen(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                       start_new_session=True)
            try:
                code = process.wait(timeout=1800)
            except subprocess.TimeoutExpired:
                if os.name == "posix":
                    os.killpg(process.pid, signal.SIGKILL)
                else:
                    process.kill()
                process.wait()
                stream.write("\nRegression stage timed out.\n")
                code = 124
        text = log.read_text(errors="replace")
        row = {"id": name, "command": command, "exit_code": code, "passed": code == 0,
               "elapsed_seconds": round(time.monotonic() - started, 3),
               "log": str(log.relative_to(ROOT)), "log_sha256": digest(log)}
        try:
            kind = gate["kind"]
            if kind == "rust_tests":
                totals = [tuple(map(int, match)) for match in re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored", text)]
                require(totals and sum(t[0] for t in totals) > 0, "No executed Rust tests")
                row.update(test_functions_passed=sum(t[0] for t in totals),
                           test_functions_failed=sum(t[1] for t in totals), ignored=sum(t[2] for t in totals),
                           target_results=len(totals))
                require(row["test_functions_failed"] == 0, "Rust test failure")
                if name == "workspace":
                    require(row["ignored"] == len(contract["ignored_tests"]), "Ignored test count changed")
            elif kind == "ignored_inventory":
                observed = sorted(re.findall(r"^([\w:]+): test$", text, re.MULTILINE))
                row["listed_ignored_tests"] = observed
                require(observed == sorted(item["name"] for item in contract["ignored_tests"]), "Ignored test inventory changed; supply a verified route")
            elif kind in ("contract", "parity", "lifecycle", "examples"):
                child_path = output / (name + ".json")
                if kind == "lifecycle":
                    child_path = output / "lifecycle/summary.json"
                elif kind == "examples":
                    child_path = output / "examples/summary.json"
                child, bound = receipts(child_path)
                row["receipt_sha256"] = bound
                require(child.get("passed") is True, "Child report did not pass")
                if kind == "contract":
                    pinned = json.loads((ROOT / gate["contract"]).read_text())
                    require(child["contract_version"] == gate["version"] and child["contract_sha256"] == digest(ROOT / gate["contract"]), "Specialist contract identity mismatch")
                    observed = child.get("verified_conditions", child.get("covered_cases", []))
                    expected = [case["id"] for case in pinned.get("cases", pinned.get("conditions", []))]
                    require(len(observed) == len(set(observed)) == gate["target_conditions"] and set(observed) == set(expected), "Incomplete specialist conditions")
                    row.update(contract=gate["contract"], contract_version=gate["version"],
                               target_conditions=gate["target_conditions"], verified_conditions=sorted(observed))
                elif kind == "parity":
                    require(child["tests_run"] == gate["expected_tests"] and child["tools_exercised"] == gate["expected_tools"], "Incomplete CLI/MCP parity inventory")
                    row.update(test_functions_passed=child["tests_run"], tools_exercised=child["tools_exercised"])
                elif kind == "lifecycle":
                    require(child["model_invoked"] is False and child["automatic_hooks"] is False and child["business_acceptance"] is False, "Invalid lifecycle evidence tier")
                    require(child["source_unchanged"] and child["claim_settled"] and child["doctor_findings"] == 0, "Lifecycle did not settle cleanly")
                    for receipt in sorted((output / "lifecycle").glob("*.json")):
                        row["receipt_sha256"][str(receipt.relative_to(ROOT))] = digest(receipt)
                else:
                    require([entry["name"] for entry in child["executed_examples"]] == ["open_store", "catalog", "runtime_transaction", "source_revision"], "Example inventory changed")
                    require(all(entry["passed"] for entry in child["executed_examples"]), "Example failed")
                    row["executed_examples"] = [entry["name"] for entry in child["executed_examples"]]
            elif kind == "yaml_intake":
                child = json.loads(text)
                require(child["ok"] and child["source_files_unchanged"] and child["idempotent"] and child["unknown_status_preserved"] and child["changed_source_preserves_identity"] and child["malformed_sources_rejected"] == 3, "Incomplete YAML intake check")
                row["result"] = child
        except (ValueError, KeyError, TypeError, OSError) as error:
            row.update(passed=False, validation_error=str(error))
        report["stages"].append(row)
        save()
        print(name + (" passed" if row["passed"] else " FAILED; inspect " + str(log.relative_to(ROOT))), flush=True)
    after = inputs()
    source_unchanged = before == after and read_command("git", "rev-parse", "HEAD") == report["source_commit"] and not read_command("git", "status", "--porcelain")
    by_id = {row["id"]: row for row in report["stages"]}
    routed = all(by_id[item["covered_by"]]["passed"] for item in contract["ignored_tests"]) and by_id["ignored_inventory"]["passed"]
    report.update(inputs_unchanged=source_unchanged, ignored_tests_routed=routed,
                  ignored_tests=contract["ignored_tests"],
                  passed=source_unchanged and routed and all(row["passed"] for row in report["stages"]),
                  gates_passed=sum(row["passed"] for row in report["stages"]),
                  specialist_conditions_verified=sum(len(row.get("verified_conditions", [])) for row in report["stages"] if row["passed"]),
                  binary_sha256={name: digest(ROOT / "target/debug" / name) for name in ("awr", "awr-mcp") if (ROOT / "target/debug" / name).is_file()},
                  finished_at=datetime.now().astimezone().isoformat(timespec="seconds"))
    save()
    print(json.dumps({key: report[key] for key in ("passed", "source_commit", "gates_passed", "target_gates", "specialist_conditions_verified", "inputs_unchanged", "ignored_tests_routed", "e4_completed")}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
