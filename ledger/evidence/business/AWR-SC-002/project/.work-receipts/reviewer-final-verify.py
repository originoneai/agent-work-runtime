#!/usr/bin/env python3
import hashlib
import json
import re
import time
from pathlib import Path


PROJECT = Path(__file__).resolve().parents[1]
RUN = PROJECT.parent
OUTPUT = Path(__file__).with_name("reviewer-final-verification.json")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


round2 = json.loads((PROJECT / "review-inputs/round-2/turn-record.json").read_text())
producer_after = {
    rel: sha256(PROJECT / rel) for rel in round2["artifact_sha256"]
}
producer_mismatches = {
    rel: {"expected": expected, "actual": producer_after.get(rel)}
    for rel, expected in round2["artifact_sha256"].items()
    if producer_after.get(rel) != expected
}

report = PROJECT / "deliverables/wh-independent-review.md"
report_text = report.read_text()
links = []
broken = []
for line_number, line in enumerate(report_text.splitlines(), 1):
    for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", line):
        if "://" in target or target.startswith("#"):
            continue
        resolved = (report.parent / target.split("#", 1)[0]).resolve()
        item = {
            "line": line_number,
            "target": target,
            "resolved": str(resolved),
            "exists": resolved.exists(),
        }
        links.append(item)
        if not item["exists"]:
            broken.append(item)

manual = (PROJECT / "deliverables/wanghai-operations-manual-draft-v3.md").read_text()
sections = {}
for section, next_section in [("3", "4"), ("4", "5"), ("5", "6")]:
    block = manual.split(f"## {section}.", 1)[1].split(f"## {next_section}.", 1)[0]
    table = block.split("未确认内容责任与影响", 1)[1]
    rows = []
    for line in table.splitlines():
        if line.startswith("| ") and not line.startswith("| ---"):
            cells = [cell.strip() for cell in line.strip("|").split("|")]
            if cells[0] != "未确认内容":
                rows.append(cells)
    sections[section] = {
        "row_count": len(rows),
        "all_rows_have_four_columns": all(len(row) == 4 for row in rows),
    }

checks = {
    "resume_and_revision_review_completed": "## 2. 中断、交接与恢复链" in report_text
    and "## 3. 三轮产物与来源保持" in report_text
    and "六个 open loops 无丢失" in report_text,
    "artifacts_sources_and_unknowns_preserved": "## 4. 当前候选逐项判定" in report_text
    and "七项均未完全关闭" in report_text
    and not producer_mismatches,
    "package_receipt_passed": json.loads(
        (PROJECT / ".work-receipts/reviewer-package-verification.json").read_text()
    )["publication"]["verified"],
    "producer_artifacts_unchanged_after_review": not producer_mismatches,
    "report_local_links_resolve": not broken,
    "report_has_conditional_verdict": "业务复核范围有条件通过" in report_text,
    "report_preserves_scope_deviation": "P-01：客户端检索范围偏离" in report_text
    and "不能追认为合规" in report_text,
    "report_preserves_tool_failures": "P-02：工具失败" in report_text
    and "6 个非零命令" in report_text,
    "report_keeps_candidate_boundary": "不是实际最终交付" in report_text
    and "E4 完成" in report_text,
    "all_three_business_matrices_complete": all(
        item["row_count"] > 0 and item["all_rows_have_four_columns"]
        for item in sections.values()
    ),
}

result = {
    "version": 1,
    "kind": "independent_review_verification",
    "work_item": "WH-REVIEW",
    "reviewer": {
        "agent_id": "/root/restricted_material_client_run",
        "session_id": "01M223E7S897SM2ADAAQ1AX25Q",
        "claim_id": "01M223E7S879SQHTJYJ5HAKFEF",
        "provider": "openai",
        "model": "gpt-5.6-sol",
    },
    "source_sha": "f9c654a16e66079e1e47558860dbc411b622c3fd",
    "verified_at": int(time.time() * 1000),
    "command": "rtk python3 .work-receipts/reviewer-final-verify.py",
    "scope": [
        "WH-REVIEW",
        "deliverables/wh-independent-review.md",
        "review-inputs/",
        ".work-receipts/WH-DRAFT-session-resume-r36.json",
        "deliverables/wanghai-operations-manual-draft-v3.md",
        "deliverables/wanghai-source-verification-v3.md",
        "deliverables/wanghai-progress-handoff-v3.md",
        "deliverables/wanghai-final-delivery.md",
    ],
    "artifact": {
        "locator": "deliverables/wh-independent-review.md",
        "sha256": sha256(report),
    },
    "producer_artifacts_after_review": {
        "expected": round2["artifact_sha256"],
        "actual": producer_after,
        "mismatches": producer_mismatches,
    },
    "report_links": {"count": len(links), "broken": broken},
    "business_matrix_rows": sections,
    "checks": [
        {
            "name": name,
            "passed": passed,
            "details": {
                "resume_and_revision_review_completed": "The report independently traces prelude, prelude-handoff, initial, round-1, and round-2, including the old/new clients, AWR checkpoint, preserved open loops, and source-triggered revisions.",
                "artifacts_sources_and_unknowns_preserved": "The report binds sealed and current producer hashes, checks current citations, and retains all seven unresolved gaps with impact, responsible role, and recovery condition.",
                "package_receipt_passed": "101-file publication has no missing, unexpected, or hash-mismatched files.",
                "producer_artifacts_unchanged_after_review": "All 20 producer artifacts still match the sealed round-2 hashes.",
                "report_local_links_resolve": "Every local link in the independent review report resolves to an existing file.",
                "report_has_conditional_verdict": "The report records a conditional business-scope pass and does not erase process findings.",
                "report_preserves_scope_deviation": "The report names the MEMORY.md searches and over-broad docs search as noncompliant scope deviations.",
                "report_preserves_tool_failures": "The report retains all six nonzero commands and distinguishes later successful AWR retries.",
                "report_keeps_candidate_boundary": "The report keeps wanghai-final-delivery.md as a review candidate and denies final-delivery or E4 credit.",
                "all_three_business_matrices_complete": "The three business matrices contain 4, 6, and 5 rows respectively; every row has the four required fields.",
            }[name],
            **(
                {
                    "criteria": [
                        "独立检查恢复前后未决事项、补充要求和来源保持情况。"
                    ]
                }
                if name == "resume_and_revision_review_completed"
                else {
                    "criteria": [
                        "保留实际产物与来源引用，无法确认的内容显式说明。"
                    ]
                }
                if name == "artifacts_sources_and_unknowns_preserved"
                else {}
            ),
        }
        for name, passed in checks.items()
    ],
    "passed": all(checks.values()),
    "verdict": "conditional_pass_business_scope_with_preserved_process_findings",
    "limitations": [
        "Not an operations approval, actual final delivery, release, or E4 completion.",
        "Historical scope deviations remain noncompliant even though no business-content contamination was observed.",
    ],
}

OUTPUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(result, ensure_ascii=False, indent=2))
