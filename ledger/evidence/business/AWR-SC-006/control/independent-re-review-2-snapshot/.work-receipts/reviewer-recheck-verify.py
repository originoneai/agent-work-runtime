#!/usr/bin/env python3
import hashlib
import json
from datetime import datetime
from pathlib import Path

import yaml


PROJECT = Path(__file__).resolve().parents[1]
CONTROL = PROJECT.parent / "control"
RECEIPTS = PROJECT / ".work-receipts"
CORRECTION = PROJECT / "review-inputs/corrections/review-correction"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: Path):
    return json.loads(path.read_text())


checks = []


def check(name, actual, expected, evidence=None):
    ok = actual == expected
    entry = {"name": name, "passed": ok, "actual": actual, "expected": expected}
    if evidence is not None:
        entry["evidence"] = evidence
    checks.append(entry)
    if not ok:
        raise AssertionError(f"{name}: {actual!r} != {expected!r}")


# Verify the operator-published immutable remediation package independently.
publication = load_json(CONTROL / "review-correction-publication.json")
published = publication["files_sha256"]
actual_package_files = sorted(
    str(path.relative_to(CORRECTION)) for path in CORRECTION.rglob("*") if path.is_file()
)
check("published-file-count", len(published), 167, "control/review-correction-publication.json")
check("published-file-set", sorted(published), actual_package_files)
for relative, expected_hash in sorted(published.items()):
    check(f"published-hash:{relative}", sha256(CORRECTION / relative), expected_hash)

# Verify the producer's v2 pointer and manifests without treating its verdict as independent.
for manifest_name in [
    "zs-remediation-recheck-inputs-v2.sha256",
    "zs-remediation-recheck-bundle-v2.sha256",
]:
    manifest_path = RECEIPTS / manifest_name
    for line in manifest_path.read_text().splitlines():
        if not line.strip():
            continue
        expected_hash, relative = line.split(None, 1)
        relative = relative.strip()
        check(f"manifest:{manifest_name}:{relative}", sha256(PROJECT / relative), expected_hash)

# Confirm the current root producer files equal the sealed correction package at review start.
for relative in [
    "AGENTS.md",
    "DELIVERABLES.md",
    "GOALS.md",
    "PLAN.md",
    "RULES.md",
    "project.toml",
    "work-ledger.yaml",
]:
    check(
        f"current-source-equals-fixed:{relative}",
        sha256(PROJECT / relative),
        sha256(CORRECTION / "sources" / relative),
    )
for relative in [
    "zhusheng-interruption-diagnosis.md",
    "zhusheng-change-proposals.md",
    "zhusheng-recovery-work.md",
    "zhusheng-consistency.md",
    "zs-independent-review.md",
]:
    check(
        f"current-deliverable-equals-fixed:{relative}",
        sha256(PROJECT / "deliverables" / relative),
        sha256(CORRECTION / "deliverables" / relative),
    )

ledger = yaml.safe_load((PROJECT / "work-ledger.yaml").read_text())
works = {item["id"]: item for item in ledger["work_items"]}
recover = works["ZS-RECOVER"]
review = works["ZS-REVIEW"]
deliver = works["ZS-DELIVER"]
expected_acceptance = load_json(RECEIPTS / "zs-remediation-acceptance-patch.json")["acceptance"]
check("recover-acceptance-exact-four", recover["acceptance"], expected_acceptance)
check("recover-status", recover["status"], "completed")
check("recover-next-action-is-review-handoff", "返检" in recover["next_action"], True)
check("review-status-before-recheck", review["status"], "planned")
check("deliver-status-before-recheck", deliver["status"], "planned")
check("final-delivery-artifact-absent", (PROJECT / "deliverables/zs-delivery.md").exists(), False)

inventory = load_json(RECEIPTS / "reviewer-recheck-proposals-before.json")["proposals"]
ids = [proposal["id"] for proposal in inventory]
old_ids = [
    "01M21X4H7GSP7DY19MTHYDBXST",
    "01M21X5MHW7RFN8PBE4J86GACM",
    "01M21X67QFPQ55YSMJEX96HYV7",
]
combined_id = "01M225AVE1BFYY0W4T2EKRV88C"
for proposal_id in old_ids + [combined_id]:
    check(f"proposal-inventory-unique:{proposal_id}", ids.count(proposal_id), 1)

old_full = [
    load_json(RECEIPTS / "reviewer-recheck-cr-01-full.json"),
    load_json(RECEIPTS / "reviewer-recheck-cr-02-full.json"),
    load_json(RECEIPTS / "reviewer-recheck-cr-03-full.json"),
]
for record, proposal_id in zip(old_full, old_ids):
    check(f"old-proposal-status:{proposal_id}", record["proposal"]["status"], "rejected")
    check(f"old-proposal-apply-attempt:{proposal_id}", record["apply_attempt"], None)

