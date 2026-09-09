#!/usr/bin/env python3
import hashlib
import json
import re
import subprocess
import time
from pathlib import Path


PROJECT = Path("/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/source-change/project")
OUTPUT = PROJECT / ".work-receipts/sg-delivery-verification.json"
DELIVERY = PROJECT / "deliverables/sg-delivery.md"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


expected = {
    "GOALS.md": "72db42bd7c8026e45523101a06a3bc3c4f23ab7fd92fbbbe745cc2e15b21a259",
    "PLAN.md": "a5fdfcd2e2922d451655eafc43705a8b0ac21baf3587fec73bce49caa15972b5",
    "RULES.md": "bbd2f4b5241b1c3fbdd774c9c749464960beaa47ac1d70501f2ddcdae441499a",
    "materials/clarification.md": "8177205a3f6aa84f02efb5f3ebb28333c4711db302f6400d443608cb38cf1e1e",
    "materials/partner-requests.csv": "af1cd4a95205845f01b56e9ff182d1f5cddbe06b47f0e1b9bb64001a5cf38db3",
    "deliverables/songguo-source-versions.md": "26bac246b439a54551533d6146b0d12045830b539ce5a5deb0b2e1b431eac136",
    "deliverables/songguo-change-impact.md": "c1a644f08291eaf7b716063c11a4ab07efd319ad82ed9d08cc4e7cc6505756cb",
    "deliverables/songguo-revised-plan.md": "8ae8430b33e93a07b74e4c3c26b59bfa3c29e956b590d8eec1883766b5083550",
    "deliverables/sg-review-response.md": "ee0f42b76eeef5755fc79d737beba6e9ccdd163f93c954c2a253c97bec191018",
    "deliverables/sg-independent-review.md": "735fe7cff0bc7357b9db30d2ef79f82fc9822932a546811b4a2403ef98fa7ee7",
    "deliverables/sg-independent-re-review.md": "479367efd6e411692552ec40d47af607fa48dee64c344c2824a67308575af72c",
    "review-inputs/reference-lookup-process-record.json": "b2f75186aee39d65367c6ce7aca7b710a12710e43a942debdefeb88d917062c5",
}
actual = {rel: sha256(PROJECT / rel) for rel in expected}
source_matches = {rel: actual[rel] == value for rel, value in expected.items()}

historical = {
    "review-inputs/initial/deliverables/songguo-source-versions.md": "b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04",
    "review-inputs/round-1/deliverables/songguo-source-versions.md": "b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04",
    "review-inputs/round-2/deliverables/songguo-source-versions.md": "b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04",
    "review-inputs/round-1/deliverables/songguo-change-impact.md": "6792b22621c6364fd3802b5a7e5c9552710f418df917ffdac819dcb0d5fe3beb",
    "review-inputs/round-2/deliverables/songguo-revised-plan.md": "58e419ab7ff87b2566fc156f3256b01e444987be7e8fae6dc6ad727027b0ca37",
}
historical_matches = {
    rel: sha256(PROJECT / rel) == value for rel, value in historical.items()
}

body = DELIVERY.read_text()
required_phrases = [
    "接入开放日期尚未确认",
    "当前实际资料核对窗口只有一家，由溪桥占用",
    "云岭只并行补联系人职责说明和申请材料",
    "南园只并行准备并提交资料目录",
    "具体姓名尚未确认",
    "REMEDIATION_ACCEPTED_FOR_PLAN_SCOPE",
    "R-01.1",
    "R-01.5",
    "R-02.1",
    "R-02.3",
    "item_46",
    "item_61",
    "item_63",
    "item_64",
    "item_66",
    "不能据此申领 E4",
    "原轮次不能称为干净范围执行",
    "计划范围的最终交付，不是伙伴接入批准或真实业务验收",
]

missing_links = []
resolved_links = []
for target in re.findall(r"\]\(([^)]+)\)", body):
    clean = target.split("#", 1)[0]
    if not clean or clean.startswith(("http://", "https://")):
        continue
    resolved = (DELIVERY.parent / clean).resolve()
    record = {"target": target, "resolved": str(resolved)}
    if resolved.exists():
        resolved_links.append(record)
    else:
        missing_links.append(record)

process = json.loads(
    (PROJECT / "review-inputs/reference-lookup-process-record.json").read_text()
)
command_ids = [item["id"] for item in process["commands"]]
git_sha = subprocess.run(
    ["git", "rev-parse", "HEAD"],
    cwd=PROJECT,
    check=True,
    capture_output=True,
    text=True,
).stdout.strip()

