#!/bin/sh
set -eu

project="/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/parallel-ownership/project"
ledger="$project/work-ledger.yaml"

hash_of() {
  rtk proxy shasum -a 256 "$1" | rtk proxy awk '{print $1}'
}

assert_hash() {
  actual="$(hash_of "$project/$1")"
  rtk proxy test "$actual" = "$2"
}

assert_work_field() {
  work_line="$(rtk rg -F "\"id\":\"$1\"" "$ledger")"
  rtk proxy printf '%s\n' "$work_line" | rtk rg -Fq "\"$2\":\"$3\""
}

agenda_total="$(rtk proxy awk -F, 'NR > 1 { sum += $2 } END { print sum }' "$project/materials/agenda.csv")"
rtk proxy test "$agenda_total" = "85"

# Current producer files and immutable pre-remediation/reference snapshots.
assert_hash "deliverables/xingqiao-content.md" "7fc0ccf93f8f8aa6b8d881c91c6f61dc32e63ed979f6cc120f5ba9086eb253c5"
assert_hash "deliverables/xingqiao-venue.md" "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"
assert_hash "deliverables/xingqiao-collaboration-review.md" "21e564a7fa985cb67a842e1e836ed84f9a69db2865c86171e0edef4df891d5c0"
assert_hash "deliverables/xq-independent-review.md" "c4dd2d34080ba24bfdb3d8408990c55b2152c33241ab259b86e4d034335c9c13"
assert_hash "deliverables/xingqiao-ownership.md" "d1238aea798cc63e1d190cb4cbae24ba0b415f786501e1221312d83dcfcceafc"
assert_hash "deliverables/xingqiao-independent-results.md" "b7ca7ce5bc545485832c64d6624607d0f16a875dedb067741e9eec189bff2021"
assert_hash "review-inputs/coordination-correction/deliverables/xingqiao-ownership.md" "2658295d0430fd4d63104c74e46e4f1c86a9b1d94c4e322d004c4932bcc0dbad"
assert_hash "review-inputs/coordination-correction/deliverables/xingqiao-independent-results.md" "6060f7ed92faa5cdb44a95838f0d3d465b9f67012ee31f1c2be8883c89aff9c1"
assert_hash ".work-receipts/work-ledger-before-xq-review-remediation.yaml" "6053d22cac099d54d50a6949e5be82e8326c574f8fb79e301a8395a98ed1dc47"
assert_hash ".work-receipts/xingqiao-content-before-successor-status-update.md" "7fc9fc339952ecccfa71b2602810e0657191247c7b64346c071f5ae4a75d8eff"

# Independent review, conflicts, rejected mutations and field-correction receipts stay intact.
assert_hash ".work-receipts/reviewer-completion-report.json" "701e78e14fd895ab32e4538171cdf515dea691b1263cc50864c9e57a2fce7296"
assert_hash ".work-receipts/reviewer-verification-attempt-1.json" "9db0eeae04db343f81366f937e3d90a690efb8309a1706bbfdf93371d2627a24"
assert_hash ".work-receipts/xq-venue-progress-conflict-r29.json" "b933a12b7d7575bf87bd54fd014c04df0ebc4e6dbef7d6131d8f9e1ca3d55181"
assert_hash ".work-receipts/018-work-progress-xq-content-handoff-wording-conflict.json" "d9d01e9a41934057f3a39dc1f871f6ce57751107a565267ba6b5b5ad61febbea"
assert_hash ".work-receipts/042-reopen-xq-merge-invalid-summary.json" "662b65d5f5a6df7dc4d8ac957801cefcc0ef82dd66c46d84b1556c6800e5b783"
assert_hash ".work-receipts/048-proposal-create-xq-input-session-conflict.json" "db92180164c30abc6350399b7b35d310b08dc462859d6e6a48b917204ac0be1a"
assert_hash ".work-receipts/049-proposal-show-xq-input-state-correction.json" "48ec8b4c1d415f1f8f56b2b186599eebd8b32b21fd67a9ef8953faefc47a7c5c"
assert_hash ".work-receipts/050-proposal-show-xq-content-state-correction.json" "f8d80e837919f6c2409911d32919a360e12986871e373d75cd58bcf461ec0b8e"
assert_hash ".work-receipts/051-proposal-show-xq-venue-state-correction.json" "d722b24a3b80d0d6ac21490d36af17a0d06076ead9569d8d4acd98daf850f608"

