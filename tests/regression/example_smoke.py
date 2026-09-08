#!/usr/bin/env python3
"""Execute the self-checking developer entrypoints in separate new fixtures."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ("open_store", "catalog", "runtime_transaction", "source_revision")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    output = parser.parse_args().output.resolve()
    output.relative_to(ROOT / ".local")
    output.mkdir(parents=True, exist_ok=False)
    report = {"passed": False, "executed_examples": [], "model_calls": 0, "e4_completed": 0}
    for name in EXAMPLES:
        fixture = output / name
        fixture.mkdir()
        binary = ROOT / "target/debug/examples" / name
        argument = fixture / "state.db" if name == "open_store" else fixture
        command = ["rtk", "proxy", str(binary), str(argument)]
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=60)
        log = output / (name + ".log")
        log.write_text(result.stdout + result.stderr)
        detail = json.loads(result.stdout) if result.returncode == 0 else None
        passed = result.returncode == 0 and isinstance(detail, dict)
        if passed and name == "open_store":
            passed = detail.get("ok") is True and not detail.get("schema_issues")
        if passed and name == "catalog":
            passed = detail["source_registered_revision"] == detail["idempotent_revision"]
        if passed and name == "runtime_transaction":
            passed = detail["committed_revision"] == detail["revision_after_failed_write"] and detail["events_since_start"] == 1
        if passed and name == "source_revision":
            passed = detail.get("ok") is True and detail["successful_projections"] == 4
        report["executed_examples"].append({
            "name": name, "passed": passed, "exit_code": result.returncode, "command": command,
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "log": str(log.relative_to(ROOT)), "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest(),
            "result": detail,
        })
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    report["passed"] = all(row["passed"] for row in report["executed_examples"])
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "examples": len(EXAMPLES)}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
