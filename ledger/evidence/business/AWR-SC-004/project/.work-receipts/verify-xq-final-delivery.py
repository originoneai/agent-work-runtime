#!/usr/bin/env python3
"""Verify the Xingqiao final preparation handoff without mutating business state."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo


ROOT = Path(__file__).resolve().parents[1]
AWR = Path("/Users/mac/Documents/originone/agent-work-running/target/release/awr")

EXPECTED_HASHES = {
    "deliverables/xingqiao-content.md": "7fc0ccf93f8f8aa6b8d881c91c6f61dc32e63ed979f6cc120f5ba9086eb253c5",
    "deliverables/xingqiao-venue.md": "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6",
    "deliverables/xingqiao-ownership.md": "d1238aea798cc63e1d190cb4cbae24ba0b415f786501e1221312d83dcfcceafc",
    "deliverables/xingqiao-collaboration-review.md": "21e564a7fa985cb67a842e1e836ed84f9a69db2865c86171e0edef4df891d5c0",
    "deliverables/xingqiao-independent-results.md": "681ebc8aa9988fe7d27d581eb4e76d68dc24dbacce49a59c5df17953311f8699",
    "deliverables/xq-independent-review.md": "c4dd2d34080ba24bfdb3d8408990c55b2152c33241ab259b86e4d034335c9c13",
    "deliverables/xq-independent-recheck.md": "e177be268f07a30c35d8f9d1539ab6248a75fb01a935966ce3045809628d902f",
    ".work-receipts/reviewer-completion-report.json": "701e78e14fd895ab32e4538171cdf515dea691b1263cc50864c9e57a2fce7296",
    ".work-receipts/reviewer-recheck-completion-report.json": "81cb64388692c1f5c94e9404c212b624854f14309eb53434a3016cf861e86405",
}

REQUIRED_PATHS = [
    "GOALS.md",
    "PLAN.md",
    "RULES.md",
    "work-ledger.yaml",
    "materials/agenda.csv",
    "materials/venue.md",
    "materials/absence-note.md",
    "materials/joint-review.md",
    "deliverables/xq-delivery.md",
    "review-inputs/initial",
    "review-inputs/round-1",
    "review-inputs/prelude",
    "review-inputs/prelude-content-update",
    "review-inputs/prelude-receipt-response",
    "review-inputs/parallel-process-evidence",
    "review-inputs/peer-branch-check",
    "review-inputs/venue-continuation",
    "review-inputs/peer-receipt-followthrough",
    "review-inputs/coordination-correction",
    "review-inputs/corrections/review-correction",
    ".work-receipts/xingqiao-content-before-successor-status-update.md",
    ".work-receipts/xq-venue-progress-conflict-r29.json",
    ".work-receipts/018-work-progress-xq-content-handoff-wording-conflict.json",
    ".work-receipts/042-reopen-xq-merge-invalid-summary.json",
    ".work-receipts/048-proposal-create-xq-input-session-conflict.json",
    ".work-receipts/084-return-check-handoff-verification-attempt-1.json",
    ".work-receipts/reviewer-verification-attempt-1.json",
    ".work-receipts/reviewer-recheck-verification-before.json",
    ".work-receipts/reviewer-recheck-completion-report-missing-version.json",
    ".work-receipts/reviewer-recheck-completion-report-missing-work-item.json",
    ".work-receipts/reviewer-recheck-work-complete-attempt-1.json",
    ".work-receipts/reviewer-recheck-work-complete-attempt-2.json",
]

REQUIRED_HEADINGS = [
    "## 二、已经完成的筹备工作",
    "## 三、当前统一执行基线",
    "## 四、现场仍待确认的事项",
    "## 五、后续责任与交接关系",
    "## 六、接手顺序与交接条件",
    "## 七、分工、修订、失败与复核的来龙去脉",
    "## 八、交付清单与完整性基线",
]

UNRESOLVED_IDS = [
    "VEN-01",
    "VEN-02",
    "EQP-01",
    "EQP-02",
    "PPL-01",
    "MAT-01",
    "MAT-02",
    "HDO-01",
    "EXE-01",
]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def awr_work(external_key: str) -> dict:
    command = [
        "rtk",
        "proxy",
        str(AWR),
        "--project",
        str(ROOT),
        "--json",
        "work",
        "show",
        external_key,
    ]
    result = subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True)
    return json.loads(result.stdout)


def check(name: str, passed: bool, details: str, failures: list[str], checks: list[dict]) -> None:
    checks.append({"name": name, "passed": passed, "details": details})
    if not passed:
        failures.append(f"{name}: {details}")


def main() -> int:
    failures: list[str] = []
    checks: list[dict] = []

    missing_paths = [relative for relative in REQUIRED_PATHS if not (ROOT / relative).exists()]
    check(
        "required-current-and-history-paths-present",
        not missing_paths,
        "all required paths are present" if not missing_paths else f"missing: {missing_paths}",
        failures,
        checks,
    )

    actual_hashes: dict[str, str] = {}
    hash_mismatches: list[str] = []
    for relative, expected in EXPECTED_HASHES.items():
        path = ROOT / relative
        if not path.is_file():
            hash_mismatches.append(f"{relative}: missing")
            continue
        actual = sha256(path)
        actual_hashes[relative] = actual
        if actual != expected:
            hash_mismatches.append(f"{relative}: expected {expected}, got {actual}")
    check(
        "reviewed-input-hashes-match",
        not hash_mismatches,
        "all reviewed input hashes match" if not hash_mismatches else "; ".join(hash_mismatches),
        failures,
        checks,
    )

    recheck_path = ROOT / ".work-receipts/reviewer-recheck-completion-report.json"
    recheck = json.loads(recheck_path.read_text(encoding="utf-8"))
    closed_findings = {item.get("id"): item.get("result") for item in recheck.get("closed_findings", [])}
    recheck_ok = (
        recheck.get("verdict") == "passed_for_xq_review_scope"
        and closed_findings == {"XQ-R01": "closed", "XQ-R02": "closed"}
        and recheck.get("verification", {}).get("passed") is True
        and recheck.get("final_delivery_performed") is False
        and recheck.get("whole_scenario_e4_granted") is False
    )
    check(
        "independent-recheck-passed-with-boundaries",
        recheck_ok,
        "recheck passed XQ scope, closed XQ-R01/R02, and did not pre-claim delivery or E4",
        failures,
        checks,
    )

    retained_failures = recheck.get("retained_failure_receipts", {})
    failure_receipt_mismatches: list[str] = []
    for relative, expected in retained_failures.items():
        path = ROOT / relative
        if not path.is_file():
            failure_receipt_mismatches.append(f"{relative}: missing")
            continue
        actual = sha256(path)
        if actual != expected:
            failure_receipt_mismatches.append(f"{relative}: expected {expected}, got {actual}")
    check(
        "recorded-failures-preserved",
        not failure_receipt_mismatches,
        "all failures listed by the independent recheck remain byte-identical"
        if not failure_receipt_mismatches
        else "; ".join(failure_receipt_mismatches),
        failures,
        checks,
    )

    delivery_path = ROOT / "deliverables/xq-delivery.md"
    delivery_text = delivery_path.read_text(encoding="utf-8")
    missing_headings = [heading for heading in REQUIRED_HEADINGS if heading not in delivery_text]
    missing_ids = [item_id for item_id in UNRESOLVED_IDS if f"`{item_id}`" not in delivery_text]
    required_phrases = [
        "passed_for_xq_review_scope",
        "requires_executor_changes",
        "RevisionConflict",
        "ClaimConflict",
        "不表示场地方已确认",
        "不得宣称现场就绪",
        "不得宣称活动执行或整场 E4 完成",
        "当前 Git 来源提交为 `8f4031f40c096691df03099c2382cbb86112be19`",
    ]
    missing_phrases = [phrase for phrase in required_phrases if phrase not in delivery_text]
    document_ok = not missing_headings and not missing_ids and not missing_phrases
    check(
        "final-document-covers-required-handoff",
        document_ok,
        "headings, unresolved IDs, history, responsibility, conditions and evidence boundaries are present"
        if document_ok
        else f"missing headings={missing_headings}, ids={missing_ids}, phrases={missing_phrases}",
        failures,
        checks,
    )

    expected_statuses = {
        "XQ-INPUT": "completed",
        "XQ-CONTENT": "completed",
        "XQ-VENUE": "completed",
        "XQ-MERGE": "completed",
        "XQ-REVIEW": "completed",
    }
    live_work: dict[str, dict] = {}
    status_errors: list[str] = []
    for external_key, expected_status in expected_statuses.items():
        result = awr_work(external_key)
        work = result["work"]
        live_work[external_key] = {
            "status": work["status"],
            "active_claims": len(work.get("active_claims", [])),
            "summary": work.get("summary"),
            "next_action": work.get("next_action"),
        }
        if work["status"] != expected_status or work.get("active_claims"):
            status_errors.append(
                f"{external_key}: status={work['status']}, active_claims={len(work.get('active_claims', []))}"
            )

    deliver_result = awr_work("XQ-DELIVER")
    deliver_work = deliver_result["work"]
    live_work["XQ-DELIVER"] = {
        "status": deliver_work["status"],
        "active_claims": len(deliver_work.get("active_claims", [])),
        "summary": deliver_work.get("summary"),
        "next_action": deliver_work.get("next_action"),
    }
    if deliver_work["status"] not in {"in_progress", "completed"}:
        status_errors.append(f"XQ-DELIVER: unexpected status={deliver_work['status']}")
    if deliver_work["status"] == "in_progress":
        active_sessions = {
            claim.get("session_id") for claim in deliver_work.get("active_claims", [])
        }
        if active_sessions != {"01M22HER372Y2YEY489KN81737"}:
            status_errors.append(f"XQ-DELIVER: unexpected active sessions={sorted(active_sessions)}")
    elif deliver_work.get("active_claims"):
        status_errors.append("XQ-DELIVER: completed work still has an active claim")

    check(
        "awr-current-state-consistent",
        not status_errors,
        "all five prerequisites are completed without claims; XQ-DELIVER is owned in progress or completed"
        if not status_errors
        else "; ".join(status_errors),
        failures,
        checks,
    )

    report = {
        "version": 1,
        "kind": "xq_final_delivery_verification",
        "work_item": "XQ-DELIVER",
        "verified_at": datetime.now(ZoneInfo("Asia/Taipei")).isoformat(timespec="seconds"),
        "passed": not failures,
        "checks": checks,
        "delivery": {
            "path": "deliverables/xq-delivery.md",
            "sha256": sha256(delivery_path),
        },
        "reviewed_input_sha256": actual_hashes,
        "live_work": live_work,
        "remaining_unknown_ids": UNRESOLVED_IDS,
        "failures": failures,
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