# Completed source work no longer instructs the operator to repeat completed execution.
assert_work_field "XQ-INPUT" "status" "completed"
assert_work_field "XQ-INPUT" "next_action" "本工作已完成；85/90/10 分钟、单投影和三组桌面约束继续作为返检与后续现场确认的来源，不再重复执行本工作。"
assert_work_field "XQ-CONTENT" "status" "completed"
assert_work_field "XQ-CONTENT" "next_action" "本工作已完成；现行内容安排提交独立返检。后续收到场地、设备或人员确认并触发内容变更时，由内容执行者或明确接续者补齐成品材料和受影响安排。"
assert_work_field "XQ-VENUE" "status" "completed"
assert_work_field "XQ-VENUE" "summary" "已形成并本地验证现场保障方案，明确 85/90/10 分钟边界、单投影与三组桌面条件及进场门槛；原执行会话已结束，实际场地和设备事实仍未确认。"
assert_work_field "XQ-VENUE" "next_action" "本工作已完成；原现场执行者不再继续处理。由协调者另行指派后续现场核验人，在活动日期、人数和内容材料明确后取得时间、场地、设备与现场人员回执，并把结果交给独立返检。"
assert_work_field "XQ-MERGE" "summary" "已完成首次汇总；本轮已按独立复核修正分工现行状态及 XQ-INPUT、XQ-CONTENT、XQ-VENUE、XQ-MERGE 的摘要或下一步，保留原稿、历次分工与失败记录并形成返检包。"
assert_work_field "XQ-MERGE" "next_action" "完成本轮整改验证并释放认领；由运营方另派独立参与者返检 XQ-R01/XQ-R02，返检再次完成 XQ-REVIEW 后方可认领 XQ-DELIVER。"
merge_line="$(rtk rg -F '"id":"XQ-MERGE"' "$ledger")"
rtk proxy printf '%s\n' "$merge_line" | rtk rg -q '"status":"(in_progress|completed)"'
assert_work_field "XQ-REVIEW" "status" "planned"
assert_work_field "XQ-REVIEW" "next_action" "等待 XQ-MERGE 完成 XQ-R01/XQ-R02 整改并提交固定版本；随后由不同独立参与者认领，逐项返检分工现行状态、四个 completed 工作项文字、原稿/失败保留和现场未知边界。"
deliver_block="$(rtk proxy awk 'BEGIN { found = 0 } /^- id: XQ-DELIVER$/ { found = 1 } found { print }' "$ledger")"
rtk proxy printf '%s\n' "$deliver_block" | rtk rg -q '^  status: planned$'

# XQ-R01 is corrected in current text; the stale sentence survives only inside a labelled historical receipt.
rtk rg -Fq 'AWR 会话 `01M21WYFWXQHZNK0BJTYVD8N1K` 已在 project revision 165 结束，当前无活动认领' "$project/deliverables/xingqiao-ownership.md"
if rtk rg -q 'AWR 会话仍为活动状态' "$project/deliverables/xingqiao-ownership.md"; then
  rtk proxy printf '%s\n' 'current ownership still claims an ended session is active' >&2
  exit 1
fi
rtk rg -Fq '现场执行者临时离岗后的接续安排（历史基线）' "$project/deliverables/xingqiao-ownership.md"
rtk rg -Fq '以下接收记录按发生时保留' "$project/deliverables/xingqiao-ownership.md"
rtk rg -Fq '`XQ-CONTENT` 继续保持进行中' "$project/deliverables/xingqiao-ownership.md"
rtk rg -Fq '现行状态与后续责任（独立复核整改）' "$project/deliverables/xingqiao-ownership.md"

# The handoff package answers every review defect and keeps unknown-site boundaries and the return-check gate.
rtk rg -Fq '| `XQ-R01`：分工记录把已结束的内容会话写成活动态 |' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -Fq '| `XQ-R02 / XQ-INPUT`：completed 仍要求验证并完成本工作 |' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -Fq '| `XQ-R02 / XQ-CONTENT`：completed 仍要求完成并启动合并 |' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -Fq '| `XQ-R02 / XQ-VENUE`：summary 仍写“正在形成”，next_action 仍要求完成 |' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -Fq '| `XQ-R02 / XQ-MERGE`：completed 仍要求再次完成本工作 |' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '^## 三、尚未闭合事项、后续角色与条件$' "$project/deliverables/xingqiao-independent-results.md"
for item in VEN-01 VEN-02 EQP-01 EQP-02 PPL-01 MAT-01 MAT-02 HDO-01 REV-01; do
  rtk rg -Fq "| $item |" "$project/deliverables/xingqiao-independent-results.md"
done
rtk rg -Fq '只有返检明确通过并再次完成 `XQ-REVIEW`，才可认领 `XQ-DELIVER`' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -Fq '最高证据层级仍是本地验证' "$project/deliverables/xingqiao-independent-results.md"

if rtk proxy test -e "$project/deliverables/xq-delivery.md"; then
  rtk proxy printf '%s\n' 'final delivery artifact exists before independent return-check passed' >&2
  exit 1
fi

rtk proxy printf '%s\n' "review remediation verification passed; agenda_total=$agenda_total ownership_sha=$(hash_of "$project/deliverables/xingqiao-ownership.md") merge_sha=$(hash_of "$project/deliverables/xingqiao-independent-results.md") return_check=pending final_delivery=blocked"
