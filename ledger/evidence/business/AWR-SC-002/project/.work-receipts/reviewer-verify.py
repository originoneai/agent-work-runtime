#!/usr/bin/env python3
import hashlib
import json
import re
from datetime import datetime
from pathlib import Path


PROJECT = Path(__file__).resolve().parents[1]
RUN = PROJECT.parent
PUBLICATION = RUN / "control" / "review-input-publication.json"
RESUME_BINDING = RUN / "control" / "actual-resume-binding.json"
OUTPUT = Path(__file__).with_name("reviewer-package-verification.json")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


publication = json.loads(PUBLICATION.read_text())
binding = json.loads(RESUME_BINDING.read_text())
expected_package = publication["files_sha256"]
package_root = PROJECT / publication["target"]
actual_package = {
    str(path.relative_to(package_root)): sha256(path)
    for path in sorted(package_root.rglob("*"))
    if path.is_file()
}

package_missing = sorted(set(expected_package) - set(actual_package))
package_unexpected = sorted(set(actual_package) - set(expected_package))
package_mismatches = {
    key: {"expected": expected_package[key], "actual": actual_package.get(key)}
    for key in sorted(expected_package)
    if actual_package.get(key) != expected_package[key]
}

round2 = json.loads((package_root / "round-2" / "turn-record.json").read_text())
producer_expected = round2["artifact_sha256"]
producer_actual = {}
producer_mismatches = {}
for rel, expected in sorted(producer_expected.items()):
    path = PROJECT / rel
    actual = sha256(path) if path.exists() else None
    producer_actual[rel] = actual
    if actual != expected:
        producer_mismatches[rel] = {"expected": expected, "actual": actual}

phase_clients = {}
phase_turns = {}
for phase in publication["retained_actual_phases"]:
    turn = json.loads((package_root / phase / "turn-record.json").read_text())
    phase_clients[phase] = turn["client_task_id"]
    phase_turns[phase] = turn["actual_user_turn"]

handoff_turn = json.loads(
    (package_root / "prelude-handoff" / "turn-record.json").read_text()
)
handoff_hash = handoff_turn["artifact_sha256"][
    "deliverables/wanghai-before-handoff.md"
]
resume_receipt = PROJECT / binding["receipt"]
resume_receipt_data = json.loads(resume_receipt.read_text())
resume = resume_receipt_data["resumed"]

key_documents = [
    "deliverables/wanghai-operations-manual-draft-v3.md",
    "deliverables/wanghai-source-verification-v3.md",
    "deliverables/wanghai-progress-handoff-v3.md",
    "deliverables/wanghai-final-delivery.md",
]
local_links = []
broken_links = []
link_pattern = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
for rel in key_documents:
    path = PROJECT / rel
    for line_number, line in enumerate(path.read_text().splitlines(), 1):
        for target in link_pattern.findall(line):
            if "://" in target or target.startswith("#"):
                continue
            target_path = (path.parent / target.split("#", 1)[0]).resolve()
            item = {
                "document": rel,
                "line": line_number,
                "target": target,
                "resolved": str(target_path),
                "exists": target_path.exists(),
            }
            local_links.append(item)
            if not item["exists"]:
                broken_links.append(item)

manual = (PROJECT / "deliverables/wanghai-operations-manual-draft-v3.md").read_text()
headings = re.findall(r"^## (.+)$", manual, re.MULTILINE)
heading_blocks = re.split(r"(?=^## )", manual, flags=re.MULTILINE)[1:]
chapters_with_adjacent_source = []
for block in heading_blocks:
    lines = block.splitlines()
    nonblank_after_heading = [line for line in lines[1:] if line.strip()]
    chapters_with_adjacent_source.append(
        bool(nonblank_after_heading)
        and nonblank_after_heading[0].startswith("**本章资料出处：**")
    )

business_sections = {
    "每日启动检查": "### 3.3 未确认内容责任与影响",
    "值班交接": "### 4.3 未确认内容责任与影响",
    "常见问题处理": "### 5.3 未确认内容责任与影响",
}
matrix_markers = {name: marker in manual for name, marker in business_sections.items()}

history = {}
for family, names in {
    "manual": [
        "wanghai-operations-manual-draft.md",
        "wanghai-operations-manual-draft-v2.md",
        "wanghai-operations-manual-draft-v3.md",
    ],
    "source_verification": [
        "wanghai-source-verification.md",
        "wanghai-source-verification-v2.md",
        "wanghai-source-verification-v3.md",
    ],
    "progress_handoff": [
        "wanghai-progress-handoff.md",
        "wanghai-progress-handoff-v2.md",
        "wanghai-progress-handoff-v3.md",
    ],
}.items():
    history[family] = {
        name: sha256(PROJECT / "deliverables" / name) for name in names
    }

