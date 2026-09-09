#!/usr/bin/env python3
"""Independent integrity and business-boundary checks for XQ-REVIEW."""

from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTROL = ROOT.parent / "control"
INPUTS = ROOT / "review-inputs"
PROCESS = INPUTS / "parallel-process-evidence"


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def snapshot_files(base: Path) -> dict[str, str]:
    return {
        path.relative_to(base).as_posix(): sha256(path)
        for path in base.rglob("*")
        if path.is_file()
    }


main_pub = load(CONTROL / "review-input-publication.json")
process_pub = load(CONTROL / "parallel-process-review-publication.json")
main_expected = main_pub["files_sha256"]
process_expected = process_pub["files_sha256"]
main_actual = {
    key: value
    for key, value in snapshot_files(INPUTS).items()
    if not key.startswith("parallel-process-evidence/")
}
process_actual = snapshot_files(PROCESS)


def compare(expected: dict[str, str], actual: dict[str, str]) -> dict:
    common = set(expected) & set(actual)
    return {
        "declared": len(expected),
        "actual": len(actual),
        "missing": sorted(set(expected) - set(actual)),
        "extra": sorted(set(actual) - set(expected)),
        "mismatches": sorted(key for key in common if expected[key] != actual[key]),
    }


main_compare = compare(main_expected, main_actual)
process_compare = compare(process_expected, process_actual)
phases = main_pub["retained_actual_phases"]
turns = {
    phase: load(INPUTS / phase / "turn-record.json")
    for phase in phases
}
peer = load(PROCESS / "peer-initial-sealed.json")
authorship = load(PROCESS / "artifact-authorship-correction.json")
claim_conflict = load(PROCESS / "actual-parallel-conflict-binding.json")
branch_observation = load(PROCESS / "parallel-branch-ownership-observation.json")

current_artifacts = {
    path: sha256(ROOT / path)
    for path in (
        "deliverables/xingqiao-independent-results.md",
        "deliverables/xingqiao-content.md",
        "deliverables/xingqiao-venue.md",
        "deliverables/xingqiao-ownership.md",
        "deliverables/xingqiao-collaboration-review.md",
    )
}
correction_artifacts = turns["coordination-correction"]["artifact_sha256"]

content = (ROOT / "deliverables/xingqiao-content.md").read_text(encoding="utf-8")
venue = (ROOT / "deliverables/xingqiao-venue.md").read_text(encoding="utf-8")
ownership = (ROOT / "deliverables/xingqiao-ownership.md").read_text(encoding="utf-8")
collab = (ROOT / "deliverables/xingqiao-collaboration-review.md").read_text(encoding="utf-8")
merged = (ROOT / "deliverables/xingqiao-independent-results.md").read_text(encoding="utf-8")

author_by_path = {
    row["path"]: row
    for row in authorship["current_actual_authorship"]
}

