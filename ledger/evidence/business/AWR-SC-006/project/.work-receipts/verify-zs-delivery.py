#!/usr/bin/env python3
import argparse
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AWR = Path("/Users/mac/Documents/originone/agent-work-running/target/release/awr")
SESSION = "01M229PEPFG7KC962FP55GCRMX"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(parts: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(parts, cwd=ROOT, text=True, capture_output=True, check=False)


def awr(*parts: str) -> dict:
    result = command(["rtk", "proxy", str(AWR), "--project", str(ROOT), "--json", *parts])
    if result.returncode != 0:
        raise RuntimeError(f"AWR command failed ({result.returncode}): {result.stderr or result.stdout}")
    return json.loads(result.stdout)


def git(*parts: str) -> subprocess.CompletedProcess[str]:
    return command(["rtk", "proxy", "git", *parts])


parser = argparse.ArgumentParser()
parser.add_argument("--phase", choices=["pre", "post"], required=True)
args = parser.parse_args()

checks: list[dict] = []


def check(name: str, condition: bool, actual=None, expected=None) -> None:
    checks.append({"name": name, "passed": bool(condition), "actual": actual, "expected": expected})


expected_hashes = {
    "materials/handover-draft.md": "4cbac09bd7a59e47e245a2d5f76e6c18a694680be26c411f97c503972df62361",
    "materials/change-requests.csv": "0872b48bc80817d733fd80b799d50e850406949f805db81b83aaa54eba433ee2",
    "materials/archive-confirmation.md": "03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c",
    "materials/role-confirmation.md": "8117dffe9cf6088adf357e9d0c404d1f4f547c430dd520cf1095d8cb27bf1da5",
    "deliverables/zhusheng-interruption-diagnosis.md": "a7feebb26b5f65e7da1eb34b0774393fb06b9835c4b4537b64028b247920a196",
    "deliverables/zhusheng-change-proposals.md": "e00e5e4e98db2476d0321fc86af82e93aa0a218edfab66c1b568f64335304a05",
    "deliverables/zhusheng-recovery-work.md": "46bfc0ea1ac8396b10280e06b3ed55a72a69242555010218974f8b4ece64fb31",
    "deliverables/zhusheng-consistency.md": "ddb10a828a04059aea355186b98a685167ac738aee77902d587b6506b585260e",
    "deliverables/zs-independent-review.md": "c580b03404518ac99436ef3f7f3ccfdb78315a7a493b3c53c18448bd685072b5",
    "deliverables/zs-independent-re-review.md": "5f8d595f33ce5fd48503d12e1076aab409b6f54605d36351ddf343e457d50173",
    ".work-receipts/reviewer-recheck-completion-report-v3.json": "c973140be6fbaefcab4075a56f8e27cd5d55b6ae4b0f5f23b91a31b140f21475",
}
for relative, expected in expected_hashes.items():
    path = ROOT / relative
    actual = sha256(path) if path.is_file() else None
    check(f"preserved-hash:{relative}", actual == expected, actual, expected)

delivery_path = ROOT / "deliverables/zs-delivery.md"
delivery_text = delivery_path.read_text(encoding="utf-8") if delivery_path.is_file() else ""
required_delivery_fragments = [
    "首次 prelude",
    "SIGKILL",
    "native return code 为 `-9`",
    "旧 session 的 `last_checkpoint_id` 为 `null`",
    "旧 patch 应用 0 次",
    "新合并提案应用 1 次",
    "使用说明",
    "交接记录",
    "问题跟踪",
    "值班人员维护交接记录",
    "资料管理员维护使用说明",
    "问题负责人维护问题跟踪",
    "目录根路径、访问权限",
    "具体人员、值班表、代理/升级路径和交接时限",
    "具体未结束问题清单",
    "Git/发布/E4",
    "无法恢复的未持久化信息",
    "01M225AVE1BFYY0W4T2EKRV88C",
    "01M21X4H7GSP7DY19MTHYDBXST",
    "01M21X5MHW7RFN8PBE4J86GACM",
    "01M21X67QFPQ55YSMJEX96HYV7",
]
check("delivery-file-present", delivery_path.is_file(), delivery_path.is_file(), True)
for fragment in required_delivery_fragments:
    check(f"delivery-fragment:{fragment}", fragment in delivery_text, fragment in delivery_text, True)

failure_paths = [
    ".work-receipts/cr-01-create-session-mismatch-error.json",
    ".work-receipts/zs-recovery-final-doctor.log",
    ".work-receipts/reviewer-tool-failures.json",
    ".work-receipts/zs-remediation-tool-failures.json",
    ".work-receipts/reviewer-recheck-tool-failures.json",
    ".work-receipts/reviewer-recheck-post-completion-tool-failures.json",
    ".work-receipts/reviewer-recheck-verification-first-failure.log",
    ".work-receipts/reviewer-recheck-verification-error-2.log",
    ".work-receipts/reviewer-recheck-work-complete.json",
    ".work-receipts/reviewer-recheck-work-complete-v2.json",
    ".work-receipts/reviewer-recheck-final-verification-first-failure.log",
    ".work-receipts/zs-deliver-evidence-add-failure.log",
]
missing_failure_paths = [relative for relative in failure_paths if not (ROOT / relative).exists()]
check("failure-history-preserved", not missing_failure_paths, missing_failure_paths, [])
empty_placeholders = sorted(
    relative for relative in failure_paths if (ROOT / relative).exists() and (ROOT / relative).stat().st_size == 0
)
check(
    "expected-empty-failure-placeholders",
    empty_placeholders == sorted([
        ".work-receipts/reviewer-recheck-work-complete.json",
        ".work-receipts/reviewer-recheck-work-complete-v2.json",
    ]),
    empty_placeholders,
    sorted([
        ".work-receipts/reviewer-recheck-work-complete.json",
        ".work-receipts/reviewer-recheck-work-complete-v2.json",
    ]),
)

recover = awr("work", "show", "ZS-RECOVER")
review = awr("work", "show", "ZS-REVIEW")
deliver = awr("work", "show", "ZS-DELIVER")
status = awr("status")
sessions = awr("session", "list")

expected_recover_acceptance = [
    "处理待应用改动，保留重试或恢复的证据，避免重复写入。",
    "保留实际产物与来源引用，无法确认的内容显式说明。",
    "资料归档按“使用说明、交接记录、问题跟踪”三个内容类型目录执行，并保留资料管理员确认来源（materials/archive-confirmation.md）；目录根路径、权限、迁移与完整性未明确时继续列为未决。",
    "角色职责按协调者确认执行：值班人员维护交接记录、资料管理员维护使用说明、问题负责人维护问题跟踪，并保留确认来源（materials/role-confirmation.md）；具体人员、值班表、代理/升级路径和交接时限未明确时继续列为未决。",
]
check("recover-completed", recover["work"]["status"] == "completed", recover["work"]["status"], "completed")
check("recover-acceptance-exact", recover["acceptance"] == expected_recover_acceptance, recover["acceptance"], expected_recover_acceptance)
check("recover-acceptance-unique", len(recover["acceptance"]) == len(set(recover["acceptance"])), len(recover["acceptance"]), len(set(recover["acceptance"])))
check("review-completed", review["work"]["status"] == "completed", review["work"]["status"], "completed")
check("review-recheck-summary", "独立整改返检已完成" in (review["work"].get("summary") or ""), review["work"].get("summary"), "contains independent recheck completion")
check("delivery-acceptance-exact", deliver["acceptance"] == ["交付最终交接说明及未决事项。", "保留实际产物与来源引用，无法确认的内容显式说明。"], deliver["acceptance"], "two authoritative criteria")

expected_delivery_status = "in_progress" if args.phase == "pre" else "completed"
check("delivery-status", deliver["work"]["status"] == expected_delivery_status, deliver["work"]["status"], expected_delivery_status)
check("delivery-summary-current", "最终交接说明已形成" in (deliver["work"].get("summary") or ""), deliver["work"].get("summary"), "contains final handoff formed")
next_action = deliver["work"].get("next_action") or ""
for fragment in ["CR-03", "归档实施证据", "具体人员", "升级", "Git 提交", "E4"]:
    check(f"delivery-next-action:{fragment}", fragment in next_action, fragment in next_action, True)

old_ids = [
    "01M21X4H7GSP7DY19MTHYDBXST",
    "01M21X5MHW7RFN8PBE4J86GACM",
    "01M21X67QFPQ55YSMJEX96HYV7",
]
old_proposals = [awr("proposal", "show", proposal_id, "--full") for proposal_id in old_ids]
for proposal_id, proposal in zip(old_ids, old_proposals):
    check(f"old-proposal-rejected:{proposal_id}", proposal["proposal"]["status"] == "rejected", proposal["proposal"]["status"], "rejected")
    check(f"old-proposal-zero-apply:{proposal_id}", proposal.get("apply_attempt") is None, proposal.get("apply_attempt"), None)

combined = awr("proposal", "show", "01M225AVE1BFYY0W4T2EKRV88C", "--full")
check("combined-proposal-applied", combined["proposal"]["status"] == "applied", combined["proposal"]["status"], "applied")
check("combined-proposal-single-attempt", combined["apply_attempt"]["event_id"] == "01M225BCXTY18AWZSRG0B9WT1P", combined["apply_attempt"]["event_id"], "01M225BCXTY18AWZSRG0B9WT1P")
check("combined-proposal-resolved-event", combined["apply_attempt"]["resolved_event_id"] == "01M225BCYBY571X8FPBY8TGME8", combined["apply_attempt"]["resolved_event_id"], "01M225BCYBY571X8FPBY8TGME8")
check("combined-acceptance-exact", combined["proposal"]["patch"]["changes"]["acceptance"] == expected_recover_acceptance, combined["proposal"]["patch"]["changes"]["acceptance"], expected_recover_acceptance)

metadata_ids = [
    "01M226DVJZNTTSTGZ20CHGQDWD",
    "01M226HF1GX6P579T58HV384G8",
    "01M228VHNTEF3J3RXB6YPQYDNS",
    "01M229TFNZC3XXMQ511NB7HY8C",
    "01M22A4P5E6HMKPFA2HHQQB35F",
]
metadata_proposals = [awr("proposal", "show", proposal_id, "--full") for proposal_id in metadata_ids]
for proposal_id, proposal in zip(metadata_ids, metadata_proposals):
    changes = proposal["proposal"]["patch"]["changes"]
    check(f"metadata-no-acceptance:{proposal_id}", "acceptance" not in changes, sorted(changes), "acceptance absent")

head = git("rev-parse", "HEAD")
check("git-head", head.returncode == 0 and head.stdout.strip() == "8218b29c348b7eda3e57b541c3489e41b87b6679", head.stdout.strip(), "8218b29c348b7eda3e57b541c3489e41b87b6679")
git_status = git("status", "--porcelain=v1")
check("git-worktree-not-clean", git_status.returncode == 0 and bool(git_status.stdout.strip()), git_status.stdout.splitlines(), "nonempty; no Git delivery claimed")
tracked_delivery = git("ls-files", "--error-unmatch", "deliverables/zs-delivery.md")
check("delivery-not-git-committed", tracked_delivery.returncode != 0, tracked_delivery.returncode, "nonzero")

active_sessions = [item["id"] for item in sessions["sessions"] if item["status"] == "active"]
if args.phase == "pre":
    check("only-delivery-session-active", active_sessions == [SESSION], active_sessions, [SESSION])
    check("project-counts-pre", status["counts"] == {"completed": 4, "in_progress": 1}, status["counts"], {"completed": 4, "in_progress": 1})
else:
    matching = [item for item in sessions["sessions"] if item["id"] == SESSION]
    check("no-active-session-post", active_sessions == [], active_sessions, [])
    check("delivery-session-ended", len(matching) == 1 and matching[0]["status"] == "ended", matching, "one ended delivery session")
    check("project-counts-post", status["counts"] == {"completed": 5}, status["counts"], {"completed": 5})
    locators = [item["locator"] for item in deliver.get("evidence", [])]
    check("delivery-evidence-associated", ".work-receipts/zs-deliver-completion-report.json" in locators, locators, "contains final completion report")

check("source-issues-empty", status.get("source_issues") == [], status.get("source_issues"), [])
failed = [item for item in checks if not item["passed"]]
result = {
    "version": 1,
    "kind": "zs_delivery_verification",
    "phase": args.phase,
    "verified_at": int(time.time() * 1000),
    "source_sha": head.stdout.strip(),
    "command": f"rtk proxy {sys.executable} .work-receipts/verify-zs-delivery.py --phase {args.phase}",
    "ok": not failed,
    "checks_passed": len(checks) - len(failed),
    "checks_failed": len(failed),
    "delivery_sha256": sha256(delivery_path) if delivery_path.is_file() else None,
    "project_revision": status["project_revision"],
    "source_revision": deliver["source_ref"]["source_revision"],
    "source_fingerprint": deliver["source_ref"]["source_fingerprint"],
    "active_sessions": active_sessions,
    "git_status": git_status.stdout.splitlines(),
    "checks": checks,
}
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(0 if result["ok"] else 1)
