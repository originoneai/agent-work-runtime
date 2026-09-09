#!/usr/bin/env python3
import hashlib
import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AWR = "/Users/mac/Documents/originone/agent-work-running/target/release/awr"
OLD_IDS = [
    "01M21X4H7GSP7DY19MTHYDBXST",
    "01M21X5MHW7RFN8PBE4J86GACM",
    "01M21X67QFPQ55YSMJEX96HYV7",
]
NEW_ID = "01M225AVE1BFYY0W4T2EKRV88C"
EXPECTED_ACCEPTANCE = [
    "处理待应用改动，保留重试或恢复的证据，避免重复写入。",
    "保留实际产物与来源引用，无法确认的内容显式说明。",
    "资料归档按“使用说明、交接记录、问题跟踪”三个内容类型目录执行，并保留资料管理员确认来源（materials/archive-confirmation.md）；目录根路径、权限、迁移与完整性未明确时继续列为未决。",
    "角色职责按协调者确认执行：值班人员维护交接记录、资料管理员维护使用说明、问题负责人维护问题跟踪，并保留确认来源（materials/role-confirmation.md）；具体人员、值班表、代理/升级路径和交接时限未明确时继续列为未决。",
]
EXPECTED_HASHES = {
    "materials/handover-draft.md": "4cbac09bd7a59e47e245a2d5f76e6c18a694680be26c411f97c503972df62361",
    "materials/change-requests.csv": "0872b48bc80817d733fd80b799d50e850406949f805db81b83aaa54eba433ee2",
    "materials/archive-confirmation.md": "03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c",
    "materials/role-confirmation.md": "8117dffe9cf6088adf357e9d0c404d1f4f547c430dd520cf1095d8cb27bf1da5",
    "deliverables/zs-independent-review.md": "c580b03404518ac99436ef3f7f3ccfdb78315a7a493b3c53c18448bd685072b5",
}


def run_awr(*args):
    command = ["rtk", "proxy", AWR, *args, "--project", str(ROOT), "--json"]
    result = subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True)
    return json.loads(result.stdout)


def sha256(relative):
    return hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()


def check(name, passed, details, criteria):
    if not passed:
        raise AssertionError(f"{name}: {details}")
    return {"name": name, "passed": True, "details": details, "criteria": criteria}


work = run_awr("work", "show", "ZS-RECOVER")
proposals = run_awr("proposal", "list")
shows = {proposal_id: run_awr("proposal", "show", proposal_id, "--full") for proposal_id in OLD_IDS + [NEW_ID]}
inventory_ids = [item["id"] for item in proposals["proposals"]]

checks = []
checks.append(check(
    "confirmed-acceptance-effective-once",
    work["acceptance"] == EXPECTED_ACCEPTANCE and len(set(work["acceptance"])) == 4,
    "Current ZS-RECOVER acceptance is the exact four-entry set: two original requirements plus the confirmed archive and role requirements, with no duplicate entry.",
    EXPECTED_ACCEPTANCE,
))
checks.append(check(
    "new-combined-proposal-applied-once",
    shows[NEW_ID]["proposal"]["status"] == "applied"
    and shows[NEW_ID]["apply_attempt"] is not None
    and shows[NEW_ID]["apply_attempt"]["proposal_id"] == NEW_ID
    and inventory_ids.count(NEW_ID) == 1,
    "The new combined acceptance proposal exists once, is applied, and has one resolved apply_attempt bound to itself.",
    [EXPECTED_ACCEPTANCE[2], EXPECTED_ACCEPTANCE[3]],
))
checks.append(check(
    "old-proposals-not-replayed",
    all(
        shows[proposal_id]["proposal"]["status"] == "rejected"
        and shows[proposal_id]["apply_attempt"] is None
        and inventory_ids.count(proposal_id) == 1
        for proposal_id in OLD_IDS
    ),
    "Each old proposal remains a unique rejected inventory record with apply_attempt null; none was replayed.",
    [EXPECTED_ACCEPTANCE[0]],
))
checks.append(check(
    "cr03-and-unknowns-retained",
    all("遗留问题保留已知影响和下一步" not in item for item in work["acceptance"])
    and "CR-03" in work["work"]["summary"]
    and "返检通过前不推进最终交付" in work["work"]["next_action"],
    "CR-03 was not promoted without a concrete issue list; the current summary and next action retain it and block final delivery before recheck.",
    [EXPECTED_ACCEPTANCE[1]],
))
checks.append(check(
    "source-materials-and-review-unchanged",
    all(sha256(path) == expected for path, expected in EXPECTED_HASHES.items()),
    "Original materials, confirmation sources and the independent review report match their fixed SHA-256 values.",
    [EXPECTED_ACCEPTANCE[1], EXPECTED_ACCEPTANCE[2], EXPECTED_ACCEPTANCE[3]],
))
checks.append(check(
    "failure-history-retained",
    all((ROOT / path).is_file() for path in [
        ".work-receipts/cr-01-create-session-mismatch-error.json",
        ".work-receipts/reviewer-tool-failures.json",
        ".work-receipts/zs-recovery-final-doctor.log",
        ".work-receipts/zs-remediation-tool-failures.json",
    ]),
    "The original proposal failure, reviewer tool failure, historically misnamed nonzero Doctor log and remediation tool failures all remain on disk.",
    [EXPECTED_ACCEPTANCE[0], EXPECTED_ACCEPTANCE[1]],
))
checks.append(check(
    "producer-record-distinguishes-history",
    "旧建议未重放（旧 patch 应用 0 次）" in (ROOT / "deliverables/zhusheng-change-proposals.md").read_text()
    and "新修订已生效（新合并提案应用 1 次）" in (ROOT / "deliverables/zhusheng-change-proposals.md").read_text()
    and "独立复核后整改" in (ROOT / "deliverables/zhusheng-recovery-work.md").read_text(),
    "Producer records explicitly distinguish zero old-patch applications from the single new combined application and retain the remediation chronology.",
    [EXPECTED_ACCEPTANCE[0], EXPECTED_ACCEPTANCE[1]],
))
checks.append(check(
    "lifecycle-ready-for-recheck",
    work["work"]["status"] in {"in_progress", "completed"}
    and not (ROOT / "deliverables/zs-delivery.md").exists(),
    "ZS-RECOVER is in a valid remediation lifecycle state and no final delivery artifact has been prefilled before independent recheck.",
    [EXPECTED_ACCEPTANCE[0], EXPECTED_ACCEPTANCE[1]],
))

report = {
    "version": 1,
    "work_item": "ZS-RECOVER",
    "source_sha": "8218b29c348b7eda3e57b541c3489e41b87b6679",
    "command": "rtk proxy /Users/mac/Documents/originone/agent-work-running/.venv/bin/python .work-receipts/verify-zs-remediation.py",
    "scope": ["ZS-RECOVER"],
    "verified_at": int(time.time() * 1000),
    "project_revision": work["project_revision"],
    "source_revision": work["source_ref"]["source_revision"],
    "source_fingerprint": work["source_ref"]["source_fingerprint"],
    "combined_proposal_id": NEW_ID,
    "combined_apply_event_id": shows[NEW_ID]["apply_attempt"]["resolved_event_id"],
    "checks": checks,
}
print(json.dumps(report, ensure_ascii=False, indent=2))