combined = load_json(RECEIPTS / "reviewer-recheck-combined-full.json")
check("combined-proposal-status", combined["proposal"]["status"], "applied")
check("combined-proposal-id", combined["proposal"]["id"], combined_id)
check("combined-proposal-acceptance", combined["proposal"]["patch"]["changes"]["acceptance"], expected_acceptance)
check("combined-apply-attempt-event", combined["apply_attempt"]["event_id"], "01M225BCXTY18AWZSRG0B9WT1P")
check("combined-resolved-event", combined["apply_attempt"]["resolved_event_id"], "01M225BCYBY571X8FPBY8TGME8")
check("combined-write-before", combined["apply_attempt"]["plan"]["before_fingerprint"], "sha256:6ac1624435b6b626e32af360e94acb9bbcd406e9a2dfe0e3bf3926474fac612a")
check("combined-write-after", combined["apply_attempt"]["plan"]["after_fingerprint"], "sha256:83f9e1a0a4cb7b598e5ac0e60b712273b4c85f8e75a246ad59b11225c8ba17a0")

recover_meta = load_json(RECEIPTS / "reviewer-recheck-recover-metadata-full.json")
review_meta = load_json(RECEIPTS / "reviewer-recheck-review-metadata-full.json")
check("recover-metadata-fields", sorted(recover_meta["proposal"]["patch"]["changes"]), ["next_action", "summary"])
check("review-metadata-fields", sorted(review_meta["proposal"]["patch"]["changes"]), ["summary"])
check("recover-metadata-no-acceptance", "acceptance" in recover_meta["proposal"]["patch"]["changes"], False)
check("review-metadata-no-acceptance", "acceptance" in review_meta["proposal"]["patch"]["changes"], False)

# Current AWR reads must agree with the fixed business source before this reviewer mutates ZS-REVIEW.
recover_current_record = load_json(RECEIPTS / "reviewer-recheck-recover-current.json")
recover_current = recover_current_record["work"]
check("awr-recover-status", recover_current["status"], "completed")
check("awr-recover-source-revision", recover_current["source_revision"], 17)
check("awr-recover-acceptance", recover_current_record["acceptance"], expected_acceptance)
status_before = load_json(RECEIPTS / "reviewer-recheck-status-before.json")
check("awr-project-revision-before", status_before["project_revision"], 181)
check("awr-source-clean", status_before["source_issues"], [])
ready_before = load_json(RECEIPTS / "reviewer-recheck-ready-before.json")
check("review-ready-before", [x["external_key"] for x in ready_before["ready"]], ["ZS-REVIEW"])
check("active-sessions-before", load_json(RECEIPTS / "reviewer-recheck-active-sessions-before.json")["sessions"], [])

# Preserve unresolved facts and the original interruption distinction.
consistency = (PROJECT / "deliverables/zhusheng-consistency.md").read_text()
narrative = consistency + "\n" + (PROJECT / "deliverables/zhusheng-change-proposals.md").read_text()
for phrase in [
    "目录根路径、访问权限、资料迁移与完整性",
    "具体人员、值班表、代理/升级路径和交接时限",
    "具体遗留问题清单及逐项影响、负责人、下一步和完成条件",
    "旧 patch 应用 0 次",
    "新合并提案应用 1 次",
]:
    check(f"business-narrative-retains:{phrase}", phrase in narrative, True)

normal = load_json(PROJECT / "review-inputs/interruption-evidence/prelude-normal-exit-observation.json")
interruption = load_json(PROJECT / "review-inputs/interruption-evidence/actual-operator-interruption-v1.json")
binding = load_json(PROJECT / "review-inputs/interruption-evidence/verified-interruption-binding.json")
recovery = load_json(PROJECT / "review-inputs/interruption-evidence/actual-recovery-binding.json")
check("prelude-normal-exit", normal["native_returncode"], 0)
check("actual-sigkill", interruption["signal"], "SIGKILL")
check("actual-sigkill-sent", interruption["signal_sent"], True)
check("native-interruption-return-code", binding["native_returncode"], -9)
check("old-checkpoint-absent", recovery["prior_checkpoint"], None)
check("different-native-recovery", recovery["prior_native_task_id"] != recovery["recovery_native_task_id"], True)

report = {
    "version": 1,
    "kind": "independent_remediation_recheck_verification",
    "reviewer": "luna_worker",
    "recorded_at": datetime.now().astimezone().isoformat(timespec="seconds"),
    "checks_passed": len(checks),
    "checks_failed": 0,
    "checks": checks,
    "source_revision": 17,
    "source_sha256": sha256(PROJECT / "work-ledger.yaml"),
    "verdict_basis": "Independent checks of fixed hashes, current authority, full proposal records, interruption receipts, and current AWR reads; producer self-check results were not used as verdict authority.",
}
(RECEIPTS / "reviewer-recheck-verification.json").write_text(
    json.dumps(report, ensure_ascii=False, indent=2) + "\n"
)
print(json.dumps({"ok": True, "checks_passed": len(checks), "source_sha256": report["source_sha256"]}, ensure_ascii=False))
