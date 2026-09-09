#!/usr/bin/env python3
import hashlib
import json
import time
from pathlib import Path


PROJECT = Path("/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/source-change/project")
RECEIPTS = PROJECT / ".work-receipts"
CONTROL = PROJECT.parent / "control/independent-re-review.json"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


verification = json.loads((RECEIPTS / "re-review-verification.json").read_text())
package = json.loads((RECEIPTS / "re-review-package-verification.json").read_text())
status = json.loads((RECEIPTS / "re-review-final-status.json").read_text())
ready = json.loads((RECEIPTS / "re-review-final-ready.json").read_text())
work = json.loads((RECEIPTS / "re-review-final-work-show.json").read_text())
evidence = json.loads((RECEIPTS / "re-review-final-evidence-show.json").read_text())
active = json.loads((RECEIPTS / "re-review-final-active-sessions.json").read_text())
doctor = json.loads((RECEIPTS / "re-review-doctor.json").read_text())
session_end = json.loads((RECEIPTS / "re-review-session-end.json").read_text())
artifact = json.loads((RECEIPTS / "re-review-artifact.json").read_text())

receipt_files = {}
for path in sorted(RECEIPTS.glob("re-review-*")):
    if path.is_file():
        receipt_files[f".work-receipts/{path.name}"] = {
            "sha256": sha(path),
            "bytes": path.stat().st_size,
        }

