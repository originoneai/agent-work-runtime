#!/usr/bin/env python3
"""Verify all current AWR fields after final delivery and post-delivery alignment."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo


ROOT = Path(__file__).resolve().parents[1]
AWR = "/Users/mac/Documents/originone/agent-work-running/target/release/awr"

EXPECTED_NEXT = {
    "XQ-INPUT": "本工作已完成且最终筹备包已交付，不再重复执行；共同约束仅供 VEN-01、VEN-02、EQP-01、EQP-02 等后续现场确认使用，新增事实以新回执追加。",
    "XQ-CONTENT": "本工作已完成且最终筹备包已交付；仅当日期、讲者、人数、场地或设备新回执触发变化时，由内容执行者或明确接续者补齐 MAT-01、MAT-02 和受影响安排，并追加版本与交接回执。",
    "XQ-VENUE": "本工作已完成且最终筹备包已交付；协调者明确日期、人数和内容材料后，另派现场核验人关闭 VEN-01、VEN-02、EQP-01、EQP-02 与 MAT-02，原执行者无需重复处理。",
    "XQ-MERGE": "本工作与整改均已完成，独立返检和最终筹备交付也已完成；不再重复合并或返检。后续仅由活动协调者依据 deliverables/xq-delivery.md 关闭现场未决项，新事实触发时追加回执。",
    "XQ-REVIEW": "本工作已完成且返检结论已用于最终筹备交付，XQ-DELIVER 已完成；不再重复返检或交付。后续现场事实由交付包所列角色取得，返检报告不替代现场验收。",
    "XQ-DELIVER": "本工作完成后不再重复交付；活动协调者先关闭 VEN-01 与 PPL-01，再按交付包条件推进场地、设备、成品材料、外部交接及真实现场执行并保留回执。",
}

SESSIONS = {
    "XQ-INPUT": "01M22K2H647TA9ARYYW9GAZ1NP",
    "XQ-CONTENT": "01M22K65QQZEFX2X74M2K9F465",
    "XQ-VENUE": "01M22K713C7KXGF3W3J1P05PRE",
    "XQ-MERGE": "01M22K7Y6RD2ZCMDYG4GWM8QVB",
    "XQ-REVIEW": "01M22K8VERPNDZTWKDYWXVSYE5",
    "XQ-DELIVERY": "01M22KM045XEKM6GHR6TMA5WXZ",
}

PROPOSALS = {
    "XQ-INPUT": "01M22K2H8723V0DWRJ4P7RAQE9",
    "XQ-CONTENT": "01M22K65ST8AGMQYKXAD5QDPGB",
    "XQ-VENUE": "01M22K715GY9WHECESCARQW6BK",
    "XQ-MERGE": "01M22K7Y8X7D9G7VW9C9DP2ZEJ",
    "XQ-REVIEW": "01M22K8VGXYXJDQAR5N4Z7807V",
    "XQ-DELIVERY": "01M22KM06ASVXQ0JZ9GWTCFR42",
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def awr(*args: str) -> dict:
    command = ["rtk", "proxy", AWR, "--project", str(ROOT), "--json", *args]
    result = subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True)
    return json.loads(result.stdout)


def main() -> int:
    failures: list[str] = []
    live_work: dict[str, dict] = {}

    for key, expected_next in EXPECTED_NEXT.items():
        work = awr("work", "show", key)["work"]
        live_work[key] = {
            "status": work["status"],
            "active_claims": len(work.get("active_claims", [])),
            "summary": work.get("summary"),
            "next_action": work.get("next_action"),
        }
        if work["status"] != "completed":
            failures.append(f"{key} status is {work['status']}, expected completed")
        if work.get("active_claims"):
            failures.append(f"{key} still has active claims")
        if work.get("next_action") != expected_next:
            failures.append(f"{key} next_action does not match the final-state text")

    session_states: dict[str, str] = {}
    for key, session_id in SESSIONS.items():
        state = awr("session", "show", session_id)["session"]["status"]
        session_states[key] = state
        if state != "ended":
            failures.append(f"{key} alignment session {session_id} is {state}")

    proposal_states: dict[str, str] = {}
    for key, proposal_id in PROPOSALS.items():
        state = awr("proposal", "show", proposal_id)["proposal"]["status"]
        proposal_states[key] = state
        if state != "applied":
            failures.append(f"{key} alignment proposal {proposal_id} is {state}")

    status = awr("status")
    if status.get("counts") != {"completed": 6}:
        failures.append(f"unexpected status counts: {status.get('counts')}")
    if status.get("ready_count") != 0 or status.get("blocked_count") != 0 or status.get("current_total") != 0:
        failures.append("project still has ready, blocked or current source work")

    milestone_hits = awr("search", "XQ-DELIVERY", "--limit", "10").get("hits", [])
    milestone = next(
        (hit for hit in milestone_hits if hit.get("kind") == "plan" and hit.get("external_key") == "XQ-DELIVERY"),
        None,
    )
    milestone_status = milestone.get("status") if milestone else None
    if milestone_status != "completed":
        failures.append(f"XQ-DELIVERY milestone status is {milestone_status}, expected completed")

    rejection_path = ROOT / ".work-receipts/096-proposal-approve-xq-input-final-state-attempt-1.json"
    rejection = json.loads(rejection_path.read_text(encoding="utf-8"))
    if rejection != {
        "code": "InvalidTransition",
        "message": "invalid transition: cannot Approve a Draft proposal",
    }:
        failures.append("the retained Draft-to-Approve rejection receipt is missing or changed")

    result = {
        "version": 1,
        "kind": "xq_final_current_state_verification",
        "verified_at": datetime.now(ZoneInfo("Asia/Taipei")).isoformat(timespec="seconds"),
        "passed": not failures,
        "project_revision": status.get("project_revision"),
        "counts": status.get("counts"),
        "ready_count": status.get("ready_count"),
        "blocked_count": status.get("blocked_count"),
        "current_total": status.get("current_total"),
        "milestone": {
            "external_key": "XQ-DELIVERY",
            "status": milestone_status,
        },
        "live_work": live_work,
        "alignment_sessions": session_states,
        "alignment_proposals": proposal_states,
        "retained_alignment_failure": {
            "locator": ".work-receipts/096-proposal-approve-xq-input-final-state-attempt-1.json",
            "sha256": sha256(rejection_path),
        },
        "delivery": {
            "locator": "deliverables/xq-delivery.md",
            "sha256": sha256(ROOT / "deliverables/xq-delivery.md"),
        },
        "failures": failures,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
