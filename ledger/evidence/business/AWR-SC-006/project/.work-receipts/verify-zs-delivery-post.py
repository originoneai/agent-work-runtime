#!/usr/bin/env python3
import hashlib
import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AWR = Path("/Users/mac/Documents/originone/agent-work-running/target/release/awr")
SESSION = "01M229PEPFG7KC962FP55GCRMX"


def sha256(relative: str) -> str | None:
    path = ROOT / relative
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def run(parts: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(parts, cwd=ROOT, text=True, capture_output=True, check=False)


def awr(*parts: str) -> dict:
    result = run(["rtk", "proxy", str(AWR), "--project", str(ROOT), "--json", *parts])
    if result.returncode != 0:
        raise RuntimeError(result.stderr or result.stdout)
    return json.loads(result.stdout)


def load(relative: str) -> dict:
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


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
    "deliverables/zs-delivery.md": "d2bb0d5bf582eecadf4282239c2e6e1a1077bbb0d4bae0e2e3fa57646d3d9ee7",
    ".work-receipts/verify-zs-delivery.py": "587a9ad17433636e2acf7d92410249c1633f8e6c010f56055b5cf4d4677a13b6",
    ".work-receipts/zs-deliver-verification-v2.json": "52c4aa7089c65dbb86b0f1f148e3124f0f0f0764fc48e226853c95aafe522a08",
    ".work-receipts/zs-deliver-completion-report-v2.json": "746610addd54f1f3e5e6d21b055a184baaec79b57c65c091df11e820d411d340",
    ".work-receipts/zs-deliver-evidence-add-failure.log": "0f6f7156a247eb3592d56e87fa08a60e3e1bbce44f3764dfc5d0c6b81d9c539b",
    ".work-receipts/zs-deliver-evidence-draft.json": "791611d7a2965023d319d8f0791d4a6eb8881e136905caf2e7ee5531f6467ece",
    ".work-receipts/zs-deliver-completion-report.json": "b187f3499f5bf0992f882b3a9a3740c98b825e18cdeade4ff91d1a9b41559ede",
}
for relative, expected in expected_hashes.items():
    actual = sha256(relative)
    check(f"fixed-hash:{relative}", actual == expected, actual, expected)

status = awr("status")
deliver = awr("work", "show", "ZS-DELIVER")
recover = awr("work", "show", "ZS-RECOVER")
review = awr("work", "show", "ZS-REVIEW")
sessions = awr("session", "list")
doctor = awr("doctor")
proposals = awr("proposal", "list", "--limit", "100")
completion_proposal = awr("proposal", "show", "01M22AR6YG28VEMAZZZ8ME9QNW", "--full")
combined_proposal = awr("proposal", "show", "01M225AVE1BFYY0W4T2EKRV88C", "--full")

check("all-work-completed", status["counts"] == {"completed": 5}, status["counts"], {"completed": 5})
check("no-current-work", status["current"] == [] and status["current_total"] == 0, status["current"], [])
check("no-ready-work", status["ready_count"] == 0 and status["suggested_work"] is None, status["ready_count"], 0)
check("status-next-action-terminal", status["next_action"] == "No nonterminal work remains in current source projections.", status["next_action"], "No nonterminal work remains in current source projections.")
check("source-issues-empty", status["source_issues"] == [], status["source_issues"], [])
check("final-project-revision", status["project_revision"] == 235, status["project_revision"], 235)

check("delivery-completed", deliver["work"]["status"] == "completed", deliver["work"]["status"], "completed")
check("delivery-source-revision", deliver["source_ref"]["source_revision"] == 23, deliver["source_ref"]["source_revision"], 23)
check("delivery-source-fingerprint", deliver["source_ref"]["source_fingerprint"] == "sha256:d27d51cfc9c3f733a3db94b13c12b668afe22ab1d28b0728c4df8e331f9f31fa", deliver["source_ref"]["source_fingerprint"], "sha256:d27d51cfc9c3f733a3db94b13c12b668afe22ab1d28b0728c4df8e331f9f31fa")
check("delivery-summary", "最终交接说明已形成" in (deliver["work"].get("summary") or ""), deliver["work"].get("summary"), "contains final handoff formed")
next_action = deliver["work"].get("next_action") or ""
for fragment in ["CR-03", "归档实施证据", "具体人员", "值班", "代理", "升级", "Git 提交", "发布", "E4"]:
    check(f"next-action-retains:{fragment}", fragment in next_action, fragment in next_action, True)

