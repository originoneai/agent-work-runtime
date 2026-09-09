#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path


PROJECT = Path(__file__).resolve().parents[1]
RUN = PROJECT.parent
CONTROL = RUN / "control"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_publication(name: str, require_exact: bool) -> dict:
    publication_path = CONTROL / name
    publication = json.loads(publication_path.read_text())
    target = PROJECT / publication["target"]
    expected = publication["files_sha256"]
    missing = []
    mismatched = []
    for relative, expected_hash in expected.items():
        path = target / relative
        if not path.is_file():
            missing.append(relative)
        elif sha256(path) != expected_hash:
            mismatched.append(relative)
    actual = {
        path.relative_to(target).as_posix()
        for path in target.rglob("*")
        if path.is_file()
    }
    extras = sorted(actual - set(expected)) if require_exact else []
    return {
        "publication": name,
        "publication_sha256": sha256(publication_path),
        "target": publication["target"],
        "declared_files": len(expected),
        "actual_files": len(actual),
        "missing": missing,
        "mismatched": mismatched,
        "extras": extras,
        "listed_hashes_verified": not missing and not mismatched,
        "exact_snapshot_verified": require_exact and not missing and not mismatched and not extras,
    }


correction_publication = json.loads(
    (CONTROL / "review-correction-publication.json").read_text()
)
current_hashes = {
    relative: sha256(PROJECT / relative)
    for relative in (
        "work-ledger.yaml",
        "deliverables/xingqiao-independent-results.md",
        "deliverables/xingqiao-content.md",
        "deliverables/xingqiao-venue.md",
        "deliverables/xingqiao-ownership.md",
        "deliverables/xingqiao-collaboration-review.md",
        "deliverables/xq-independent-review.md",
        "materials/agenda.csv",
        "materials/venue.md",
    )
}
expected_current = {
    relative: correction_publication["files_sha256"][
        "sources/" + relative if relative == "work-ledger.yaml" else relative
    ]
    for relative in current_hashes
    if relative not in ("materials/agenda.csv", "materials/venue.md")
}

ownership = (PROJECT / "deliverables/xingqiao-ownership.md").read_text()
merged = (PROJECT / "deliverables/xingqiao-independent-results.md").read_text()
ledger = (PROJECT / "work-ledger.yaml").read_text()

checks = {
    "correction_seal_sha_matches_operator": sha256(
        CONTROL / "review-correction-sealed.json"
    ) == "54e8014758354247bb33f122be129d09eb3ea14242cea9ae0f2415fa63e25ef0",
    "correction_publication_sha_matches_operator": sha256(
        CONTROL / "review-correction-publication.json"
    ) == "6c4a15d910a83eb44ee1967c6b6c25f17f4c5c32ebab54e8e4878f3fda5380d5",
    "current_correction_sources_and_artifacts_match": all(
        current_hashes[path] == expected for path, expected in expected_current.items()
    ),
    "xq_r01_closed_in_current_ownership": all(
        text in ownership
        for text in (
            "已在 project revision 165 结束",
            "当前无活动认领",
            "内容安排的已完成不等于现场可行性或内容成品已经齐备",
        )
    ),
    "xq_r02_all_completed": ledger.count('\"status\":\"completed\"') == 4,
    "xq_r02_repeated_completion_language_removed": all(
        text in ledger
        for text in (
            "不再重复执行本工作",
            "现行内容安排提交独立返检",
            "原现场执行者不再继续处理",
            "本工作及本轮整改均已完成",
        )
    ),
    "review_and_delivery_gate_current": all(
        text in ledger
        for text in (
            "首次独立复核已完成并提出 XQ-R01/XQ-R02",
            "等待不同独立参与者返检通过并再次完成 XQ-REVIEW",
            "最终交付受 XQ-REVIEW 独立返检门禁阻断",
        )
    ),
    "unknowns_and_responsibilities_preserved": all(
        text in merged
        for text in (
            "额外 10 分钟是否位于 90 分钟之外",
            "投影实机状态",
            "参会人数",
            "成品材料",
            "后续现场核验人",
        )
    ),
    "historical_conflicts_distinguished": all(
        (PROJECT / path).is_file()
        for path in (
            ".work-receipts/xq-venue-progress-conflict-r29.json",
            ".work-receipts/018-work-progress-xq-content-handoff-wording-conflict.json",
        )
    ),
    "first_review_preserved": current_hashes[
        "deliverables/xq-independent-review.md"
    ] == "c4dd2d34080ba24bfdb3d8408990c55b2152c33241ab259b86e4d034335c9c13",
    "final_delivery_absent": not (PROJECT / "deliverables/xq-delivery.md").exists(),
}

result = {
    "kind": "independent_recheck_verification",
    "publications": [
        verify_publication("review-correction-publication.json", True),
        verify_publication("review-input-publication.json", False),
        verify_publication("parallel-process-review-publication.json", True),
    ],
    "current_hashes_before_recheck_source_completion": current_hashes,
    "expected_current_hashes": expected_current,
    "checks": checks,
}
result["passed"] = (
    all(checks.values())
    and result["publications"][0]["exact_snapshot_verified"]
    and result["publications"][1]["listed_hashes_verified"]
    and result["publications"][2]["exact_snapshot_verified"]
)
print(json.dumps(result, ensure_ascii=False, indent=2))
