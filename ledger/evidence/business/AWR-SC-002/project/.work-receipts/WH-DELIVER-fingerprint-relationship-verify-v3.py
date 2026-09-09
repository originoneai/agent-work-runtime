#!/usr/bin/env python3
import hashlib
import json
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
CURRENT = ROOT / "deliverables/wh-delivery-v2.md"
ORIGINAL = ROOT / "deliverables/wh-delivery.md"
MACHINE = ROOT / "deliverables/wh-delivery-package-manifest-v2.json"
REPORT = (
    (ROOT / sys.argv[1]).resolve()
    if len(sys.argv) > 1
    else ROOT / ".work-receipts/WH-DELIVER-fingerprint-relationship-verification-v3.json"
)
START_MARKER = "<!-- PACKAGE_FINGERPRINT_TABLE_START -->"
END_MARKER = "<!-- PACKAGE_FINGERPRINT_TABLE_END -->"
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
LINK_RE = re.compile(r"\[[^\]]+\]\(([^)]+)\)")


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def project_relative(path: pathlib.Path) -> str:
    resolved = path.resolve()
    if not resolved.is_relative_to(ROOT):
        raise ValueError(f"path escapes project: {resolved}")
    return str(resolved.relative_to(ROOT))


def parse_table(path: pathlib.Path, marked: bool):
    text = path.read_text(encoding="utf-8")
    if marked:
        if START_MARKER not in text or END_MARKER not in text:
            raise ValueError("authority table markers are missing")
        text = text.split(START_MARKER, 1)[1].split(END_MARKER, 1)[0]

    rows = []
    for line_number, line in enumerate(text.splitlines(), 1):
        if not line.lstrip().startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        hashes = [cell for cell in cells if SHA_RE.fullmatch(cell)]
        links = LINK_RE.findall(line)
        if marked:
            if not hashes and not links:
                continue
        elif not hashes or not links:
            continue
        if len(hashes) != 1 or len(links) != 1:
            raise ValueError(
                f"ambiguous fingerprint row at {path}:{line_number}: "
                f"hashes={len(hashes)} links={len(links)}"
            )
        target = links[0].split("#", 1)[0]
        resolved = (path.parent / target).resolve()
        relative = project_relative(resolved)
        row = {
            "line": line_number,
            "path": relative,
            "declared_sha256": hashes[0],
            "actual_sha256": sha256(resolved) if resolved.exists() else None,
        }
        if marked:
            if len(cells) != 5:
                raise ValueError(
                    f"current authority row has {len(cells)} columns at "
                    f"{path}:{line_number}"
                )
            row["category"] = cells[0]
            row["role"] = cells[1]
        rows.append(row)
    return rows


def all_local_links(path: pathlib.Path):
    results = []
    for target in LINK_RE.findall(path.read_text(encoding="utf-8")):
        if target.startswith(("http://", "https://", "#")):
            continue
        clean = target.split("#", 1)[0]
        resolved = (path.parent / clean).resolve()
        outside = not resolved.is_relative_to(ROOT)
        results.append(
            {
                "target": target,
                "resolved": str(resolved) if outside else str(resolved.relative_to(ROOT)),
                "outside_project": outside,
                "exists": resolved.exists(),
            }
        )
    return results