checks = {
    "main_publication_147_exact": main_compare == {"declared": 147, "actual": 147, "missing": [], "extra": [], "mismatches": []},
    "process_publication_10_exact": process_compare == {"declared": 10, "actual": 10, "missing": [], "extra": [], "mismatches": []},
    "ten_retained_real_phases": len(phases) == 10 and set(turns) == set(phases),
    "main_client_bound": all(turns[p]["client_task_id"] == "01a083c0-2c0b-7fc1-ba7d-0b0721dfeb0f" for p in ("prelude", "prelude-content-update", "prelude-receipt-response", "initial", "round-1", "round-2", "coordination-correction")),
    "peer_client_bound": all(turns[p]["client_task_id"] == "01a083ca-4a86-7b81-93cd-59afa64767e0" for p in ("peer-branch-check", "peer-receipt-followthrough")) and peer["client"]["native_thread_id"] == "01a083ca-4a86-7b81-93cd-59afa64767e0",
    "successor_client_bound": turns["venue-continuation"]["client_task_id"] == "01a08460-5417-75c3-8ee2-7249153e9c67",
    "peer_venue_authorship": author_by_path["deliverables/xingqiao-venue.md"]["actual_client_task_id"] == "01a083ca-4a86-7b81-93cd-59afa64767e0" and author_by_path["deliverables/xingqiao-venue.md"]["producing_turn"] == "peer-initial",
    "main_content_and_merge_authorship": all(author_by_path[path]["actual_client_task_id"] == "01a083c0-2c0b-7fc1-ba7d-0b0721dfeb0f" for path in ("deliverables/xingqiao-content.md", "deliverables/xingqiao-independent-results.md")),
    "successor_ownership_authorship": author_by_path["deliverables/xingqiao-ownership.md"]["actual_client_task_id"] == "01a08460-5417-75c3-8ee2-7249153e9c67",
    "snapshot_membership_not_used_as_authorship": "incorrectly labeled unchanged peer output as initial" in authorship["reason"],
    "current_artifacts_match_correction_snapshot": current_artifacts == correction_artifacts,
    "venue_remained_peer_bytes": current_artifacts["deliverables/xingqiao-venue.md"] == peer["validation"]["venue_artifact_sha256"],
    "earlier_revision_conflict_distinguished": peer["concurrency"]["revision_conflict_observed"] is True and peer["concurrency"]["claim_conflict_observed"] is False and peer["concurrency"]["revision_conflict_details"]["code"] == "RevisionConflict",
    "later_claim_conflict_bound": claim_conflict["source_event_item"]["aggregated_output"].startswith('{"code":"ClaimConflict"') and claim_conflict["source_event_item"]["exit_code"] == 1,
    "different_runtime_branch_observed": any(row["branch_id"] for row in branch_observation["claims"]) and any(row["branch_id"] is None for row in branch_observation["claims"]),
    "initial_peer_branch_gap_preserved": peer["branch_evidence"]["independent_work_branch_observed"] is False,
    "two_natural_followups_bound": turns["round-1"]["actual_user_turn"]["text"] == "其中一位执行者暂时离开，请安排接续，保留尚未解决的问题。" and turns["round-2"]["actual_user_turn"]["text"] == "请分别复核两项工作的结果，并汇总需要共同处理的事项。",
    "later_wording_correction_bound": "稿件状态不一致" in turns["coordination-correction"]["actual_user_turn"]["text"],
    "successor_received_four_handoffs": all(token in ownership for token in ("C→V-01", "C→V-02", "C→V-03", "C→V-04", "已收到")) and "01M22688HDB0K4K691RF9CB8TC" in ownership,
    "successor_scope_stays_bounded": all(token in ownership for token in ("不是实机或场地验收", "时间边界、实机、场地、人员与资源事实均未闭合", "交回协调者")),
    "historical_stale_status_labeled": "第一轮快照与边界（历史）" in collab and "原现场方案" in merged and "历史状态" in merged,
    "time_constraints_consistent": all(token in content and token in venue and token in merged for token in ("85", "90", "10")) and "至少短缺 5 分钟" in venue and "至少短缺 5 分钟" in merged,
    "single_projector_nonoverlap": all("T+10—T+35" in text or "T+10–T+35" in text for text in (content, venue, merged)) and "唯一投影占用" in merged,
    "three_tables_conditional": "三组桌面贯穿全程" in content and "条件性基线" in venue and "条件性流程选择" in merged,
    "unknowns_preserved": all(token in merged for token in ("场地尺寸", "投影实机", "参会人数", "成品材料", "尚未闭合")),
    "no_final_or_e4_claim": "不是实际场地验收、正式独立复核结论或最终交付" in merged,
}

result = {
    "kind": "xq_independent_review_verification",
    "verified_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
    "source_commit": "8f4031f40c096691df03099c2382cbb86112be19",
    "checks": checks,
    "ok": all(checks.values()),
    "publications": {
        "main": {"path": "../control/review-input-publication.json", "sha256": sha256(CONTROL / "review-input-publication.json"), **main_compare},
        "parallel_process": {"path": "../control/parallel-process-review-publication.json", "sha256": sha256(CONTROL / "parallel-process-review-publication.json"), **process_compare},
    },
    "retained_phases": [
        {
            "phase": phase,
            "client_task_id": turns[phase]["client_task_id"],
            "user_timestamp": turns[phase]["actual_user_turn"]["timestamp"],
            "user_text": turns[phase]["actual_user_turn"]["text"],
            "events_sha256": turns[phase]["events_sha256"],
            "native_transcript_sha256": turns[phase]["native_transcript_sha256"],
        }
        for phase in phases
    ],
    "current_artifacts_sha256": current_artifacts,
    "actual_authorship": authorship["current_actual_authorship"],
    "conflicts": {
        "early_revision_conflict": peer["concurrency"]["revision_conflict_details"],
        "late_claim_conflict": {
            "code": "ClaimConflict",
            "receipt": claim_conflict["conflict_receipt"],
            "receipt_sha256": claim_conflict["conflict_receipt_sha256"],
            "native_item_id": claim_conflict["source_event_item"]["id"],
        },
    },
    "boundaries": {
        "producer_files_modified_by_reviewer": False,
        "native_clients_started_by_reviewer": False,
        "final_delivery_performed": False,
        "whole_scenario_e4_granted": False,
    },
}

output = ROOT / ".work-receipts/reviewer-verification.json"
output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"ok": result["ok"], "output": str(output), "checks": checks}, ensure_ascii=False, indent=2))