evidence = deliver["evidence"]
runtime_evidence = [item for item in evidence if item["external_key"] == "zs-final-recovery-delivery-20260909-v2"]
source_evidence = [item for item in evidence if item["external_key"] == "ZS-DELIVER/evidence/.work-receipts/zs-deliver-completion-report-v2.json"]
check("one-runtime-evidence", len(runtime_evidence) == 1, len(runtime_evidence), 1)
check("one-source-projected-evidence", len(source_evidence) == 1, len(source_evidence), 1)
check("same-evidence-locator-two-projections", len(evidence) == 2 and {item["locator"] for item in evidence} == {".work-receipts/zs-deliver-completion-report-v2.json"}, [item["locator"] for item in evidence], "one runtime registration plus one source projection of same locator")
check("runtime-evidence-level", len(runtime_evidence) == 1 and runtime_evidence[0]["level"] == "locally_verified", runtime_evidence[0]["level"] if runtime_evidence else None, "locally_verified")

expected_recover_acceptance = [
    "处理待应用改动，保留重试或恢复的证据，避免重复写入。",
    "保留实际产物与来源引用，无法确认的内容显式说明。",
    "资料归档按“使用说明、交接记录、问题跟踪”三个内容类型目录执行，并保留资料管理员确认来源（materials/archive-confirmation.md）；目录根路径、权限、迁移与完整性未明确时继续列为未决。",
    "角色职责按协调者确认执行：值班人员维护交接记录、资料管理员维护使用说明、问题负责人维护问题跟踪，并保留确认来源（materials/role-confirmation.md）；具体人员、值班表、代理/升级路径和交接时限未明确时继续列为未决。",
]
check("recover-still-completed", recover["work"]["status"] == "completed", recover["work"]["status"], "completed")
check("recover-acceptance-still-exact", recover["acceptance"] == expected_recover_acceptance, recover["acceptance"], expected_recover_acceptance)
check("recover-acceptance-still-unique", len(recover["acceptance"]) == len(set(recover["acceptance"])) == 4, len(recover["acceptance"]), 4)
check("review-still-completed", review["work"]["status"] == "completed", review["work"]["status"], "completed")

active_sessions = [item["id"] for item in sessions["sessions"] if item["status"] == "active"]
matching_sessions = [item for item in sessions["sessions"] if item["id"] == SESSION]
check("no-active-sessions", active_sessions == [], active_sessions, [])
check("delivery-session-ended", len(matching_sessions) == 1 and matching_sessions[0]["status"] == "ended", matching_sessions, "one ended session")
check("delivery-session-checkpoint", len(matching_sessions) == 1 and matching_sessions[0]["last_checkpoint_id"] == "01M22AS1HW41MK9HVYD59WTD70", matching_sessions[0]["last_checkpoint_id"] if matching_sessions else None, "01M22AS1HW41MK9HVYD59WTD70")
check("doctor-clean", doctor["ok"] is True and doctor["findings"] == [] and doctor["integrity"] == ["ok"], {"ok": doctor["ok"], "findings": doctor["findings"], "integrity": doctor["integrity"]}, {"ok": True, "findings": [], "integrity": ["ok"]})

proposal_rows = proposals["proposals"]
check("proposal-inventory-complete", proposals["may_have_more"] is False, proposals["may_have_more"], False)
pending = [item["id"] for item in proposal_rows if item["status"] in {"draft", "ready", "approved"}]
check("no-unresolved-proposals", pending == [], pending, [])
for proposal_id in ["01M21X4H7GSP7DY19MTHYDBXST", "01M21X5MHW7RFN8PBE4J86GACM", "01M21X67QFPQ55YSMJEX96HYV7"]:
    rows = [item for item in proposal_rows if item["id"] == proposal_id]
    detail = awr("proposal", "show", proposal_id, "--full")
    check(f"old-proposal-unique:{proposal_id}", len(rows) == 1, len(rows), 1)
    check(f"old-proposal-rejected:{proposal_id}", detail["proposal"]["status"] == "rejected", detail["proposal"]["status"], "rejected")
    check(f"old-proposal-zero-apply:{proposal_id}", detail.get("apply_attempt") is None, detail.get("apply_attempt"), None)
check("combined-proposal-unique", len([item for item in proposal_rows if item["id"] == "01M225AVE1BFYY0W4T2EKRV88C"]) == 1, len([item for item in proposal_rows if item["id"] == "01M225AVE1BFYY0W4T2EKRV88C"]), 1)
check("combined-proposal-one-apply", combined_proposal["proposal"]["status"] == "applied" and combined_proposal["apply_attempt"]["event_id"] == "01M225BCXTY18AWZSRG0B9WT1P", combined_proposal["apply_attempt"], "one resolved apply attempt")
check("delivery-completion-proposal-applied", completion_proposal["proposal"]["status"] == "applied", completion_proposal["proposal"]["status"], "applied")
check("delivery-completion-no-acceptance", "acceptance" not in completion_proposal["proposal"]["patch"]["changes"], sorted(completion_proposal["proposal"]["patch"]["changes"]), "acceptance absent")
check("delivery-completion-event", completion_proposal["apply_attempt"]["resolved_event_id"] == "01M22AR70DZQ28FM465Z8V0ZGX", completion_proposal["apply_attempt"]["resolved_event_id"], "01M22AR70DZQ28FM465Z8V0ZGX")

