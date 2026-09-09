#!/usr/bin/env python3
import hashlib
import json
import re
import time
from pathlib import Path


REPO = Path("/Users/mac/Documents/originone/agent-work-running")
PROJECT = REPO / ".local/business-live-20260909-v1/source-change/project"
PUBLICATION = PROJECT.parent / "control/review-correction-publication.json"
PACKAGE = PROJECT / "review-inputs/corrections/review-correction"
OUTPUT = PROJECT / ".work-receipts/re-review-package-verification.json"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


publication = json.loads(PUBLICATION.read_text())
declared = publication["files_sha256"]
package_files = {}
for rel, expected in sorted(declared.items()):
    path = PACKAGE / rel
    actual = sha256(path) if path.is_file() else None
    package_files[rel] = {
        "expected_sha256": expected,
        "actual_sha256": actual,
        "bytes": path.stat().st_size if path.is_file() else None,
        "match": actual == expected,
    }

producer_artifacts = {
    path: {
        "before_sha256": before,
        "after_expected_sha256": declared[path],
        "after_actual_sha256": sha256(PROJECT / path),
    }
    for path, before in {
        "deliverables/songguo-source-versions.md": "b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04",
        "deliverables/songguo-change-impact.md": "6792b22621c6364fd3802b5a7e5c9552710f418df917ffdac819dcb0d5fe3beb",
        "deliverables/songguo-revised-plan.md": "58e419ab7ff87b2566fc156f3256b01e444987be7e8fae6dc6ad727027b0ca37",
    }.items()
}
producer_artifacts["deliverables/sg-review-response.md"] = {
    "before_sha256": None,
    "after_expected_sha256": declared["deliverables/sg-review-response.md"],
    "after_actual_sha256": sha256(PROJECT / "deliverables/sg-review-response.md"),
}
for item in producer_artifacts.values():
    item["after_matches_publication"] = (
        item["after_actual_sha256"] == item["after_expected_sha256"]
    )

historical = {
    "review-inputs/initial/deliverables/songguo-source-versions.md": "b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04",
    "review-inputs/round-1/deliverables/songguo-source-versions.md": "b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04",
    "review-inputs/round-2/deliverables/songguo-source-versions.md": "b026c37190ea1f5dddc2a30a0e7aab3e4e6577c541843e4cfe61395ed7f78d04",
    "review-inputs/round-1/deliverables/songguo-change-impact.md": "6792b22621c6364fd3802b5a7e5c9552710f418df917ffdac819dcb0d5fe3beb",
    "review-inputs/round-2/deliverables/songguo-change-impact.md": "6792b22621c6364fd3802b5a7e5c9552710f418df917ffdac819dcb0d5fe3beb",
    "review-inputs/round-2/deliverables/songguo-revised-plan.md": "58e419ab7ff87b2566fc156f3256b01e444987be7e8fae6dc6ad727027b0ca37",
}
historical_checks = {
    rel: {
        "expected_sha256": expected,
        "actual_sha256": sha256(PROJECT / rel),
        "match": sha256(PROJECT / rel) == expected,
    }
    for rel, expected in historical.items()
}

immutable = {
    "deliverables/sg-independent-review.md": "735fe7cff0bc7357b9db30d2ef79f82fc9822932a546811b4a2403ef98fa7ee7",
    "review-inputs/reference-lookup-process-record.json": "b2f75186aee39d65367c6ce7aca7b710a12710e43a942debdefeb88d917062c5",
}
immutable_checks = {
    rel: {
        "expected_sha256": expected,
        "actual_sha256": sha256(PROJECT / rel),
        "match": sha256(PROJECT / rel) == expected,
    }
    for rel, expected in immutable.items()
}

receipt_paths = sorted(
    rel for rel in declared if rel.startswith(".work-receipts/")
)
current_receipts = {}
for rel in receipt_paths:
    current = PROJECT / rel
    actual = sha256(current) if current.is_file() else None
    current_receipts[rel] = {
        "published_sha256": declared[rel],
        "current_sha256": actual,
        "unchanged_since_publication": actual == declared[rel],
    }

receipt_json_parse_errors = []
receipt_json_count = 0
for rel in receipt_paths:
    if not rel.endswith(".json"):
        continue
    receipt_json_count += 1
    try:
        json.loads((PROJECT / rel).read_text())
    except Exception as error:
        receipt_json_parse_errors.append({"path": rel, "error": str(error)})

