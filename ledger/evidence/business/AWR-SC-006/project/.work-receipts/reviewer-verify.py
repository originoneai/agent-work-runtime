from __future__ import annotations

import hashlib
import json
from pathlib import Path

import yaml


BASE = Path(__file__).resolve().parents[1]
RECEIPTS = BASE / ".work-receipts"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(name: str) -> dict:
    return json.loads((RECEIPTS / name).read_text())


def parse_manifest(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        digest, rel = line.split("  ", 1)
        result[rel] = digest
    return result


checks: list[dict] = []


def check(name: str, passed: bool, details: str) -> None:
    checks.append({"name": name, "passed": bool(passed), "details": details})


published = load_json("reviewer-review-inputs-verification.json")
check(
    "published-review-inputs",
    published["passed"] and published["published_file_total"] == 62,
    "56 historical business files and 6 interruption-evidence files match their publication hashes.",
)

before = parse_manifest(RECEIPTS / "reviewer-producer-files-before.sha256")
after = {rel: sha256(BASE / rel) for rel in before}
check(
    "producer-files-unchanged-during-review",
    before == after,
    f"Compared {len(before)} producer-owned material and deliverable files before and after review authoring.",
)

proposals = [load_json(f"reviewer-cr-{number}-current.json") for number in ("01", "02", "03")]
proposal_ok = all(
    item["proposal"]["status"] == "rejected" and item["apply_attempt"] is None
    for item in proposals
)
check(
    "old-proposal-lifecycle",
    proposal_ok and len({item["proposal"]["id"] for item in proposals}) == 3,
    "Three distinct old business proposals are rejected and each has no apply attempt; this proves zero replays, not one application.",
)

ledger = yaml.safe_load((BASE / "work-ledger.yaml").read_text())
work = {item["id"]: item for item in ledger["work_items"]}
archive_acceptance = "资料归档按内容类型分目录；三个目录的具体名称或位置及资料类型对应关系须经资料管理员确认后才可应用。"
role_acceptance = "负责角色按值班与资料分工；具体角色与职责边界须经协调者确认后才可应用。"
review_basis_ok = (
    archive_acceptance not in work["ZS-RECOVER"]["acceptance"]
    and role_acceptance not in work["ZS-RECOVER"]["acceptance"]
    and work["ZS-RECOVER"]["status"] == "completed"
    and "应用" in work["ZS-RECOVER"]["next_action"]
)
check(
    "blocking-findings-reproduced",
    review_basis_ok,
    "Confirmed archive and role changes are absent from authoritative acceptance while completed recovery still carries a pending application action.",
)

old_session = load_json("reviewer-old-session.json")
new_session = load_json("reviewer-recovery-session.json")
check(
    "awr-recovery-session-chain",
    old_session["session"]["status"] == "interrupted"
    and old_session["session"]["last_checkpoint_id"] is None
    and all(claim["status"] == "released" for claim in old_session["claims"])
    and new_session["session"]["status"] == "ended"
    and new_session["session"]["last_checkpoint_id"] is not None,
    "Old session is interrupted with no checkpoint and released claims; successor session ended with a later checkpoint.",
)

process = json.loads(
    (BASE / "review-inputs/interruption-evidence/verified-interruption-binding.json").read_text()
)
normal = json.loads(
    (BASE / "review-inputs/interruption-evidence/prelude-normal-exit-observation.json").read_text()
)
check(
    "native-process-evidence",
    normal["native_returncode"] == 0
    and process["native_returncode"] == -9
    and process["native_process_absent"] is True,
    "The first normal exit is distinct from the later terminated native process.",
)

report = BASE / "deliverables/zs-independent-review.md"
report_text = report.read_text()
check(
    "review-report",
    "requires_executor_changes" in report_text
    and "ZS-R04" in report_text
    and "ZS-R06" in report_text,
    "Independent review records the blocking business-application and authoritative-state findings.",
)
check(
    "final-delivery-boundary",
    not (BASE / "deliverables/zs-delivery.md").exists(),
    "Final delivery remains absent and is not credited by this review.",
)

output = {
    "version": 1,
    "reviewer": "luna_worker",
    "review_session": "01M223JC0DGNZACGCB1GK9FWQR",
    "review_claim": "01M223JC0DMZEZAF591R4QH2BC",
    "artifact": {
        "locator": "deliverables/zs-independent-review.md",
        "sha256": sha256(report),
    },
    "checks": checks,
    "passed": all(item["passed"] for item in checks),
    "business_verdict": "requires_executor_changes",
    "e4_credit": False,
}
(RECEIPTS / "reviewer-verification.json").write_text(
    json.dumps(output, ensure_ascii=False, indent=2) + "\n"
)
if not output["passed"]:
    raise SystemExit(1)
print(json.dumps({"passed": True, "check_count": len(checks), "artifact_sha256": output["artifact"]["sha256"]}))
