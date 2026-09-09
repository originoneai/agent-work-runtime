#!/usr/bin/env python3
import hashlib
import json
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE_SHA = "cd44a161471cee94fd4f797955327a4fd995cf3a"
COMMAND = "rtk proxy python3 .work-receipts/qh-deliver-verify.py"

EXPECTED_HASHES = {
    "deliverables/qh-delivery.md": "2cbfd52859b4c5ca34cd803b18980aa2d0ec0505fe0403d9ed935ef4bd4d2fce",
    "deliverables/qinghe-brief.md": "cbfa8a49ab2b1f2ad071257dc85f6eac15e9d8783f7f379b6bf411f1a4e50576",
    "deliverables/qinghe-dependencies.md": "39be620608391f4e05a1161766cd586f4971eb10efa28318b3aa80fcccd56167",
    "deliverables/qinghe-context-reference.md": "60ab471924b12cbe1fd129b251c6e004c6b3fb0f2a5672a9bfcddffd3a2d3f17",
    "deliverables/qh-independent-review.md": "cfd81dd8bbc85608a1d6210d9f482dd300b71af8687fb3efc3538b3a8ac19de1",
    "deliverables/qh-review-response.md": "92bc899572eab62027c30ec143b75fd9c9c8971569888493a14e17e31d43398d",
    "deliverables/qh-independent-re-review.md": "adf4fc58abfc863a660913a912f5d57262bb4d6b0182cfdbe8ec16e0a5984d8d",
    "review-inputs/reference-lookup-process-record.json": "0aa782e30d7ac1f2f2b9ba65f9c2224a07404d3111ea5353f1ff0f3015722920",
    "GOALS.md": "575f650e4ff9b782bebdb8830f5b2670a3bdc447a41ff84e53eec7cf35c249a8",
    "PLAN.md": "2d726e0e77754a51f779b5eee59ac29c919a4e7ab87b2a43a53a74938b2ba053",
    "RULES.md": "05c7d1e94f96e3f6bdaccfd8790895a07e11472fe95542c046eb570456ad218d",
}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


checks = []


def check(name, passed, details, criteria=None):
    item = {"name": name, "passed": bool(passed), "details": details}
    if criteria:
        item["criteria"] = criteria
    checks.append(item)


actual_hashes = {}
for relative, expected in EXPECTED_HASHES.items():
    path = ROOT / relative
    actual = sha256(path) if path.is_file() else None
    actual_hashes[relative] = actual
    check(
        f"hash:{relative}",
        actual == expected,
        f"expected={expected}; actual={actual}",
    )

delivery = (ROOT / "deliverables/qh-delivery.md").read_text(encoding="utf-8")
brief = (ROOT / "deliverables/qinghe-brief.md").read_text(encoding="utf-8")
dependencies = (ROOT / "deliverables/qinghe-dependencies.md").read_text(encoding="utf-8")
context = (ROOT / "deliverables/qinghe-context-reference.md").read_text(encoding="utf-8")
ledger = (ROOT / "work-ledger.yaml").read_text(encoding="utf-8")

required_delivery_phrases = [
    "QH-R01",
    "QH-R02",
    "QH-R03",
    "QH-R04",
    "QH-R05",
    "项目中仍未取得三份说明的实际文件或链接",
    "实际内容目录尚未形成",
    "域名、旧版页面停用日期、切换条件和具体切换时间均未确认",
    "安排级整改通过",
    "不是“目录通过”“外部就绪”或“门户上线”",
    "locator-only Unknown",
    "显式登记采用调用者提交的绑定",
    "未观察到污染”不是“已证明无污染",
    "本次执行者按项目边界未访问该目录",
    "明确未决事项移交",
    "QH-FINAL-HANDOFF-20260909",
]
missing_phrases = [phrase for phrase in required_delivery_phrases if phrase not in delivery]
check(
    "delivery required conclusions and boundaries",
    not missing_phrases,
    f"missing={missing_phrases}",
)

for relative in (
    "deliverables/qinghe-brief.md",
    "deliverables/qinghe-dependencies.md",
    "deliverables/qinghe-context-reference.md",
    "deliverables/qh-independent-review.md",
    "deliverables/qh-review-response.md",
    "deliverables/qh-independent-re-review.md",
    "review-inputs/reference-lookup-process-record.json",
):
    expected = EXPECTED_HASHES[relative]
    check(
        f"manifest binds:{relative}",
        expected in delivery,
        f"expected hash present={expected in delivery}",
    )

check(
    "brief reflects independent re-review while keeping directory review pending",
    "五项安排级整改通过" in brief
    and "实际内容目录仍不存在" in brief
    and "尚未经过目录级独立复核" in brief,
    "arrangement re-review is closed; directory-level verification remains open",
)
check(
    "dependencies reflect current handoff boundary",
    "`QH-REVIEW` | completed" in dependencies
    and "实际目录形成后仍须另做目录级独立复核" in dependencies
    and "未决真实输入继续移交，不自动变为就绪" in dependencies,
    "review completed; external and directory gates remain separate",
)
check(
    "context distinguishes evidence identities",
    "Unknown 记录" in context
    and "显式验证记录" in context
    and "不能互相覆盖、合并" in context
    and "completed" in context
    and "自动提升" in context,
    "Unknown locator projection remains distinct from explicit evidence registration",
)
check(
    "context retains technical lookup deviation",
    "原复核和最新返检均判断该行为越过当时项目范围" in context
    and "未观察到青禾业务答案" in context
    and "不等于证明没有污染" in context,
    "historical process defect retained without being treated as a business source",
)