sealed_execution = json.loads(
    (PACKAGE / "review-correction-sealed.json").read_text()
)

producer_docs = [
    "deliverables/songguo-source-versions.md",
    "deliverables/songguo-change-impact.md",
    "deliverables/songguo-revised-plan.md",
    "deliverables/sg-review-response.md",
]
bodies = {rel: (PROJECT / rel).read_text() for rel in producer_docs}
missing_links = []
expected_absent_links = []
resolved_links = []
for rel, body in bodies.items():
    for target in re.findall(r"\]\(([^)]+)\)", body):
        clean = target.split("#", 1)[0]
        if not clean or clean.startswith(("http://", "https://")):
            continue
        resolved = (PROJECT / rel).parent.joinpath(clean).resolve()
        record = {"document": rel, "target": target, "resolved": str(resolved)}
        if clean == "sg-delivery.md":
            record["absent"] = not resolved.exists()
            expected_absent_links.append(record)
        elif resolved.exists():
            resolved_links.append(record)
        else:
            missing_links.append(record)

process_path = PROJECT / "review-inputs/reference-lookup-process-record.json"
process = json.loads(process_path.read_text())
commands = process["commands"]
command_ids = [item["id"] for item in commands]
business_keywords = [
    "溪桥",
    "云岭",
    "南园",
    "松果",
    "伙伴接入",
    "source-change",
    "AWR-SC-007",
]
keyword_hits = {
    item["id"]: {
        keyword: item.get("aggregated_output", "").count(keyword)
        for keyword in business_keywords
    }
    for item in commands
}
initial_agents = (
    PROJECT / "review-inputs/initial/sources/AGENTS.md"
).read_text()
round1_agents = (
    PROJECT / "review-inputs/round-1/sources/AGENTS.md"
).read_text()
r02 = {
    "record_sha256": sha256(process_path),
    "command_ids": command_ids,
    "allowed_at_time": ["item_46"],
    "scope_deviations_at_time": ["item_61", "item_63", "item_64", "item_66"],
    "item_46_initially_allowlisted": (
        "/docs/integrations/codex.md" in initial_agents
    ),
    "cli_mcp_contract_only_later_allowlisted": (
        "/docs/reference/cli-mcp-contract.md" not in initial_agents
        and "/docs/reference/cli-mcp-contract.md" in round1_agents
    ),
    "examples_and_crates_not_later_allowlisted": (
        "Do not read the parent repository's `ledger/`, `tests/`, source code, examples"
        in round1_agents
    ),
    "business_keyword_hits_in_retained_outputs": keyword_hits,
    "all_business_keyword_hits_zero": all(
        count == 0
        for item_hits in keyword_hits.values()
        for count in item_hits.values()
    ),
    "independent_assessment_field_retained": process.get("independent_assessment"),
    "e4_credit": False,
}

