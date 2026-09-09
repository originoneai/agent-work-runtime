#!/usr/bin/env python3
import hashlib
import json
import re
import time
from pathlib import Path


PROJECT = Path("/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/source-change/project")
OUTPUT = PROJECT / ".work-receipts/re-review-verification.json"
REPORT = PROJECT / "deliverables/sg-independent-re-review.md"
PACKAGE_REPORT = PROJECT / ".work-receipts/re-review-package-verification.json"
CONTEXT = PROJECT / ".work-receipts/re-review-context-final.json"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


package = json.loads(PACKAGE_REPORT.read_text())
context = json.loads(CONTEXT.read_text())
report_text = REPORT.read_text()

producer_expected = {
    "deliverables/songguo-source-versions.md": "26bac246b439a54551533d6146b0d12045830b539ce5a5deb0b2e1b431eac136",
    "deliverables/songguo-change-impact.md": "c1a644f08291eaf7b716063c11a4ab07efd319ad82ed9d08cc4e7cc6505756cb",
    "deliverables/songguo-revised-plan.md": "8ae8430b33e93a07b74e4c3c26b59bfa3c29e956b590d8eec1883766b5083550",
    "deliverables/sg-review-response.md": "ee0f42b76eeef5755fc79d737beba6e9ccdd163f93c954c2a253c97bec191018",
}
producer_post = {
    rel: {
        "expected_sha256": expected,
        "actual_sha256": sha(PROJECT / rel),
        "unchanged_during_re_review": sha(PROJECT / rel) == expected,
    }
    for rel, expected in producer_expected.items()
}

links = []
missing_links = []
for target in re.findall(r"\]\(([^)]+)\)", report_text):
    clean = target.split("#", 1)[0]
    if not clean or clean.startswith(("http://", "https://")):
        continue
    resolved = REPORT.parent.joinpath(clean).resolve()
    item = {"target": target, "resolved": str(resolved), "exists": resolved.exists()}
    links.append(item)
    if not resolved.exists():
        missing_links.append(item)

required_findings = [
    "R-01.1",
    "R-01.2",
    "R-01.3",
    "R-01.4",
    "R-01.5",
    "R-02.1",
    "R-02.2",
    "R-02.3",
]
checks = [
    {
        "name": "sealed correction package and producer receipts",
        "passed": (
            package["ok"]
            and package["publication"]["declared_file_count"] == 103
            and package["published_execution_receipt_count"] == 89
            and not package["published_receipt_json_parse"]["parse_errors"]
            and package["sealed_execution_counts"]["nonzero_commands"] == 4
        ),
        "details": (
            "Verified all 103 published files, all 89 published work receipts, "
            "all 86 JSON receipt parses, and the retained sealed execution counts."
        ),
        "criteria": [
            "保留实际产物与来源引用，无法确认的内容显式说明。"
        ],
    },
    {
        "name": "R-01 current and historical source chain",
        "passed": (
            all(item["unchanged_during_re_review"] for item in producer_post.values())
            and all(finding in report_text for finding in required_findings[:5])
            and "REMEDIATION_ACCEPTED_FOR_PLAN_SCOPE" in report_text
            and "接入开放日期尚未确认" in report_text
        ),
        "details": (
            "Verified the draft-three, historical-two, current-one and Xiqiao "
            "chain, both real revision turns, supersession boundaries, exact "
            "producer hashes and resolved cross-references."
        ),
        "criteria": [
            "核对最新规则、来源版本和受影响的验收条件。"
        ],
    },
    {
        "name": "R-02 retained technical lookup deviation",
        "passed": (
            all(finding in report_text for finding in required_findings[5:])
            and "item_46" in report_text
            and all(item in report_text for item in ["item_61", "item_63", "item_64", "item_66"])
            and "不能用于申领 E4" in report_text
            and package["reference_lookup_assessment"]["all_business_keyword_hits_zero"]
            and package["immutable_review_records"]["review-inputs/reference-lookup-process-record.json"]["match"]
        ),
        "details": (
            "Kept item_46 as allowed at the time and item_61/63/64/66 as "
            "historical scope deviations; retained zero business-keyword hits, "
            "limited impact language and the no-E4 boundary."
        ),
        "criteria": [
            "核对最新规则、来源版本和受影响的验收条件。",
            "保留实际产物与来源引用，无法确认的内容显式说明。",
        ],
    },
    {
        "name": "independent report and delivery boundary",
        "passed": (
            not missing_links
            and not (PROJECT / "deliverables/sg-delivery.md").exists()
            and package["immutable_review_records"]["deliverables/sg-independent-review.md"]["match"]
            and all(
                phrase in report_text
                for phrase in [
                    "不是溪桥或其他伙伴的接入批准",
                    "资料核验通过",
                    "真实业务验收",
                    "最终交付",
                    "E4",
                    "更早受控权威版本",
                    "批准链",
                ]
            )
        ),
        "details": (
            "The new report is independently authored, all its links resolve, "
            "the original review remains unchanged, all unknowns remain explicit, "
            "and final delivery remains absent for the executor."
        ),
        "criteria": [
            "保留实际产物与来源引用，无法确认的内容显式说明。"
        ],
    },
]