for receipt in (
    ".work-receipts/re-review-verification.json",
    ".work-receipts/re-review-evidence-add-v2.json",
    ".work-receipts/re-review-work-complete.json",
    ".work-receipts/re-review-post-close-verification.json",
    ".work-receipts/20260909-QH-DELIVER-session-start.json",
    ".work-receipts/20260909-QH-DELIVER-context-bootstrap.json",
    ".work-receipts/20260909-QH-DELIVER-context-compile.json",
    ".work-receipts/20260909-QH-DELIVER-artifact-add.json",
    ".work-receipts/20260909-QH-DELIVER-progress-final.json",
):
    try:
        json.loads((ROOT / receipt).read_text(encoding="utf-8"))
        valid = True
        details = "valid JSON"
    except Exception as error:
        valid = False
        details = str(error)
    check(f"receipt:{receipt}", valid, details)

re_review = json.loads((ROOT / ".work-receipts/re-review-verification.json").read_text())
re_review_post = json.loads(
    (ROOT / ".work-receipts/re-review-post-close-verification.json").read_text()
)
check(
    "independent re-review accepted only arrangement-level remediation",
    re_review.get("all_passed") is True
    and re_review.get("verdict") == "remediation_accepted_for_arrangement_level_handoff",
    f"all_passed={re_review.get('all_passed')}; verdict={re_review.get('verdict')}",
)
check(
    "independent re-review close-out is internally consistent",
    re_review_post.get("all_passed") is True
    and "QH-DELIVER remains planned" in re_review_post.get("caveats", [""])[0],
    f"all_passed={re_review_post.get('all_passed')}; final_project_revision={re_review_post.get('final_project_revision')}",
)

artifact = json.loads(
    (ROOT / ".work-receipts/20260909-QH-DELIVER-artifact-add.json").read_text()
)
check(
    "registered artifact matches final handoff",
    artifact.get("artifact", {}).get("sha256")
    == EXPECTED_HASHES["deliverables/qh-delivery.md"],
    f"artifact_id={artifact.get('artifact', {}).get('id')}; sha256={artifact.get('artifact', {}).get('sha256')}",
)

check(
    "work record has final progress and next action without external completion claims",
    '"id":"QH-DELIVER"' in ledger
    and '"status":"in_progress"' in ledger
    and "三份文档、标题、实际目录、域名、停用及切换/上线确认仍未取得" in ledger
    and "随后形成无个人联系人的目录并由不同参与者做目录级复核" in ledger,
    f"ledger_sha256={sha256(ROOT / 'work-ledger.yaml')}",
)

check(
    "acceptance: review findings handled and final briefing trace delivered",
    not missing_phrases
    and re_review.get("all_passed") is True
    and artifact.get("artifact", {}).get("sha256")
    == EXPECTED_HASHES["deliverables/qh-delivery.md"],
    "final handoff, brief, dependency, context, original review, remediation and re-review are present and bound",
    ["处理复核意见，交付最终简报与追溯记录。"],
)
check(
    "acceptance: actual artifacts and sources retained while unknowns remain explicit",
    all(actual_hashes[path] == expected for path, expected in EXPECTED_HASHES.items())
    and "项目中仍未取得三份说明的实际文件或链接" in delivery
    and "实际内容目录尚未形成" in delivery
    and "均未确认" in delivery
    and "Unknown" in delivery
    and "技术查找偏离历史" in delivery,
    "fixed artifacts and source hashes match; missing documents, titles, directory and external confirmations remain explicit",
    ["保留实际产物与来源引用，无法确认的内容显式说明。"],
)

git_sha = subprocess.run(
    ["git", "rev-parse", "HEAD"],
    cwd=ROOT,
    check=True,
    capture_output=True,
    text=True,
).stdout.strip()
check("git source SHA", git_sha == SOURCE_SHA, f"expected={SOURCE_SHA}; actual={git_sha}")

diff_check = subprocess.run(
    ["git", "diff", "--check"],
    cwd=ROOT,
    capture_output=True,
    text=True,
)
check(
    "git diff whitespace check",
    diff_check.returncode == 0,
    diff_check.stdout.strip() or diff_check.stderr.strip() or "clean",
)

all_passed = all(item["passed"] for item in checks)
report = {
    "version": 1,
    "work_item": "QH-DELIVER",
    "source_sha": SOURCE_SHA,
    "command": COMMAND,
    "verified_at": int(time.time() * 1000),
    "evidence_level": "locally_verified",
    "verdict": "final_handoff_package_locally_verified" if all_passed else "verification_failed",
    "all_passed": all_passed,
    "checks": checks,
    "artifacts_sha256": actual_hashes,
    "work_ledger_sha256_before_completion": sha256(ROOT / "work-ledger.yaml"),
    "independent_re_review": {
        "verdict": re_review.get("verdict"),
        "scope": "arrangement-level remediation only",
        "report": ".work-receipts/re-review-verification.json",
    },
    "scope": ["QH-DELIVER"],
    "limitations": [
        "This report verifies local package integrity, citations, work-record wording and retained history only.",
        "It does not verify the three missing documents, public titles, directory entries, personal-contact removal, domain, retirement date, cutover, external access or portal launch.",
        "It is executor-produced locally_verified evidence, not a new independent review or business acceptance.",
        "The authoritative ledger will change when QH-DELIVER is completed; final status is captured in separate completion and post-close receipts.",
    ],
}
print(json.dumps(report, ensure_ascii=False, indent=2))
sys.exit(0 if all_passed else 1)