forbidden_current_phrases = [
    "当前窗口支持两家伙伴",
    "当前窗口的伙伴资料核对数量是两家",
    "现有权威源没有指定两家",
    "保留两家名单",
]
required_response_ids = [
    "R-01.1",
    "R-01.2",
    "R-01.3",
    "R-01.4",
    "R-01.5",
    "R-02.1",
    "R-02.2",
    "R-02.3",
]
checks = {
    "publication_declares_103_files": len(declared) == 103,
    "all_103_package_files_match": all(item["match"] for item in package_files.values()),
    "all_published_execution_receipts_unchanged": all(
        item["unchanged_since_publication"] for item in current_receipts.values()
    ),
    "all_published_json_receipts_parse": not receipt_json_parse_errors,
    "sealed_execution_counts_retained": (
        sealed_execution.get("command_executions") == 70
        and sealed_execution.get("nonzero_commands") == 4
        and sealed_execution.get("e4_completion_claim") is False
    ),
    "producer_artifacts_match_publication": all(
        item["after_matches_publication"] for item in producer_artifacts.values()
    ),
    "sealed_historical_artifacts_unchanged": all(
        item["match"] for item in historical_checks.values()
    ),
    "original_review_and_process_record_unchanged": all(
        item["match"] for item in immutable_checks.values()
    ),
    "four_layer_chain_present": all(
        phrase in bodies["deliverables/songguo-source-versions.md"]
        for phrase in [
            "原草案三家",
            "首次生效规则两家",
            "追加现行规则一家",
            "协调会确认溪桥进入本轮",
            "现行执行安排",
        ]
    ),
    "no_forbidden_old_current_phrases": all(
        phrase not in body
        for phrase in forbidden_current_phrases
        for body in bodies.values()
    ),
    "current_one_partner_xiqiao_scope_present": (
        "当前窗口一家" in bodies["deliverables/songguo-source-versions.md"]
        and "溪桥进入本轮核对" in bodies["deliverables/songguo-revised-plan.md"]
        and "云岭补联系人职责说明" in bodies["deliverables/songguo-source-versions.md"]
        and "南园先提交资料目录" in bodies["deliverables/songguo-source-versions.md"]
    ),
    "all_external_docs_retain_unconfirmed_date": all(
        "接入开放日期尚未确认" in body for body in bodies.values()
    ),
    "response_addresses_all_findings": all(
        finding in bodies["deliverables/sg-review-response.md"]
        for finding in required_response_ids
    ),
    "r02_classification_retained": (
        command_ids == ["item_46", "item_61", "item_63", "item_64", "item_66"]
        and r02["item_46_initially_allowlisted"]
        and r02["cli_mcp_contract_only_later_allowlisted"]
        and r02["examples_and_crates_not_later_allowlisted"]
        and r02["all_business_keyword_hits_zero"]
        and "不能用于申领 E4" in bodies["deliverables/sg-review-response.md"]
    ),
    "cross_references_resolve": not missing_links,
    "future_final_delivery_absent": (
        not (PROJECT / "deliverables/sg-delivery.md").exists()
        and bool(expected_absent_links)
        and all(item["absent"] for item in expected_absent_links)
    ),
    "unresolved_facts_preserved": all(
        phrase in bodies["deliverables/sg-review-response.md"]
        for phrase in [
            "开放日期",
            "字段说明正文",
            "云岭职责说明",
            "南园目录",
            "具体执行人",
            "接入批准",
            "真实业务验收",
            "更早权威版本",
            "批准链",
        ]
    ),
}

result = {
    "kind": "independent_re_review_package_verification",
    "reviewer": {
        "canonical_task": "/root/restricted_material_client_run",
        "participant_type": "luna_worker",
        "awr_session_id": "01M2202ANNA5WFK3Q9DZ015B7A",
        "awr_claim_id": "01M2203KGQ2881PWDRPWTH2SX1",
    },
    "verified_at": int(time.time() * 1000),
    "publication": {
        "path": str(PUBLICATION),
        "sha256": sha256(PUBLICATION),
        "published_at": publication["published_at"],
        "actual_client_task_id": publication["actual_client_task_id"],
        "declared_file_count": len(declared),
        "private_native_transcript_published": publication[
            "private_native_transcript_published"
        ],
        "independent_verdict": publication["independent_verdict"],
        "e4_credit": publication["e4_credit"],
    },
    "package_files": package_files,
    "published_execution_receipt_count": len(receipt_paths),
    "published_execution_receipts": current_receipts,
    "published_receipt_json_parse": {
        "json_file_count": receipt_json_count,
        "parse_errors": receipt_json_parse_errors,
    },
    "sealed_execution_counts": {
        "command_executions": sealed_execution.get("command_executions"),
        "nonzero_commands": sealed_execution.get("nonzero_commands"),
        "tool_event_counts": sealed_execution.get("tool_event_counts"),
        "e4_completion_claim": sealed_execution.get("e4_completion_claim"),
        "note": (
            "Counts are preserved from the public sealed summary. The private "
            "native transcript was not published or inspected in this re-review."
        ),
    },
    "producer_artifacts_before_after": producer_artifacts,
    "sealed_historical_artifacts": historical_checks,
    "immutable_review_records": immutable_checks,
    "cross_references": {
        "resolved_count": len(resolved_links),
        "resolved": resolved_links,
        "expected_absent_future_links": expected_absent_links,
        "missing": missing_links,
    },
    "reference_lookup_assessment": r02,
    "checks": checks,
    "ok": all(checks.values()),
    "evidence_boundary": (
        "Independent re-review of the sealed correction package and current "
        "producer documents only; not final delivery, partner onboarding approval, "
        "material verification, real business acceptance, or E4."
    ),
}
OUTPUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({
    "ok": result["ok"],
    "publication_sha256": result["publication"]["sha256"],
    "declared_file_count": len(declared),
    "published_execution_receipt_count": len(receipt_paths),
    "failed_checks": [name for name, passed in checks.items() if not passed],
    "output": str(OUTPUT),
}, ensure_ascii=False, indent=2))
raise SystemExit(0 if result["ok"] else 1)