def main() -> int:
    errors = []
    report = json.loads(REPORT.read_text(encoding="utf-8"))
    machine = json.loads(MACHINE.read_text(encoding="utf-8"))

    entries = machine["entries"]
    machine_by_path = {}
    for entry in entries:
        path = entry["path"]
        if path in machine_by_path:
            errors.append({"check": "machine_unique_path", "path": path})
            continue
        machine_by_path[path] = entry
        if not SHA_RE.fullmatch(entry["sha256"]):
            errors.append({"check": "machine_sha_format", "path": path})
            continue
        target = ROOT / path
        actual = sha256(target) if target.exists() else None
        if actual != entry["sha256"]:
            errors.append(
                {
                    "check": "machine_file_hash",
                    "path": path,
                    "declared": entry["sha256"],
                    "actual": actual,
                }
            )
    if machine["entry_count"] != len(entries):
        errors.append(
            {
                "check": "machine_entry_count",
                "declared": machine["entry_count"],
                "actual": len(entries),
            }
        )

    current_rows = parse_table(CURRENT, marked=True)
    current_by_path = {}
    duplicates = []
    for row in current_rows:
        if row["path"] in current_by_path:
            duplicates.append(row["path"])
        current_by_path[row["path"]] = row
        if row["actual_sha256"] != row["declared_sha256"]:
            errors.append(
                {
                    "check": "current_declared_file_hash",
                    "path": row["path"],
                    "declared": row["declared_sha256"],
                    "actual": row["actual_sha256"],
                }
            )
    if duplicates:
        errors.append({"check": "current_unique_path", "duplicates": duplicates})

    machine_path = project_relative(MACHINE)
    expected_current_paths = set(machine_by_path) | {machine_path}
    actual_current_paths = set(current_by_path)
    if expected_current_paths != actual_current_paths:
        errors.append(
            {
                "check": "human_machine_path_coverage",
                "missing_from_human": sorted(expected_current_paths - actual_current_paths),
                "unexpected_in_human": sorted(actual_current_paths - expected_current_paths),
            }
        )

    for path, entry in machine_by_path.items():
        row = current_by_path.get(path)
        if row is None:
            continue
        if (
            row["category"] != entry["category"]
            or row["role"] != entry["role"]
            or row["declared_sha256"] != entry["sha256"]
        ):
            errors.append(
                {
                    "check": "human_machine_relationship",
                    "path": path,
                    "human": {
                        "category": row["category"],
                        "role": row["role"],
                        "sha256": row["declared_sha256"],
                    },
                    "machine": {
                        "category": entry["category"],
                        "role": entry["role"],
                        "sha256": entry["sha256"],
                    },
                }
            )

    envelope = current_by_path.get(machine_path)
    if envelope is None or (
        envelope["category"] != "envelope"
        or envelope["role"] != "machine_package_manifest"
        or envelope["declared_sha256"] != sha256(MACHINE)
    ):
        errors.append(
            {
                "check": "machine_manifest_envelope",
                "actual": envelope,
                "expected_sha256": sha256(MACHINE),
            }
        )

    original_rows = parse_table(ORIGINAL, marked=False)
    original_mismatches = [
        {
            "path": row["path"],
            "declared_sha256": row["declared_sha256"],
            "actual_sha256": row["actual_sha256"],
        }
        for row in original_rows
        if row["declared_sha256"] != row["actual_sha256"]
    ]
    expected_original_mismatches = report["negative_control"]["detected_mismatches"]
    if original_mismatches != expected_original_mismatches:
        errors.append(
            {
                "check": "negative_control",
                "expected": expected_original_mismatches,
                "actual": original_mismatches,
            }
        )
    if len(original_rows) != report["negative_control"]["linked_fingerprint_rows_checked"]:
        errors.append(
            {
                "check": "negative_control_row_count",
                "expected": report["negative_control"]["linked_fingerprint_rows_checked"],
                "actual": len(original_rows),
            }
        )

    links = all_local_links(CURRENT)
    broken_links = [link for link in links if not link["exists"]]
    outside_links = [link for link in links if link["outside_project"]]
    if broken_links or outside_links or len(links) != report["local_links"]["count"]:
        errors.append(
            {
                "check": "current_local_links",
                "expected_count": report["local_links"]["count"],
                "actual_count": len(links),
                "broken": broken_links,
                "outside_project": outside_links,
            }
        )

    for path, expected in report["preserved_artifacts"].items():
        target = ROOT / path
        actual = sha256(target) if target.exists() else None
        if actual != expected:
            errors.append(
                {
                    "check": "preserved_artifact",
                    "path": path,
                    "expected": expected,
                    "actual": actual,
                }
            )

    current_text = CURRENT.read_text(encoding="utf-8")
    missing_gaps = [
        f"GAP-{index:02d}"
        for index in range(1, 8)
        if f"GAP-{index:02d}" not in current_text
    ]
    if missing_gaps:
        errors.append({"check": "business_gap_disclosure", "missing": missing_gaps})

    boundary_terms = ["不是生效手册", "不构成业务批准", "实际操作", "E4"]
    missing_boundary_terms = [
        term for term in boundary_terms if term not in current_text
    ]
    if missing_boundary_terms:
        errors.append(
            {"check": "delivery_boundary", "missing": missing_boundary_terms}
        )

    if sha256(CURRENT) != report["artifact"]["sha256"]:
        errors.append(
            {
                "check": "report_current_manifest_binding",
                "expected": report["artifact"]["sha256"],
                "actual": sha256(CURRENT),
            }
        )
    if sha256(MACHINE) != report["machine_manifest"]["sha256"]:
        errors.append(
            {
                "check": "report_machine_manifest_binding",
                "expected": report["machine_manifest"]["sha256"],
                "actual": sha256(MACHINE),
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

    result = {
        "ok": not errors,
        "report": str(REPORT.relative_to(ROOT)),
        "current_manifest_sha256": sha256(CURRENT),
        "machine_manifest_sha256": sha256(MACHINE),
        "machine_entries_checked": len(entries),
        "machine_unique_paths": len(machine_by_path),
        "current_authority_rows_checked": len(current_rows),
        "current_manifest_mismatches": [
            error
            for error in errors
            if error["check"].startswith("current_")
            or error["check"].startswith("human_machine")
            or error["check"] == "machine_manifest_envelope"
        ],
        "original_linked_fingerprint_rows_checked": len(original_rows),
        "original_mismatches_detected": original_mismatches,
        "current_local_links_checked": len(links),
        "current_broken_links": broken_links,
        "seven_business_gaps_disclosed": not missing_gaps,
        "errors": errors,
        "evidence_tier": "local_document_fingerprint_relationship_verification",
        "boundary": "document consistency repair only; no business approval, real operation, effective release, production acceptance, or E4",
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
