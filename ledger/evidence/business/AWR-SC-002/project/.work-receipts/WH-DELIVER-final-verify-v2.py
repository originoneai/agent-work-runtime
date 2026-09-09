#!/usr/bin/env python3
import hashlib
import json
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
REPORT_PATH = ROOT / ".work-receipts/WH-DELIVER-final-verification-v2.json"
FINAL_DOCUMENTS = [
    ROOT / "deliverables/wh-delivery.md",
    ROOT / "deliverables/wanghai-final-work-record.md",
    ROOT / "deliverables/wanghai-final-handoff.md",
]
HISTORICAL_HASHES = {
    "deliverables/wanghai-before-handoff.md": "f4c58c9422f8452f3c0bd148859be94d2ecceebe670fe7c62c21401292773efc",
    "deliverables/wanghai-resumed-work.md": "c143afbe7e3c32d61c1e31587407cfb42b322b8fab483a29efea719996dd1576",
    "deliverables/wanghai-operations-manual-draft.md": "813bd164add4ed663223d083e56b76a9f57a2c372906236bcad713712c2b0545",
    "deliverables/wanghai-operations-manual-draft-v2.md": "16011e71b8d4bb2b0f588c93b325f07491b3d33bea4052c89ad6c751bbe65438",
    "deliverables/wanghai-operations-manual-draft-v3.md": "b09950015623422d6ae287dd6295abbd83b6ea47970f7da64835d3957f222e68",
    "deliverables/wanghai-source-verification.md": "9a34cdefb4a0aa684a70f7d79d958680f9512ca2a4b79ce1371536e95dcf2e19",
    "deliverables/wanghai-source-verification-v2.md": "9019b11cf6558543b2741a63870556a166dfc2daac8abebdf75f2427f8857892",
    "deliverables/wanghai-source-verification-v3.md": "267dae9476a283a3bdaeb2539a9912e1849294e22db5bb16d300ae3f5459dbba",
    "deliverables/wanghai-progress-handoff.md": "b4b606c662296ebd99c4d695395f4a0d26f73a773bc5c848749324945c70add6",
    "deliverables/wanghai-progress-handoff-v2.md": "9ed1028c36b4fa525424f79bb17aa61157384582961dd34ad7743ee9cd93d341",
    "deliverables/wanghai-progress-handoff-v3.md": "526fe9513c657c927ff71a6d887f353508612ab0b29b2afe32ba7f8d2feb3bff",
    "deliverables/wanghai-final-delivery.md": "ad31de837d2d7be3d2cb22fb6cc8acf2de55b3ddc7ae4cb1a1c5c4765bec48f0",
    "deliverables/wh-independent-review.md": "17d9efb27e766de37c1bc8ed84f1140f644f80238d0c23360beec55f549af602",
}


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_hash_map(name, expected, recorded_actual, errors):
    for relative, expected_hash in expected.items():
        path = ROOT / relative
        actual_hash = sha256(path) if path.exists() else None
        if actual_hash != expected_hash:
            errors.append(
                {
                    "check": name,
                    "path": relative,
                    "expected": expected_hash,
                    "actual": actual_hash,
                }
            )
        if recorded_actual.get(relative) != actual_hash:
            errors.append(
                {
                    "check": f"{name}_recorded_actual",
                    "path": relative,
                    "recorded": recorded_actual.get(relative),
                    "actual": actual_hash,
                }
            )


def main() -> int:
    report = json.loads(REPORT_PATH.read_text(encoding="utf-8"))
    errors = []
    verify_hash_map(
        "artifact_hash",
        report["artifacts"]["expected"],
        report["artifacts"]["actual"],
        errors,
    )
    verify_hash_map(
        "source_hash",
        report["authoritative_sources"]["expected"],
        report["authoritative_sources"]["actual"],
        errors,
    )
    verify_hash_map("historical_hash", HISTORICAL_HASHES, HISTORICAL_HASHES, errors)

    link_count = 0
    broken_links = []
    combined = []
    for source in FINAL_DOCUMENTS:
        text = source.read_text(encoding="utf-8")
        combined.append(text)
        for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", text):
            if target.startswith(("http://", "https://", "#")):
                continue
            link_count += 1
            clean = target.split("#", 1)[0]
            if not (source.parent / clean).resolve().exists():
                broken_links.append(
                    {
                        "source": str(source.relative_to(ROOT)),
                        "target": target,
                    }
                )
    if link_count != report["local_links"]["count"] or broken_links:
        errors.append(
            {
                "check": "local_links",
                "expected_count": report["local_links"]["count"],
                "actual_count": link_count,
                "broken": broken_links,
            }
        )

    combined_text = "\n".join(combined)
    missing_gaps = [
        f"GAP-{index:02d}"
        for index in range(1, 8)
        if f"GAP-{index:02d}" not in combined_text
    ]
    if missing_gaps:
        errors.append({"check": "gap_disclosure", "missing": missing_gaps})

    boundary_phrases = [
        "不是生效手册",
        "不构成业务批准",
        "不证明任何真实操作已执行",
        "E4",
    ]
    missing_boundaries = [
        phrase for phrase in boundary_phrases if phrase not in combined_text
    ]
    if missing_boundaries:
        errors.append(
            {"check": "delivery_boundary", "missing": missing_boundaries}
        )

    missing_failure_receipts = [
        relative
        for relative in report["preserved_process_findings"]["failure_receipts"]
        if not (ROOT / relative).exists()
    ]
    if missing_failure_receipts:
        errors.append(
            {
                "check": "failure_receipts",
                "missing": missing_failure_receipts,
            }
        )

    if not isinstance(report.get("checks"), list):
        errors.append({"check": "completion_report_contract", "checks_is_list": False})
    else:
        for index, check in enumerate(report["checks"]):
            if check.get("passed") is not True or not check.get("details"):
                errors.append(
                    {
                        "check": "completion_report_check",
                        "index": index,
                        "value": check,
                    }
                )

    required_criteria = {
        "处理复核意见，交付可继续追溯的手册及交接说明。",
        "保留实际产物与来源引用，无法确认的内容显式说明。",
    }
    mapped_criteria = {
        criterion
        for check in report["checks"]
        for criterion in check.get("criteria", [])
    }
    if not required_criteria.issubset(mapped_criteria):
        errors.append(
            {
                "check": "acceptance_mapping",
                "missing": sorted(required_criteria - mapped_criteria),
            }
        )

    empty_review_output = ROOT / ".work-receipts/reviewer-complete-r206.json"
    if empty_review_output.stat().st_size != 0:
        errors.append(
            {
                "check": "preserved_empty_review_output",
                "expected_size": 0,
                "actual_size": empty_review_output.stat().st_size,
            }
        )

    result = {
        "ok": not errors,
        "report": str(REPORT_PATH.relative_to(ROOT)),
        "report_contract_checks_is_sequence": isinstance(report.get("checks"), list),
        "declared_artifact_hashes": len(report["artifacts"]["expected"]),
        "declared_source_hashes": len(report["authoritative_sources"]["expected"]),
        "historical_hashes": len(HISTORICAL_HASHES),
        "links_checked": link_count,
        "seven_gaps_disclosed": not missing_gaps,
        "failure_receipts_present": not missing_failure_receipts,
        "errors": errors,
        "evidence_tier": "local_document_and_trace_verification",
        "boundary": "document delivery only; no business approval, real operation, effective release, or E4",
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