evidence_add = load(".work-receipts/zs-deliver-evidence-add-v2.json")
complete = load(".work-receipts/zs-deliver-work-complete.json")
checkpoint = load(".work-receipts/zs-deliver-checkpoint.json")
session_end = load(".work-receipts/zs-deliver-session-end.json")
tool_failures = load(".work-receipts/zs-deliver-tool-failures.json")
post_failures = load(".work-receipts/zs-deliver-post-completion-tool-failures-v2.json")
check("evidence-add-receipt", evidence_add["ok"] and evidence_add["evidence"]["id"] == "01M22ANMPA7J6GTMRMDN0Z8MPS" and evidence_add["project_revision"] == 225, {"id": evidence_add["evidence"]["id"], "revision": evidence_add["project_revision"]}, {"id": "01M22ANMPA7J6GTMRMDN0Z8MPS", "revision": 225})
check("completion-receipt", complete["ok"] and complete["event"]["id"] == "01M22AR70DZQ28FM465Z8V0ZGX" and complete["project_revision"] == 232, {"event": complete["event"]["id"], "revision": complete["project_revision"]}, {"event": "01M22AR70DZQ28FM465Z8V0ZGX", "revision": 232})
check("checkpoint-receipt", checkpoint["ok"] and checkpoint["checkpoint"]["id"] == "01M22AS1HW41MK9HVYD59WTD70" and len(checkpoint["checkpoint"]["open_loops"]) == 4, {"id": checkpoint["checkpoint"]["id"], "open_loops": checkpoint["checkpoint"]["open_loops"]}, "checkpoint plus four open loops")
check("session-end-receipt", session_end["ok"] and session_end["session"]["status"] == "ended" and session_end["project_revision"] == 235, {"status": session_end["session"]["status"], "revision": session_end["project_revision"]}, {"status": "ended", "revision": 235})
check("failed-evidence-input-preserved", len(tool_failures["failures"]) == 1 and tool_failures["failed_input_overwritten"] is False, tool_failures, "one preserved failed input and successful v2 recovery")
check("post-failures-preserved", len(post_failures["failures"]) == 2 and post_failures["source_or_runtime_mutated"] is False, post_failures, "two read-only post-completion failures")

git_head = run(["rtk", "proxy", "git", "rev-parse", "HEAD"])
git_status = run(["rtk", "proxy", "git", "status", "--porcelain=v1"])
check("git-head-unchanged", git_head.returncode == 0 and git_head.stdout.strip() == "8218b29c348b7eda3e57b541c3489e41b87b6679", git_head.stdout.strip(), "8218b29c348b7eda3e57b541c3489e41b87b6679")
check("git-not-delivered", git_status.returncode == 0 and bool(git_status.stdout.strip()), git_status.stdout.splitlines(), "dirty worktree retained; no Git delivery")

ledger_text = (ROOT / "work-ledger.yaml").read_text(encoding="utf-8")
milestone_status = "in_progress" if "  status: in_progress" in ledger_text.split("work_items:", 1)[0] else "unknown"
failed = [item for item in checks if not item["passed"]]
result = {
    "version": 1,
    "kind": "zs_delivery_post_completion_verification",
    "verified_at": int(time.time() * 1000),
    "ok": not failed,
    "checks_passed": len(checks) - len(failed),
    "checks_failed": len(failed),
    "project_revision": status["project_revision"],
    "source_revision": deliver["source_ref"]["source_revision"],
    "source_fingerprint": deliver["source_ref"]["source_fingerprint"],
    "work_counts": status["counts"],
    "active_sessions": active_sessions,
    "milestone_header_status": milestone_status,
    "milestone_note": "The source grouping marker remains in_progress; AWR exposes no milestone transition command. All five source work items are completed and no nonterminal work remains; no direct source bypass was used.",
    "evidence_projection_note": "The same locator appears once as registered runtime evidence and once as a source-projected evidence reference after work completion; this is two projections of one report, not two evidence registrations or two business applications.",
    "git_status": git_status.stdout.splitlines(),
    "checks": checks,
  }
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(0 if result["ok"] else 1)
