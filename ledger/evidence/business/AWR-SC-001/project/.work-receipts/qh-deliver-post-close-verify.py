#!/usr/bin/env python3
import hashlib
import json
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE_SHA = "cd44a161471cee94fd4f797955327a4fd995cf3a"


def load(relative):
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def sha256(relative):
    return hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()


checks = []


def check(name, passed, details):
    checks.append({"name": name, "passed": bool(passed), "details": details})


status = load(".work-receipts/20260909-QH-DELIVER-final-status.json")
ready = load(".work-receipts/20260909-QH-DELIVER-final-ready.json")
work = load(".work-receipts/20260909-QH-DELIVER-final-work-show.json")
session = load(".work-receipts/20260909-QH-DELIVER-final-session-show.json")
complete = load(".work-receipts/20260909-QH-DELIVER-complete.json")
checkpoint = load(".work-receipts/20260909-QH-DELIVER-checkpoint.json")
artifact = load(".work-receipts/20260909-QH-DELIVER-artifact-add.json")
evidence_v1 = load(".work-receipts/20260909-QH-DELIVER-evidence-report.json")
evidence_v2 = load(".work-receipts/20260909-QH-DELIVER-evidence-report-v2.json")
evidence_add_v2 = load(".work-receipts/20260909-QH-DELIVER-evidence-add-v2.json")
failed_verify = load(".work-receipts/20260909-QH-DELIVER-evidence-report-attempt-1.json")
failed_complete = load(".work-receipts/20260909-QH-DELIVER-complete-attempt-1.json")

check(
    "final AWR project shape",
    status.get("project_revision") == 206
    and status.get("counts") == {"completed": 4}
    and status.get("current_total") == 0
    and ready.get("ready_total") == 0,
    f"revision={status.get('project_revision')}; counts={status.get('counts')}; current={status.get('current_total')}; ready={ready.get('ready_total')}",
)
check(
    "QH-DELIVER source work completed with safe handoff wording",
    work.get("work", {}).get("status") == "completed"
    and work.get("work", {}).get("source_revision") == 22
    and "仍未取得并明确移交" in work.get("work", {}).get("summary", "")
    and "目录级复核" in work.get("work", {}).get("next_action", ""),
    f"status={work.get('work', {}).get('status')}; revision={work.get('work', {}).get('revision')}; source_revision={work.get('work', {}).get('source_revision')}",
)

completion = complete.get("event", {}).get("payload", {}).get("work_action", {}).get("completion", {})
completion_criteria = {item.get("criterion"): item.get("evidence") for item in completion.get("acceptance", [])}
accepted_evidence = completion.get("evidence", [])
check(
    "completion maps both exact acceptance criteria to V2 evidence",
    set(completion_criteria) == {
        "处理复核意见，交付最终简报与追溯记录。",
        "保留实际产物与来源引用，无法确认的内容显式说明。",
    }
    and len(accepted_evidence) == 1
    and accepted_evidence[0].get("external_key") == "QH-FINAL-HANDOFF-20260909-V2",
    f"criteria={sorted(completion_criteria)}; evidence={[item.get('external_key') for item in accepted_evidence]}",
)
check(
    "accepted V2 evidence is current and locally verified",
    evidence_add_v2.get("evidence", {}).get("id") == "01M221V4GQ583N8JBM5TFVHDZK"
    and evidence_add_v2.get("evidence", {}).get("level") == "locally_verified"
    and evidence_v2.get("all_passed") is True
    and len([item for item in evidence_v2.get("checks", []) if item.get("criteria")]) == 2,
    f"report_sha256={sha256('.work-receipts/20260909-QH-DELIVER-evidence-report-v2.json')}; checks={len(evidence_v2.get('checks', []))}",
)

evidence_index = {item.get("external_key"): item for item in work.get("evidence", [])}
check(
    "explicit evidence and locator-only Unknown remain distinct",
    evidence_index.get("QH-FINAL-HANDOFF-20260909-V2", {}).get("currency") == "current"
    and evidence_index.get("QH-FINAL-HANDOFF-20260909-V2", {}).get("level") == "locally_verified"
    and evidence_index.get(
        "QH-DELIVER/evidence/.work-receipts/20260909-QH-DELIVER-evidence-report-v2.json",
        {},
    ).get("level") == "unknown",
    f"keys={sorted(evidence_index)}",
)

check(
    "session ended, claim released and checkpoint retained",
    session.get("session", {}).get("status") == "ended"
    and all(item.get("status") == "released" for item in session.get("claims", []))
    and session.get("checkpoint", {}).get("id") == "01M221Y8TE6A7RDTHB38PXJYV9"
    and checkpoint.get("checkpoint", {}).get("id") == "01M221Y8TE6A7RDTHB38PXJYV9",
    f"session={session.get('session', {}).get('status')}; claims={[item.get('status') for item in session.get('claims', [])]}; checkpoint={session.get('checkpoint', {}).get('id')}",
)

