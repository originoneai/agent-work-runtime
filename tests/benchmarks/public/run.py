#!/usr/bin/env python3
"""Compare full-source input with AWR context on every active synthetic task."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import platform
from pathlib import Path
import subprocess
import sys
import time

import tiktoken

from fixture import create, RULES

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "context"))
from oracle import assess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--awr", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="New directory under .local")
    args = parser.parse_args()
    binary = args.awr.resolve(strict=True)
    output = args.output.resolve()
    output.relative_to(ROOT / ".local")
    output.mkdir(parents=True, exist_ok=False)
    project = output / "project"
    works = create(project)
    by_key = {w["id"]: w for w in works}
    active = [w for w in works if w["status"] != "completed"]
    source_paths = sorted(p for p in project.iterdir() if p.suffix in {".md", ".yaml"})
    source_hashes = {p.name: digest(p) for p in source_paths}
    corpus = "\n".join(p.read_text(encoding="utf-8") for p in source_paths)
    encoding = tiktoken.get_encoding("o200k_base")
    count = lambda value: len(encoding.encode_ordinary(value))
    baseline = count(corpus)
    receipts = output / "receipts"
    receipts.mkdir()
    sequence = 0

    def run(*args):
        nonlocal sequence
        sequence += 1
        command = [str(binary), "--project", str(project), "--json", *args]
        started = time.perf_counter_ns()
        result = subprocess.run(command, capture_output=True, text=True)
        elapsed = (time.perf_counter_ns() - started) / 1_000_000
        (receipts / f"{sequence:04d}.stdout").write_text(result.stdout, encoding="utf-8")
        (receipts / f"{sequence:04d}.stderr").write_text(result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(f"Command {sequence} failed: {result.stderr}")
        return json.loads(result.stdout), result.stdout, elapsed

    run("init", "--manifest", str(project / "project.toml"), "--accept")
    cases = []
    for work in active:
        key = work["id"]
        current, _, _ = run("object", "show", "work", key, "--full")
        bootstrap, raw_l0, _ = run("context", "bootstrap", "--work", key, "--budget", "1000")
        compiled, raw_l1, _ = run("context", "compile", "--work", key, "--goal", work["goal"], "--budget", "5000")
        assert bootstrap["context"]["complete"] and compiled["completeness"]["complete"], key
        expected = {"work": work, "rules": [{"key": r["key"], "text": r["key"].title() + "\n\n" + r["text"]} for r in RULES],
                    "unresolved_dependencies": [by_key[k] for k in work["depends_on"] if by_key[k]["status"] != "completed"]}
        facts = assess(expected, current["object"], bootstrap, compiled)
        assert facts["passed"] == facts["assertions"], (key, facts)
        tokens = {}
        for level, pack in [("l0", bootstrap), ("l1", compiled["work_context"])]:
            tokens[level] = count(pack["rendered_context"])
            assert tokens[level] == pack["token_estimate"], (key, level)
        cases.append({"work": key, **tokens, "l0_cli_json": count(raw_l0), "l1_cli_json": count(raw_l1),
                      "fact_assertions": facts["assertions"], "facts_passed": facts["passed"],
                      "unresolved_dependencies": len(expected["unresolved_dependencies"])})

    # Fixed selection, not the task with the fastest timing or largest savings.
    selected = active[0]
    commands = {
        "status": ["status"],
        "work show": ["work", "show", selected["id"]],
        "context compile": ["context", "compile", "--work", selected["id"], "--goal", selected["goal"], "--budget", "5000"],
    }
    timings = {}
    for name, command in commands.items():
        for _ in range(3):
            run(*command)
        samples = []
        for _ in range(30):
            value, _, elapsed = run(*command)
            assert value.get("ok", True)
            if name == "status":
                assert value["total"] == len(works)
            if name == "work show":
                assert value["work"]["external_key"] == selected["id"]
                assert value["work"]["next_action"] == selected["next_action"]
                assert value["acceptance"] == selected["acceptance"]
            if name == "context compile":
                assert value["completeness"]["complete"] and selected["next_action"] in value["work_context"]["rendered_context"]
            samples.append(elapsed)
        timings[name] = {"samples": 30, "p95_ms": round(sorted(samples)[math.ceil(.95 * len(samples))-1], 3)}
        (output / (name.replace(" ", "-") + "-timings.json")).write_text(json.dumps(samples) + "\n")

    assert source_hashes == {p.name: digest(p) for p in source_paths}
    maximum = {name: max(row[name] for row in cases) for name in ("l0", "l1", "l0_cli_json", "l1_cli_json")}
    report = {
        "benchmark": "public-synthetic-context-v1",
        "measured_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "clean_checkout": not bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
        "binary_sha256": digest(binary),
        "benchmark_input_sha256": {str(p.relative_to(ROOT)): digest(p) for p in [HERE/"fixture.py", HERE/"run.py", HERE/"requirements.txt", HERE.parent/"context/oracle.py"]},
        "environment": {"os": platform.platform(), "architecture": platform.machine(), "python": platform.python_version(), "tiktoken": tiktoken.__version__,
                        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                        "cpu": subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip() if platform.system() == "Darwin" else platform.processor()},
        "sample": {"synthetic": True, "work_items": len(works), "active_work_items": len(active), "completed_work_items": len(works)-len(active),
                   "milestones": 6, "goals": 6, "hard_rules": len(RULES), "source_files": source_hashes, "source_bytes": len(corpus.encode("utf-8"))},
        "tokenizer": "o200k_base / encode_ordinary",
        "baseline_tokens": baseline, "maximum_tokens": maximum,
        "minimum_l1_input_reduction_percent": round(100 * (1-maximum["l1"]/baseline), 2),
        "minimum_l1_cli_json_reduction_percent": round(100 * (1-maximum["l1_cli_json"]/baseline), 2),
        "facts": {"passed": sum(r["facts_passed"] for r in cases), "assertions": sum(r["fact_assertions"] for r in cases),
                  "cases_with_unresolved_dependencies": sum(bool(r["unresolved_dependencies"]) for r in cases)},
        "latency": timings, "cases": cases, "model_calls": 0, "source_files_unchanged": True,
        "limitations": [
            "Full-source read is a simple baseline, not an optimized search or another product.",
            "Headline reduction counts only rendered L1 input; full native CLI JSON is reported separately. MCP framing, chat history and model output are excluded.",
            "Synthetic workload, fixed tokenizer and exact source-fact assertions do not measure model quality, billed tokens or real-client task success.",
            "Latency includes fresh native CLI processes and source refresh with warm filesystem/SQLite pages, 3 warmups then 30 sequential samples, nearest-rank p95. No concurrent-agent capacity claim.",
            "AWR generates runtime IDs; reruns can differ slightly in encoded token counts. Every active task is measured and the largest packet is reported.",
        ],
    }
    (output / "summary.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: report[key] for key in ["baseline_tokens", "maximum_tokens", "minimum_l1_input_reduction_percent", "facts", "latency", "clean_checkout"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
