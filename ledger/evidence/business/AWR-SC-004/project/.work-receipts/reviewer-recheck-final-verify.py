#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path


PROJECT = Path(__file__).resolve().parents[1]
RUN = PROJECT.parent
CONTROL = RUN / "control"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load(path: Path) -> dict:
    return json.loads(path.read_text())


control = load(CONTROL / "independent-recheck.json")
verification = load(PROJECT / ".work-receipts/reviewer-recheck-verification.json")
completion_report = load(
    PROJECT / ".work-receipts/reviewer-recheck-completion-report.json"
)
completion_input = load(
    PROJECT / ".work-receipts/reviewer-recheck-completion-input.json"
)
status = load(PROJECT / ".work-receipts/reviewer-recheck-final-status.json")
ready = load(PROJECT / ".work-receipts/reviewer-recheck-final-ready.json")
review = load(PROJECT / ".work-receipts/reviewer-recheck-final-work-review.json")
deliver = load(PROJECT / ".work-receipts/reviewer-recheck-final-work-deliver.json")
session = load(PROJECT / ".work-receipts/reviewer-recheck-final-session.json")
doctor = load(PROJECT / ".work-receipts/reviewer-recheck-final-doctor.json")

business_hashes = {
    relative: sha256(PROJECT / relative)
    for relative in control["business_files_sha256"]
}
review_output_hashes = {
    relative: sha256(PROJECT / relative)
    for relative in control["review_outputs_sha256"]
}
retained_failure_hashes = {}
for item in control["retained_failures"]:
    relative = item["path"]
    path = RUN / relative if relative.startswith(("project/", "control/")) else PROJECT / relative
    retained_failure_hashes[relative] = sha256(path)

review_evidence_keys = {
    item["external_key"] for item in review.get("evidence", [])
}
checks = {
    "fixed_input_verification_passed": verification["passed"] is True,
    "control_business_hashes_current": business_hashes
    == control["business_files_sha256"],
    "control_review_output_hashes_current": review_output_hashes
    == control["review_outputs_sha256"],
    "retained_failure_hashes_current": all(
        retained_failure_hashes[item["path"]] == item["sha256"]
        for item in control["retained_failures"]
    ),
    "work_ledger_final_hash_current": sha256(PROJECT / "work-ledger.yaml")
    == control["source"]["work_ledger_final_sha256"],
    "completion_report_schema_complete": all(
        key in completion_report
        for key in (
            "version",
            "work_item",
            "source_sha",
            "command",
            "scope",
            "verified_at",
            "artifact",
            "verification_record",
            "checks",
        )
    ),
    "completion_report_is_v1_for_review": completion_report["version"] == 1
    and completion_report["work_item"] == "XQ-REVIEW",
    "completion_mapping_uses_v3": all(
        entry["evidence"] == ["XQ-INDEPENDENT-RECHECK-20260909-V3"]
        for entry in completion_input["acceptance"]
    )
    and completion_input["required_evidence"]
    == ["XQ-INDEPENDENT-RECHECK-20260909-V3"],
    "final_project_revision_311": status["project_revision"] == 311,
    "final_counts_five_completed_one_planned": status["counts"]
    == {"completed": 5, "planned": 1},
    "no_current_work": status["current"] == [],
    "review_completed": review["work"]["status"] == "completed",
    "review_has_no_active_claim": review["work"]["active_claims"] == [],
    "review_v3_evidence_present": "XQ-INDEPENDENT-RECHECK-20260909-V3"
    in review_evidence_keys,
    "review_summary_is_terminal": "独立返检已完成" in review["work"]["summary"]
    and "等待证据" not in review["work"]["summary"],
    "deliver_is_planned_and_unclaimed": deliver["work"]["status"] == "planned"
    and deliver["work"]["active_claims"] == [],
    "deliver_is_graph_ready": ready["ready_total"] == 1
    and ready["ready"][0]["external_key"] == "XQ-DELIVER",
    "final_delivery_artifact_absent": not (
        PROJECT / "deliverables/xq-delivery.md"
    ).exists(),
    "review_session_ended": session["session"]["status"] == "ended",
    "review_checkpoint_bound": session["session"]["last_checkpoint_id"]
    == "01M22G6AZ11CSDHDXQ1EF3GQPZ",
    "review_claim_released": all(
        claim["status"] == "released" for claim in session["claims"]
    ),
    "doctor_clean": doctor["ok"] is True
    and doctor["database_ok"] is True
    and doctor.get("schema_issues", []) == []
    and doctor.get("findings", []) == [],
    "no_final_or_e4_claim": control["boundaries"]["final_delivery_performed"]
    is False
    and control["boundaries"]["whole_scenario_e4_granted"] is False,
}

result = {
    "version": 1,
    "kind": "independent_recheck_final_verification",
    "verdict": control["verdict"],
    "checks": checks,
    "passed": all(checks.values()),
    "control_sha256": sha256(CONTROL / "independent-recheck.json"),
    "report_sha256": sha256(PROJECT / "deliverables/xq-independent-recheck.md"),
    "completion_report_sha256": sha256(
        PROJECT / ".work-receipts/reviewer-recheck-completion-report.json"
    ),
    "work_ledger_sha256": sha256(PROJECT / "work-ledger.yaml"),
    "business_hashes": business_hashes,
    "review_output_hashes": review_output_hashes,
    "retained_failure_hashes": retained_failure_hashes,
    "awr": {
        "project_revision": status["project_revision"],
        "review_status": review["work"]["status"],
        "review_source_revision": review["work"]["source_revision"],
        "deliver_status": deliver["work"]["status"],
        "deliver_ready": ready["ready"][0]["ready"],
        "session_status": session["session"]["status"],
        "claim_statuses": [claim["status"] for claim in session["claims"]],
        "checkpoint_id": session["session"]["last_checkpoint_id"],
    },
}
print(json.dumps(result, ensure_ascii=False, indent=2))