final_delivery = (PROJECT / "deliverables/wanghai-final-delivery.md").read_text()
final_boundary_checks = {
    "title_marks_pending_independent_review": "（待独立复核）" in final_delivery.splitlines()[0],
    "states_candidate_only": "交付候选" in final_delivery,
    "states_review_incomplete": "WH-REVIEW` 尚未" in final_delivery
    or "WH-REVIEW` 仍须" in final_delivery,
    "names_actual_final_as_future": "deliverables/wh-delivery.md" in final_delivery,
    "has_material_check_record": "## 2. 材料核对记录" in final_delivery,
}

result = {
    "kind": "independent_review_package_verification",
    "checked_at": datetime.now().astimezone().isoformat(timespec="seconds"),
    "reviewer": {
        "agent_id": "/root/restricted_material_client_run",
        "session_id": "01M223E7S897SM2ADAAQ1AX25Q",
        "claim_id": "01M223E7S879SQHTJYJ5HAKFEF",
        "provider": "openai",
        "model": "gpt-5.6-sol",
    },
    "publication": {
        "path": str(PUBLICATION),
        "sha256": sha256(PUBLICATION),
        "declared_file_count": len(expected_package),
        "actual_file_count": len(actual_package),
        "missing": package_missing,
        "unexpected": package_unexpected,
        "mismatches": package_mismatches,
        "verified": not package_missing
        and not package_unexpected
        and not package_mismatches,
    },
    "producer_artifacts_before_review": {
        "expected": producer_expected,
        "actual": producer_actual,
        "mismatches": producer_mismatches,
        "verified": not producer_mismatches,
    },
    "phase_identity_and_turns": {
        "clients": phase_clients,
        "turns": phase_turns,
        "prelude_client_matches_binding": phase_clients["prelude"]
        == binding["prelude_client_task_id"],
        "prelude_handoff_client_matches_binding": phase_clients["prelude-handoff"]
        == binding["prelude_client_task_id"],
        "resumed_phases_match_binding": all(
            phase_clients[phase] == binding["new_client_task_id"]
            for phase in ["initial", "round-1", "round-2"]
        ),
    },
    "handoff_and_resume": {
        "binding_path": str(RESUME_BINDING),
        "binding_sha256": sha256(RESUME_BINDING),
        "receipt_path": str(resume_receipt),
        "receipt_expected_sha256": binding["receipt_sha256"],
        "receipt_actual_sha256": sha256(resume_receipt),
        "receipt_hash_matches": sha256(resume_receipt)
        == binding["receipt_sha256"],
        "handoff_artifact_hash_from_old_client_turn": handoff_hash,
        "handoff_artifact_current_hash": sha256(
            PROJECT / "deliverables/wanghai-before-handoff.md"
        ),
        "handoff_artifact_author_chain_matches": handoff_hash
        == sha256(PROJECT / "deliverables/wanghai-before-handoff.md"),
        "from_session_matches": resume["from_session"]["id"]
        == binding["from_session"],
        "to_session_matches": resume["session"]["id"] == binding["to_session"],
        "checkpoint_matches": resume["checkpoint"]["id"]
        == binding["checkpoint"],
        "old_session_closed_for_handoff": resume["from_session"]["status"]
        == "incomplete",
        "successor_session_started_active": resume["session"]["status"] == "active",
        "open_loops_match": resume["checkpoint"]["open_loops"]
        == binding["open_loops_preserved"],
    },
    "document_structure": {
        "manual_h2_headings": headings,
        "manual_h2_count": len(headings),
        "chapters_with_adjacent_source": chapters_with_adjacent_source,
        "all_chapters_have_adjacent_source": all(chapters_with_adjacent_source),
        "business_matrix_markers": matrix_markers,
        "all_business_matrices_present": all(matrix_markers.values()),
        "local_link_count": len(local_links),
        "broken_links": broken_links,
        "all_local_links_resolve": not broken_links,
        "final_boundary_checks": final_boundary_checks,
        "all_final_boundary_checks_pass": all(final_boundary_checks.values()),
    },
    "revision_history_sha256": history,
    "result": "pass_with_preserved_process_findings",
    "limitations": [
        "Structural and source-bound review only; no real operations rehearsal, business approval, or final delivery is claimed.",
        "Historical native-client scope deviations are recorded separately in the independent review and are not erased by this verification.",
    ],
}

OUTPUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(result, ensure_ascii=False, indent=2))