result = {
    "version": 1,
    "work_item": "SG-REVIEW",
    "source_sha": "146ed3bbf79fd0561158e46bb2219eeb05c89e67",
    "command": "rtk python3 .work-receipts/re-review-post-verify.py",
    "scope": ["SG-REVIEW"],
    "verified_at": int(time.time() * 1000),
    "reviewer": {
        "canonical_task": "/root/restricted_material_client_run",
        "participant_kind": "luna_worker",
        "awr_session": "01M2202ANNA5WFK3Q9DZ015B7A",
        "awr_claim": "01M2203KGQ2881PWDRPWTH2SX1",
        "independent_from_client_task_id": "01a08381-04a8-7871-9372-a3f55c2d8367",
    },
    "verdict": "REMEDIATION_ACCEPTED_FOR_PLAN_SCOPE",
    "artifact": {
        "path": "deliverables/sg-independent-re-review.md",
        "sha256": sha(REPORT),
    },
    "source_context": {
        "project_revision": context["project_revision"],
        "context_hash": context["work_context"]["context_hash"],
        "complete": context["completeness"]["complete"],
        "correction_source_sha256": {
            "GOALS.md": package["package_files"]["sources/GOALS.md"]["actual_sha256"],
            "PLAN.md": package["package_files"]["sources/PLAN.md"]["actual_sha256"],
            "RULES.md": package["package_files"]["sources/RULES.md"]["actual_sha256"],
            "work-ledger.yaml": package["package_files"]["sources/work-ledger.yaml"]["actual_sha256"],
        },
    },
    "publication": package["publication"],
    "package_verification": {
        "path": ".work-receipts/re-review-package-verification.json",
        "sha256": sha(PACKAGE_REPORT),
        "ok": package["ok"],
        "declared_files": package["publication"]["declared_file_count"],
        "published_receipts": package["published_execution_receipt_count"],
    },
    "producer_files_post_re_review": producer_post,
    "report_links": {"count": len(links), "missing": missing_links},
    "checks": checks,
    "all_checks_passed": all(check["passed"] for check in checks),
    "closed_findings": ["R-01.1", "R-01.2", "R-01.3", "R-01.4", "R-01.5"],
    "retained_process_findings": ["R-02.1", "R-02.2", "R-02.3"],
    "unresolved": [
        "The onboarding opening date remains unconfirmed.",
        "The Xiqiao field-description body and actual verification evidence are absent.",
        "The Yunling responsibility statement and Nanyuan directory remain absent.",
        "Named executor roles, onboarding approval and real business acceptance remain absent.",
        "Earlier controlled authoritative versions, exact edit time and approval chain remain absent.",
        "The original executor must consume this re-review and perform final delivery; this reviewer does not write it.",
    ],
    "evidence_boundary": (
        "Locally verified independent acceptance of the plan-scope remediation only. "
        "R-02 remains a historical scope deviation. No partner onboarding approval, "
        "material verification, final delivery, E4, clean-scope execution claim, or "
        "real business acceptance is granted."
    ),
}
OUTPUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({
    "ok": result["all_checks_passed"],
    "verdict": result["verdict"],
    "report_sha256": result["artifact"]["sha256"],
    "package_verification_sha256": result["package_verification"]["sha256"],
    "missing_report_links": missing_links,
    "output": str(OUTPUT),
}, ensure_ascii=False, indent=2))
raise SystemExit(0 if result["all_checks_passed"] else 1)