expected_hashes = {
    "deliverables/qh-delivery.md": "2cbfd52859b4c5ca34cd803b18980aa2d0ec0505fe0403d9ed935ef4bd4d2fce",
    "deliverables/qinghe-brief.md": "cbfa8a49ab2b1f2ad071257dc85f6eac15e9d8783f7f379b6bf411f1a4e50576",
    "deliverables/qinghe-dependencies.md": "39be620608391f4e05a1161766cd586f4971eb10efa28318b3aa80fcccd56167",
    "deliverables/qinghe-context-reference.md": "60ab471924b12cbe1fd129b251c6e004c6b3fb0f2a5672a9bfcddffd3a2d3f17",
    "deliverables/qh-independent-review.md": "cfd81dd8bbc85608a1d6210d9f482dd300b71af8687fb3efc3538b3a8ac19de1",
    "deliverables/qh-review-response.md": "92bc899572eab62027c30ec143b75fd9c9c8971569888493a14e17e31d43398d",
    "deliverables/qh-independent-re-review.md": "adf4fc58abfc863a660913a912f5d57262bb4d6b0182cfdbe8ec16e0a5984d8d",
    "review-inputs/reference-lookup-process-record.json": "0aa782e30d7ac1f2f2b9ba65f9c2224a07404d3111ea5353f1ff0f3015722920",
}
actual_hashes = {path: sha256(path) for path in expected_hashes}
check(
    "final and historical artifact hashes remain stable",
    actual_hashes == expected_hashes,
    f"mismatches={{{', '.join(f'{path}: {actual_hashes[path]}' for path in expected_hashes if actual_hashes[path] != expected_hashes[path])}}}",
)
check(
    "managed artifact matches final delivery",
    artifact.get("artifact", {}).get("sha256") == expected_hashes["deliverables/qh-delivery.md"],
    f"artifact_id={artifact.get('artifact', {}).get('id')}; sha256={artifact.get('artifact', {}).get('sha256')}",
)

check(
    "failed verification and completion attempts retained",
    failed_verify.get("all_passed") is False
    and any(not item.get("passed") for item in failed_verify.get("checks", []))
    and failed_complete.get("code") == "EvidenceMissing",
    f"verification_verdict={failed_verify.get('verdict')}; completion_code={failed_complete.get('code')}",
)
check(
    "successful evidence follows failed attempts without overwriting them",
    evidence_v1.get("all_passed") is True
    and evidence_v2.get("all_passed") is True
    and sha256(".work-receipts/20260909-QH-DELIVER-evidence-report-attempt-1.json")
    == "50cfa2e5e2939d62b9c67d219cea4df673ca5208003e0456299076709e152858",
    f"v1_checks={len(evidence_v1.get('checks', []))}; v2_checks={len(evidence_v2.get('checks', []))}",
)

git_sha = subprocess.run(
    ["git", "rev-parse", "HEAD"], cwd=ROOT, check=True, capture_output=True, text=True
).stdout.strip()
check("Git source SHA remains bound", git_sha == SOURCE_SHA, f"actual={git_sha}")
diff_check = subprocess.run(
    ["git", "diff", "--check"], cwd=ROOT, capture_output=True, text=True
)
check(
    "git diff whitespace check",
    diff_check.returncode == 0,
    diff_check.stdout.strip() or diff_check.stderr.strip() or "clean",
)

all_passed = all(item["passed"] for item in checks)
report = {
    "version": 1,
    "work_item": "QH-DELIVER",
    "source_sha": SOURCE_SHA,
    "verified_at": int(time.time() * 1000),
    "all_passed": all_passed,
    "verdict": "handoff_work_closed_with_external_items_explicitly_pending"
    if all_passed
    else "post_close_verification_failed",
    "checks": checks,
    "final_project_revision": status.get("project_revision"),
    "final_source_revision": work.get("work", {}).get("source_revision"),
    "final_work_ledger_sha256": sha256("work-ledger.yaml"),
    "final_artifacts_sha256": actual_hashes,
    "evidence_report_v1_sha256": sha256(
        ".work-receipts/20260909-QH-DELIVER-evidence-report.json"
    ),
    "evidence_report_v2_sha256": sha256(
        ".work-receipts/20260909-QH-DELIVER-evidence-report-v2.json"
    ),
    "bound_completion_evidence": "QH-FINAL-HANDOFF-20260909-V2",
    "limitations": [
        "All four AWR source work items are completed, but that state describes this defined handoff work set only.",
        "The result is not evidence that documents, titles, directory review, domain, retirement, cutover, external access or portal launch are complete.",
        "The accepted QH-DELIVER evidence is executor-produced locally_verified evidence, not another independent review.",
        "The independent re-review covers arrangement-level remediation only and explicitly excludes the final handoff file and external readiness.",
    ],
}
print(json.dumps(report, ensure_ascii=False, indent=2))
sys.exit(0 if all_passed else 1)