checks = {
    "reviewed_sources_unchanged": all(source_matches.values()),
    "historical_snapshots_unchanged": all(historical_matches.values()),
    "current_arrangement_complete": all(phrase in body for phrase in required_phrases[:5]),
    "review_chain_and_scope_boundary_complete": all(
        phrase in body for phrase in required_phrases[5:]
    ),
    "all_unresolved_categories_retained": all(
        phrase in body
        for phrase in [
            "接入开放日期",
            "字段说明正文或可访问引用",
            "云岭联系人职责说明",
            "南园资料目录",
            "具体身份",
            "伙伴接入批准和真实业务验收记录",
            "更早受控权威版本",
        ]
    ),
    "handler_roles_and_unknown_people_retained": all(
        phrase in body
        for phrase in [
            "计划执行负责人",
            "伙伴资料联络人",
            "资料核对人",
            "有权业务审批人",
            "独立业务验收人",
            "来源或档案保管人",
            "尚未确认",
        ]
    ),
    "r02_process_record_retained": command_ids
    == ["item_46", "item_61", "item_63", "item_64", "item_66"],
    "all_delivery_links_resolve": not missing_links,
    "git_source_binding_matches": git_sha
    == "146ed3bbf79fd0561158e46bb2219eeb05c89e67",
}

result = {
    "version": 1,
    "work_item": "SG-DELIVER",
    "source_sha": git_sha,
    "command": "rtk python3 .work-receipts/sg-delivery-verify.py",
    "scope": ["SG-DELIVER"],
    "verified_at": int(time.time() * 1000),
    "verdict": "DELIVERY_PACKAGE_LOCALLY_VERIFIED",
    "artifact": {
        "path": "deliverables/sg-delivery.md",
        "sha256": sha256(DELIVERY),
    },
    "reviewed_sources": {
        rel: {
            "expected_sha256": expected[rel],
            "actual_sha256": actual[rel],
            "match": source_matches[rel],
        }
        for rel in expected
    },
    "historical_snapshots": {
        rel: {
            "expected_sha256": historical[rel],
            "actual_sha256": sha256(PROJECT / rel),
            "match": historical_matches[rel],
        }
        for rel in historical
    },
    "review_chain": {
        "original_review_verdict": "CHANGES_REQUIRED",
        "independent_re_review_verdict": "REMEDIATION_ACCEPTED_FOR_PLAN_SCOPE",
        "closed_findings": ["R-01.1", "R-01.2", "R-01.3", "R-01.4", "R-01.5"],
        "retained_process_findings": ["R-02.1", "R-02.2", "R-02.3"],
    },
    "checks": [
        {
            "name": "final arrangement and source version delivery",
            "passed": checks["current_arrangement_complete"]
            and checks["reviewed_sources_unchanged"]
            and checks["all_delivery_links_resolve"],
            "details": "The delivery binds the current one-window Xiqiao arrangement to unchanged current sources and exact re-reviewed artifact hashes; all local references resolve.",
            "criteria": ["处理复核意见并交付版本对照与最终计划。"],
        },
        {
            "name": "unresolved facts and role ownership",
            "passed": checks["all_unresolved_categories_retained"]
            and checks["handler_roles_and_unknown_people_retained"],
            "details": "Unavailable materials, verification, named owners, approval, historical authority and all dates remain explicitly unconfirmed, with next-handler roles and blocking scope recorded.",
            "criteria": ["保留实际产物与来源引用，无法确认的内容显式说明。"],
        },
        {
            "name": "original review remediation and technical deviation retention",
            "passed": checks["review_chain_and_scope_boundary_complete"]
            and checks["r02_process_record_retained"]
            and checks["historical_snapshots_unchanged"],
            "details": "The original review, response, re-review, sealed revisions and item_46/61/63/64/66 lookup record remain traceable; R-02 is not retroactively normalized and earns no E4 credit.",
            "criteria": [
                "处理复核意见并交付版本对照与最终计划。",
                "保留实际产物与来源引用，无法确认的内容显式说明。",
            ],
        },
    ],
    "raw_checks": checks,
    "links": {"resolved_count": len(resolved_links), "missing": missing_links},
    "all_checks_passed": all(checks.values()),
    "unresolved": [
        "The onboarding opening date remains unconfirmed.",
        "The Xiqiao field-description body, version, receipt metadata, fingerprint, actual verification and rework evidence remain unavailable.",
        "The Yunling responsibility statement and remaining application materials remain unavailable.",
        "The Nanyuan directory remains unavailable.",
        "Named execution, liaison, verification, approval and real-acceptance owners remain unconfirmed.",
        "Approval, real business acceptance and all related dates remain unconfirmed.",
        "Earlier controlled authoritative versions, exact change time and approval chain remain unavailable.",
    ],
    "evidence_boundary": "Locally verified plan-scope final delivery only. The independent re-review closes R-01 and accepts R-02 disclosure, but does not grant partner onboarding approval, material verification, real business acceptance, E4, or a clean-scope historical execution claim.",
}

result["all_checks_passed"] = result["all_checks_passed"] and all(
    item["passed"] for item in result["checks"]
)
OUTPUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(0 if result["all_checks_passed"] else 1)
