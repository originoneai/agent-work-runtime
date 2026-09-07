#!/usr/bin/env python3
"""Validate the planning contract and ledger; this is not runtime acceptance."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import Counter
from pathlib import Path

import yaml


class UniqueLoader(yaml.SafeLoader):
    pass


def unique_mapping(loader, node, deep=False):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in result:
            raise ValueError(f"duplicate YAML key: {key}")
        result[key] = loader.construct_object(value_node, deep=deep)
    return result


UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping)


def load_json(path):
    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result
    return json.loads(path.read_text(), object_pairs_hook=pairs)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def local_file(root, relative):
    path = (root / relative).resolve()
    require(path.is_relative_to(root), f"path escapes repository: {relative}")
    require(path.is_file(), f"missing evidence/source file: {relative}")
    return path


def by_id(items, label):
    result = {item["id"]: item for item in items}
    require(len(result) == len(items), f"duplicate {label} ID")
    return result


def read_evidence(root, references, repository):
    require(references, "completed entry has no evidence")
    result = []
    for reference in references:
        evidence = load_json(local_file(root, reference))
        require(evidence.get("repository") == repository, f"evidence repository: {reference}")
        sha = evidence.get("source_commit", "")
        require(re.fullmatch(r"[0-9a-f]{40}", sha), f"invalid source commit: {reference}")
        receipt = evidence.get("remote_receipt", {})
        require(receipt.get("commit") == sha and receipt.get("verified_at"),
                f"missing matching remote receipt: {reference}")
        require(receipt.get("url") == f"{repository}/commit/{sha}",
                f"invalid remote commit URL: {reference}")
        require(evidence.get("checked_at") and evidence.get("checks"),
                f"missing concrete checks: {reference}")
        for check in evidence["checks"]:
            require(check.get("name") and check.get("passed") is True and check.get("result"),
                    f"failed or empty check: {reference}")
        for artifact in evidence.get("artifacts", []):
            local_file(root, artifact)
        require(evidence.get("artifacts"), f"no artifacts: {reference}")
        result.append(evidence)
    return result


def validate(root):
    root = root.resolve()
    contract = load_json(root / "contracts/awr-v1.json")
    ledger = yaml.load((root / "ledger/work-ledger.yaml").read_text(), Loader=UniqueLoader)
    require(ledger["contract_id"] == contract["contract_id"], "contract ID mismatch")
    require(ledger["contract_version"] == contract["version"], "contract version mismatch")
    require(isinstance(ledger["ledger_revision"], int) and ledger["ledger_revision"] > 0,
            "ledger revision must be positive")
    original = local_file(root, contract["authority"]["original_design"])
    require(hashlib.sha256(original.read_bytes()).hexdigest()
            == contract["authority"]["original_design_sha256"], "original design fingerprint changed")
    for field in ("intent", "plan", "rules", "status"):
        local_file(root, contract["authority"][field])

    items = by_id(ledger["work_items"], "work item")
    required = set(contract["scope"]["required_work_item_ids"])
    prep = set(contract["scope"]["preparation_work_item_ids"])
    targets = contract["scope"]["targets"]
    require(not required & prep and set(items) == required | prep, "work scope differs from contract")
    require(len(required) == targets["v1_work_items"] and len(prep) == targets["preparation_work_items"],
            "work target count mismatch")
    require(len(required) == len(contract["scope"]["required_work_item_ids"]), "duplicate V1 scope ID")
    require(len(prep) == len(contract["scope"]["preparation_work_item_ids"]), "duplicate preparation ID")
    milestones = by_id(contract["milestones"], "milestone")
    milestone_states = by_id(ledger["milestones"], "milestone state")
    require(set(milestones) == set(milestone_states), "milestone scope mismatch")
    statuses = set(contract["allowed_statuses"])
    repository = contract["product"]["repository"]
    require(ledger["project"]["repository"] == repository, "project repository mismatch")
    require(ledger["project"]["authority_mode"] == "source_first", "authority mode mismatch")
    require(ledger["project"]["release_status"] in ("not_released", "release_candidate", "released"),
            "unknown release status")

    for key, item in items.items():
        require(item["status"] in statuses, f"{key}: unknown status")
        require(item["required_for_v1"] is (key in required), f"{key}: incorrect V1 counting flag")
        require(item["milestone"] in milestones, f"{key}: unknown milestone")
        require(item["priority"] == milestones[item["milestone"]]["priority"], f"{key}: priority mismatch")
        for field in ("title", "deliverables", "acceptance", "source_sections", "next_action"):
            require(item.get(field), f"{key}: missing {field}")
        require(all(isinstance(x, str) and x.strip() for x in item["acceptance"]),
                f"{key}: empty acceptance criterion")
        require(len(set(item["acceptance"])) == len(item["acceptance"]), f"{key}: duplicate acceptance")
        deps = item["depends_on"]
        require(len(deps) == len(set(deps)) and set(deps) <= set(items), f"{key}: invalid dependency")
        require(item["verification"]["evidence_level"] in contract["evidence_levels"],
                f"{key}: invalid evidence level")
        if item["status"] in ("ready", "claimed", "in_progress", "completed"):
            require(all(items[d]["status"] == "completed" for d in deps), f"{key}: dependencies incomplete")
            require(not item["blocker"], f"{key}: active blocker")
        if item["status"] in ("claimed", "in_progress"):
            require(item["owner"], f"{key}: no owner")
        if item["status"] == "blocked":
            require(item["blocker"], f"{key}: blocked without reason")
        if item["status"] == "completed":
            require(item["verification"]["evidence_level"] in contract["evidence_levels"][3:],
                    f"{key}: completed without validation")
            evidence = read_evidence(root, item["evidence"], repository)
            records = [e["work_items"][key] for e in evidence if key in e.get("work_items", {})]
            require(records, f"{key}: evidence does not cover item")
            passed = {a["criterion"] for r in records for a in r["acceptance"] if a.get("passed") is True}
            require(set(item["acceptance"]) <= passed, f"{key}: acceptance evidence incomplete")
            allowed = {"implementation", "validation", "release"} if key in required else {"preparation"}
            require(all(e.get("kind") in allowed for e in evidence), f"{key}: wrong evidence tier")

    visiting, done = set(), set()
    def visit(key):
        require(key not in visiting, f"dependency cycle at {key}")
        if key in done:
            return
        visiting.add(key)
        for dep in items[key]["depends_on"]:
            visit(dep)
        visiting.remove(key)
        done.add(key)
    for key in items:
        visit(key)

    for key, milestone in milestone_states.items():
        states = [w["status"] for w in items.values() if w["milestone"] == key]
        expected = "completed" if all(s == "completed" for s in states) else (
            "in_progress" if any(s in ("completed", "claimed", "in_progress") for s in states) else "planned")
        require(milestone["status"] == expected, f"{key}: milestone status should be {expected}")
    coverage = {row["section"]: row["work_item_ids"] for row in contract["design_coverage"]}
    require(len(contract["design_coverage"]) == 58 and set(coverage) == set(range(58)),
            "design coverage must include each section 0-57 once")
    for section, refs in coverage.items():
        expected = {w["id"] for w in items.values() if section in w["source_sections"]}
        require(refs and len(refs) == len(set(refs)) and set(refs) == expected,
                f"section {section}: coverage mismatch")
    for key, item in items.items():
        require(set(item["source_sections"]) <= set(coverage), f"{key}: unknown source section")

    scenario_specs = by_id(contract["scenarios"], "scenario contract")
    scenarios = by_id(ledger["scenarios"], "scenario state")
    require(len(scenarios) == targets["business_scenarios"] and set(scenarios) == set(scenario_specs),
            "scenario scope mismatch")
    for field in ("fixture", "namespace", "work_graph", "artifact_directory"):
        require(len({s[field] for s in scenario_specs.values()}) == len(scenarios),
                f"scenario isolation failure: {field}")
    dimensions = {d for s in scenario_specs.values() for d in s["dimensions"]}
    require({"daily", "boundary", "exception", "recovery", "permission", "channel", "topology"} <= dimensions,
            "business coverage dimensions incomplete")
    scenario_commits = set()
    for key, state in scenarios.items():
        spec = scenario_specs[key]
        require(state["status"] in statuses, f"{key}: unknown scenario status")
        require(len(spec["followups"]) >= 2 and spec["expected_artifacts"], f"{key}: incomplete business plan")
        require(set(state["gates"]) == set(spec["required_gates"])
                == set(contract["completion"]["scenario_required_gates"]), f"{key}: gate scope mismatch")
        require(all(type(v) is bool for v in state["gates"].values()), f"{key}: gates must be boolean")
        if state["status"] != "completed":
            continue
        require(all(state["gates"].values()) and not state["blocker"], f"{key}: missing hard gate")
        require(state["executor"] and state["reviewer"] and state["executor"] != state["reviewer"],
                f"{key}: independent review missing")
        evidence = read_evidence(root, state["evidence"], repository)
        matching = [e for e in evidence if e.get("scenario_id") == key and e.get("kind") == "business_acceptance"]
        require(matching, f"{key}: no business acceptance evidence")
        for ev in matching:
            require(set(ev.get("gates", {})) == set(spec["required_gates"])
                    and all(v is True for v in ev["gates"].values()), f"{key}: evidence gates incomplete")
            require(ev.get("executor") == state["executor"] and ev.get("reviewer") == state["reviewer"],
                    f"{key}: reviewer binding mismatch")
        sha = state["delivery"]["source_commit"]
        require(sha in {e["source_commit"] for e in matching}, f"{key}: delivery mismatch")
        require(sha not in scenario_commits, f"{key}: reused scenario delivery commit")
        scenario_commits.add(sha)

    metric_specs = by_id(contract["metrics"], "metric contract")
    metrics = by_id(ledger["metrics"], "metric state")
    require(len(metrics) == targets["metrics"] and set(metrics) == set(metric_specs), "metric scope mismatch")
    compare = {"lt": lambda a,b: a < b, "lte": lambda a,b: a <= b,
               "gt": lambda a,b: a > b, "eq": lambda a,b: a == b}
    for key, state in metrics.items():
        require(state["status"] in ("planned", "measured", "passed", "failed", "blocked"),
                f"{key}: invalid metric status")
        if state["status"] == "passed":
            spec = metric_specs[key]
            require(type(state["value"]) in (float, int), f"{key}: value missing")
            require(compare[spec["comparison"]](state["value"], spec["target"]), f"{key}: threshold failed")
            evidence = read_evidence(root, state["evidence"], repository)
            require(any(e.get("metrics", {}).get(key) == state["value"] for e in evidence),
                    f"{key}: measurement evidence missing")
    current = ledger["current"]["work_item"]
    require(current in items and ledger["current"]["milestone"] == items[current]["milestone"],
            "current item/milestone mismatch")
    require(ledger["current"]["next_action"], "missing next action")
    if ledger["project"]["release_status"] == "released":
        require(all(items[k]["status"] == "completed" for k in required), "release work incomplete")
        require(all(s["status"] == "completed" for s in scenarios.values()), "release scenarios incomplete")
        require(all(m["status"] == "passed" for m in metrics.values()), "release metrics incomplete")
        read_evidence(root, ledger["project"].get("release_evidence", []), repository)
    return contract, ledger


def render(contract, ledger):
    items = ledger["work_items"]
    counts = Counter((w["required_for_v1"], w["status"]) for w in items)
    target = contract["scope"]["targets"]
    esc = lambda value: str(value).replace("|", "/").replace("\n", " ")
    lines = ["# AWR 工作台账索引", "", "<!-- Generated by scripts/check_ledger.py --render. Do not edit. -->", "",
             f"合同：{contract['contract_id']} {contract['version']}；台账 revision：{ledger['ledger_revision']}。",
             "状态权威：[work-ledger.yaml](work-ledger.yaml)。本页为派生视图。", "",
             f"- 筹备完成：{counts[(False, 'completed')]}/{target['preparation_work_items']}。",
             f"- V1 工作完成：{counts[(True, 'completed')]}/{target['v1_work_items']}。",
             f"- 完整业务场景：{sum(s['status']=='completed' for s in ledger['scenarios'])}/{target['business_scenarios']}。",
             f"- 达标指标：{sum(m['status']=='passed' for m in ledger['metrics'])}/{target['metrics']}。", "",
             f"当前：**{ledger['current']['work_item']}**。", "",
             f"下一步：{ledger['current']['next_action']}", "",
             "## 工作项", "", "| ID | 优先级 | 里程碑 | 状态 | 工作 | 依赖 |",
             "| --- | --- | --- | --- | --- | --- |"]
    for w in items:
        lines.append("| " + " | ".join(map(esc, [w["id"],w["priority"],w["milestone"],w["status"],
                                               w["title"],", ".join(w["depends_on"]) or "—"])) + " |")
    lines += ["", "## 完整业务场景", "", "| ID | 业务闭环 | 覆盖维度 | 状态 |", "| --- | --- | --- | --- |"]
    states = {s["id"]:s["status"] for s in ledger["scenarios"]}
    for s in contract["scenarios"]:
        lines.append(f"| {s['id']} | {s['title']} | {', '.join(s['dimensions'])} | {states[s['id']]} |")
    lines += ["", "场景定义、自然业务输入和硬门槛见合同；产物与复核结果以源台账证据为准。", "",
              "## 指标", "", "| 指标 | 目标 | 状态 |", "| --- | --- | --- |"]
    states = {m["id"]:m["status"] for m in ledger["metrics"]}
    for m in contract["metrics"]:
        lines.append(f"| {m['title']} | {m['comparison']} {m['target']} {m['unit']} | {states[m['id']]} |")
    lines += ["", "## 原始设计覆盖", "", "全部 58 个编号章节的映射由合同与每项 source_sections 交叉校验。",
              "覆盖表示已有实施安排，不表示功能已经实现。", ""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--render", action="store_true", help="regenerate the derived index")
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        contract, ledger = validate(root)
        index = root / "ledger/README.md"
        expected = render(contract, ledger)
        if args.render:
            index.write_text(expected)
        require(index.is_file() and index.read_text() == expected, "ledger index stale; run with --render")
    except (ValueError, KeyError, TypeError, OSError, yaml.YAMLError) as error:
        parser.exit(1, f"FAIL: {error}\n")
    v1 = [w for w in ledger["work_items"] if w["required_for_v1"]]
    print(f"PASS: {len(v1)} V1 items; 3 preparation items; 58 design sections; "
          f"{len(ledger['scenarios'])} business scenarios; {len(ledger['metrics'])} metrics.")
    print(f"V1 completed: {sum(w['status']=='completed' for w in v1)}/{len(v1)}. "
          "Planning validation only; no runtime/E4 claim.")


if __name__ == "__main__":
    main()
