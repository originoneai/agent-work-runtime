#!/usr/bin/env python3
"""Run the manual AWR lifecycle on a fresh copy of examples/basic.

This calls the AWR CLI, not Codex or a model. It leaves all receipts and the
runtime database in the requested new output directory for inspection.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--awr", required=True, help="Path to the built awr executable")
    parser.add_argument("--output", type=Path, required=True, help="New directory; never overwritten")
    parser.add_argument("--model", default="example-metadata", help="Recorded metadata only; no model invocation")
    args = parser.parse_args()
    executable = Path(args.awr).expanduser().resolve(strict=True)
    output = args.output.expanduser().absolute()
    output.mkdir(parents=True, exist_ok=False)
    project = output / "project"
    project.mkdir()
    fixture = Path(__file__).resolve().parents[1] / "basic"
    source_names = ["project.toml", "GOALS.md", "PLAN.md", "RULES.md", "work-ledger.yaml"]
    for name in source_names:
        source = Path(__file__).with_name(name) if name == "work-ledger.yaml" else fixture / name
        shutil.copyfile(source, project / name)
    before = {name: hashlib.sha256((project / name).read_bytes()).hexdigest() for name in source_names}

    def run(label, *arguments):
        command = [str(executable), "--project", str(project), "--json", *map(str, arguments)]
        result = subprocess.run(command, text=True, capture_output=True, check=False)
        receipt = {"command": command, "exit_code": result.returncode,
                   "stdout": result.stdout, "stderr": result.stderr}
        (output / (label + ".json")).write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        require(result.returncode == 0, f"{label} failed; inspect {output / (label + '.json')}. No retry was made.")
        return json.loads(result.stdout)

    run("01-preview", "init", "--manifest", "project.toml")
    run("02-init", "init", "--manifest", "project.toml", "--accept")
    status = run("03-status", "status")
    started = run("04-start", "session", "start", "--work", "EXAMPLE-001",
                  "--agent", "codex-example", "--provider", "openai", "--model", args.model,
                  "--claim", "--ttl-ms", 3600000, "--expected-revision", status["project_revision"])
    session = started["session"]["id"]
    bootstrap = run("05-bootstrap", "context", "bootstrap", "--session", session, "--budget", 1000)
    require(bootstrap["context"]["complete"], "Bootstrap is incomplete; inspect its gaps")
    require(not bootstrap["context"]["execution_context_complete"], "L0 must remain orientation only")
    context = run("06-context", "context", "compile", "--session", session, "--budget", 5000)
    require(context["completeness"]["complete"] and context["work_context"], "L1 is incomplete")

    # A real agent reads the context and does the work before saving its own digest.
    # This example checkpoints only the lifecycle operations it actually performed.
    status = run("07-status", "status")
    next_action = "Inspect the recovered context before continuing the source-intake work."
    open_loop = "The source-intake acceptance work and its independent review remain unfinished."
    checkpoint = run("08-checkpoint", "session", "checkpoint", "--session", session,
                     "--context-hash", context["work_context"]["context_hash"],
                     "--digest", "Initialized the copied fixture, started a claimed session, and read L0/L1.",
                     "--next-action", next_action, "--open-loop", open_loop,
                     "--expected-revision", status["project_revision"])
    resumed = run("09-resume", "session", "resume", "--from-session", session,
                  "--agent", "codex-example-resumed", "--provider", "openai", "--model", args.model,
                  "--budget", 5000, "--expected-revision", checkpoint["project_revision"])
    require(resumed["context_ready"], "Inspect resume receipts before attempting another session")
    successor = resumed["resumed"]["session"]["id"]
    require(successor != session, "Resume must create a distinct AWR session")
    require(resumed["checkpoint_id"] == checkpoint["checkpoint"]["id"], "Checkpoint inheritance mismatch")
    require(resumed["resumed"]["claim"]["expires_at"] == started["claim"]["expires_at"],
            "Default resume must not silently extend a claim")
    recovered = run("10-recovered-context", "context", "compile", "--session", successor, "--budget", 5000)
    require(recovered["completeness"]["complete"], "Recovered L1 is incomplete")
    recovered_text = json.dumps(recovered)
    require(next_action in recovered_text and open_loop in recovered_text, "Recovery lost the next action or open loop")
    run("11-predecessor", "session", "show", session)
    status = run("12-status", "status")
    ended = run("13-end", "session", "end", "--session", successor, "--outcome", "incomplete",
                "--expected-revision", status["project_revision"])
    final = run("14-successor", "session", "show", successor)
    require(final["session"]["status"] == "incomplete", "Example session did not close")
    require(not any(c["status"] == "active" for c in final["claims"]), "Example left an active claim")
    after = {name: hashlib.sha256((project / name).read_bytes()).hexdigest() for name in source_names}
    require(before == after, "Lifecycle unexpectedly changed the copied source files")
    doctor = run("15-doctor", "doctor")
    require(doctor["ok"] and not doctor["findings"], "Inspect the example's Doctor findings")
    summary = {
        "kind": "cli_lifecycle_example", "passed": True, "model_invoked": False,
        "automatic_hooks": False, "business_acceptance": False,
        "session_id": session, "checkpoint_id": checkpoint["checkpoint"]["id"],
        "successor_id": successor, "final_project_revision": ended["project_revision"],
        "bootstrap_tokens": bootstrap["token_estimate"],
        "context_tokens": context["work_context"]["token_estimate"],
        "recovered_context_tokens": recovered["work_context"]["token_estimate"],
        "source_unchanged": True, "claim_settled": True, "doctor_findings": 0,
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(output), **summary}, indent=2))


if __name__ == "__main__":
    main()
