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


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_manifest(relative):
    checked = []
    for line in (ROOT / relative).read_text().splitlines():
        expected, path = line.split("  ", 1)
        actual = digest(ROOT / path)
        if actual != expected:
            raise AssertionError(f"hash mismatch for {path}: {actual} != {expected}")
        checked.append(path)
    return checked


def awr(*args):
    command = ["rtk", "proxy", AWR, *args, "--project", str(ROOT), "--json"]
    result = subprocess.run(command, cwd=ROOT, check=True, text=True, capture_output=True)
    return json.loads(result.stdout)


core_files = verify_manifest(".work-receipts/zs-remediation-recheck-inputs-v2.sha256")
bundle_files = verify_manifest(".work-receipts/zs-remediation-recheck-bundle-v2.sha256")
status = awr("status")
recover = awr("work", "show", "ZS-RECOVER")
review = awr("work", "show", "ZS-REVIEW")
shows = {proposal_id: awr("proposal", "show", proposal_id, "--full") for proposal_id in OLD_IDS + [NEW_ID]}

checks = [
    recover["acceptance"] == EXPECTED_ACCEPTANCE and len(set(recover["acceptance"])) == 4,
    shows[NEW_ID]["proposal"]["status"] == "applied" and shows[NEW_ID]["apply_attempt"] is not None,
    all(shows[proposal_id]["proposal"]["status"] == "rejected" and shows[proposal_id]["apply_attempt"] is None for proposal_id in OLD_IDS),
    "CR-03" in recover["work"]["summary"] and "返检通过后才可推进" in recover["work"]["next_action"],
    recover["work"]["status"] == "completed",
    review["work"]["status"] == "planned" and review["work"]["ready"] is True,
    status["suggested_work"]["external_key"] == "ZS-REVIEW" and status["blocked_count"] == 1,
    digest(ROOT / "deliverables/zs-independent-review.md") == "c580b03404518ac99436ef3f7f3ccfdb78315a7a493b3c53c18448bd685072b5"
    and not (ROOT / "deliverables/zs-delivery.md").exists(),
]
assert all(checks)

print(json.dumps({
    "version": 1,
    "verified_at": int(time.time() * 1000),
    "command": "rtk proxy /Users/mac/Documents/originone/agent-work-running/.venv/bin/python .work-receipts/verify-zs-remediation-recheck-inputs.py",
    "project_revision": status["project_revision"],
    "source_revision": review["source_ref"]["source_revision"],
    "source_fingerprint": review["source_ref"]["source_fingerprint"],
    "core_files_verified": len(core_files),
    "bundle_files_verified": len(bundle_files),
    "producer_checks_passed": len(checks),
    "recover_status": recover["work"]["status"],
    "review_status": review["work"]["status"],
    "review_ready": review["work"]["ready"],
    "deliver_dependency_blocked_count": status["blocked_count"],
    "final_delivery_artifact_absent": True,
    "independent_recheck_performed": False,
}, ensure_ascii=False, indent=2))