control = {
    "kind": "independent_re_review_control",
    "recorded_at": int(time.time() * 1000),
    "reviewer": {
        "canonical_task": "/root/restricted_material_client_run",
        "participant_kind": "luna_worker",
        "original_review_session_id": "01M21X23BEFAEAYF6RJ97K3CG3",
        "re_review_session_id": "01M2202ANNA5WFK3Q9DZ015B7A",
        "re_review_claim_id": "01M2203KGQ2881PWDRPWTH2SX1",
        "original_client_task_id": "01a08381-04a8-7871-9372-a3f55c2d8367",
        "independent_from_producer": True,
    },
    "verdict": "REMEDIATION_ACCEPTED_FOR_PLAN_SCOPE",
    "finding_results": {
        "R-01.1": "closed",
        "R-01.2": "closed",
        "R-01.3": "closed",
        "R-01.4": "closed_with_missing-history_condition_retained",
        "R-01.5": "closed",
        "R-02.1": "disclosure_confirmed",
        "R-02.2": "classification_confirmed_and_deviation_permanently_retained",
        "R-02.3": "limited_business_impact_boundary_confirmed",
    },
    "reference_lookup_classification": {
        "allowed_at_time": ["item_46"],
        "scope_deviations_at_time": ["item_61", "item_63", "item_64", "item_66"],
        "retained_record_sha256": "b2f75186aee39d65367c6ce7aca7b710a12710e43a942debdefeb88d917062c5",
        "all_retained_business_keyword_hits_zero": package[
            "reference_lookup_assessment"
        ]["all_business_keyword_hits_zero"],
        "scope_deviation_retroactively_cured": False,
        "business_pollution_claimed": False,
        "e4_credit": False,
    },
    "publication": {
        **package["publication"],
        "all_declared_files_match": package["checks"][
            "all_103_package_files_match"
        ],
        "published_execution_receipt_count": package[
            "published_execution_receipt_count"
        ],
        "published_json_receipt_count": package["published_receipt_json_parse"][
            "json_file_count"
        ],
        "published_json_parse_errors": package["published_receipt_json_parse"][
            "parse_errors"
        ],
        "sealed_execution_counts": package["sealed_execution_counts"],
    },
    "producer_files_before_after": package["producer_artifacts_before_after"],
    "producer_files_unchanged_during_re_review": all(
        item["unchanged_during_re_review"]
        for item in verification["producer_files_post_re_review"].values()
    ),
    "original_review": {
        "path": "deliverables/sg-independent-review.md",
        "sha256": "735fe7cff0bc7357b9db30d2ef79f82fc9822932a546811b4a2403ef98fa7ee7",
        "unchanged": package["immutable_review_records"][
            "deliverables/sg-independent-review.md"
        ]["match"],
    },
    "re_review_artifact": {
        "path": "deliverables/sg-independent-re-review.md",
        "sha256": verification["artifact"]["sha256"],
        "artifact_id": artifact["artifact"]["id"],
    },
    "re_review_evidence": {
        "external_key": "SG-REVIEW-INDEPENDENT-RECHECK-20260909",
        "evidence_id": evidence["evidence"]["id"],
        "verification_path": ".work-receipts/re-review-verification.json",
        "verification_sha256": evidence["evidence"]["sha256"],
        "currency": evidence["currency"],
        "missing_bindings": evidence["missing_bindings"],
        "level": evidence["evidence"]["level"],
    },
    "awr_final": {
        "project_revision": status["project_revision"],
        "sg_review_status": work["work"]["status"],
        "sg_review_revision": work["work"]["revision"],
        "sg_deliver_status": "blocked",
        "sg_deliver_blocker": ready["blocked_sample"][0]["blocker"],
        "sg_deliver_next_action": ready["blocked_sample"][0]["next_action"],
        "session_status": session_end["session"]["status"],
        "active_session_count": len(active["sessions"]),
        "doctor_ok": doctor["ok"],
        "final_delivery_exists": (PROJECT / "deliverables/sg-delivery.md").exists(),
    },
    "preserved_failures": [
        {
            "stage": "initial parallel read launch",
            "result": "process_not_created",
            "cause": "incorrect workdir /Users/mac/Documents/originone/agent/z",
            "state_changed": False,
            "receipt": None,
        },
        {
            "stage": "work reopen attempt 1",
            "result": "RevisionConflict actual=119 expected=3",
            "state_changed": False,
            "receipt": ".work-receipts/re-review-work-reopen-first-attempt-error.json",
        },
        {
            "stage": "work reopen attempt 2",
            "result": "summary field not authorized",
            "state_changed": False,
            "receipt": ".work-receipts/re-review-work-reopen-second-attempt-error.json",
        },
        {
            "stage": "work reopen attempt 3",
            "result": "session not bound to exact target work",
            "state_changed": False,
            "receipt": ".work-receipts/re-review-work-reopen-third-attempt-error.json",
        },
        {
            "stage": "post verification attempt 1",
            "result": "KeyError sources",
            "state_changed": False,
            "receipt": ".work-receipts/re-review-post-verify-first-attempt-error.json",
        },
        {
            "stage": "post verification attempt 2",
            "result": "R-02 item IDs used shorthand instead of four verbatim IDs",
            "state_changed": False,
            "receipt": ".work-receipts/re-review-post-verify-second-attempt-error.json",
        },
        {
            "stage": "event append attempt 1",
            "result": "payload contains unknown field or invalid value",
            "state_changed": False,
            "receipt": ".work-receipts/re-review-event-first-attempt-error.json",
        },
        {
            "stage": "evidence add attempt 1",
            "result": "repo-relative input path not found from AWR project",
            "state_changed": False,
            "receipt": ".work-receipts/re-review-evidence-add-first-attempt-error.json",
        },
        {
            "stage": "work complete attempt 1",
            "result": "project-relative completion input not found from invocation cwd",
            "state_changed": False,
            "receipt": ".work-receipts/re-review-work-complete-first-attempt-error.json",
        },
    ],
    "unresolved": verification["unresolved"],
    "next_action": (
        "Return this report to the original client. The original client may clear "
        "the re-review-wait blocker and create the final delivery while preserving "
        "the date statement, unknown facts and R-02 process deviation."
    ),
    "receipt_files": receipt_files,
    "evidence_boundary": verification["evidence_boundary"],
    "e4_credit": False,
}
CONTROL.write_text(json.dumps(control, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({
    "control": str(CONTROL),
    "verdict": control["verdict"],
    "project_revision": control["awr_final"]["project_revision"],
    "receipt_count": len(receipt_files),
    "report_sha256": control["re_review_artifact"]["sha256"],
}, ensure_ascii=False, indent=2))
