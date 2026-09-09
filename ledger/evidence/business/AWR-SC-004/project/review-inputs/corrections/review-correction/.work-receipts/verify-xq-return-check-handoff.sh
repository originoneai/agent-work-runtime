#!/bin/sh
set -eu

project="/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/parallel-ownership/project"
awr="/Users/mac/Documents/originone/agent-work-running/target/release/awr"
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

assert_hash "deliverables/xingqiao-content.md" "7fc0ccf93f8f8aa6b8d881c91c6f61dc32e63ed979f6cc120f5ba9086eb253c5"
assert_hash "deliverables/xingqiao-venue.md" "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"
assert_hash "deliverables/xingqiao-ownership.md" "d1238aea798cc63e1d190cb4cbae24ba0b415f786501e1221312d83dcfcceafc"
assert_hash "deliverables/xingqiao-independent-results.md" "681ebc8aa9988fe7d27d581eb4e76d68dc24dbacce49a59c5df17953311f8699"
assert_hash "deliverables/xq-independent-review.md" "c4dd2d34080ba24bfdb3d8408990c55b2152c33241ab259b86e4d034335c9c13"
assert_hash "review-inputs/coordination-correction/deliverables/xingqiao-ownership.md" "2658295d0430fd4d63104c74e46e4f1c86a9b1d94c4e322d004c4932bcc0dbad"
assert_hash "review-inputs/coordination-correction/deliverables/xingqiao-independent-results.md" "6060f7ed92faa5cdb44a95838f0d3d465b9f67012ee31f1c2be8883c89aff9c1"
assert_hash ".work-receipts/work-ledger-before-xq-review-remediation.yaml" "6053d22cac099d54d50a6949e5be82e8326c574f8fb79e301a8395a98ed1dc47"
assert_hash ".work-receipts/verify-xq-review-remediation-final.sh" "45a4e5dce6f82aca23a04ad3c12a9094e8ca4d8f7d064a48bdc487cf7e64e549"
assert_hash ".work-receipts/xq-review-remediation-final-evidence-report.json" "269bd0dde5599096930e014217533c67c751f8ad96a2fb0f9dbca41f09f37b11"
assert_hash ".work-receipts/reviewer-verification-attempt-1.json" "9db0eeae04db343f81366f937e3d90a690efb8309a1706bbfdf93371d2627a24"
assert_hash ".work-receipts/xq-venue-progress-conflict-r29.json" "b933a12b7d7575bf87bd54fd014c04df0ebc4e6dbef7d6131d8f9e1ca3d55181"
assert_hash ".work-receipts/018-work-progress-xq-content-handoff-wording-conflict.json" "d9d01e9a41934057f3a39dc1f871f6ce57751107a565267ba6b5b5ad61febbea"
assert_hash ".work-receipts/042-reopen-xq-merge-invalid-summary.json" "662b65d5f5a6df7dc4d8ac957801cefcc0ef82dd66c46d84b1556c6800e5b783"
assert_hash ".work-receipts/048-proposal-create-xq-input-session-conflict.json" "db92180164c30abc6350399b7b35d310b08dc462859d6e6a48b917204ac0be1a"
assert_hash ".work-receipts/053-review-remediation-verification-attempt-1.json" "835a706912c95d5c3f5627bd3c1bb4e78e5ee6c029a818e561afe76da39489d8"

assert_hash "work-ledger.yaml" "c571682e22d0a891f8d7072c8f4ce4575909463b974e9443fe72f062d3b316ae"
assert_hash ".work-receipts/069-proposal-create-xq-review-handoff-update.json" "00f0a0fbea0fa78777990d6c9f6836d7cf4e922f5a210a4f98a48ea094489ddc"
assert_hash ".work-receipts/072-proposal-apply-xq-review-handoff-update.json" "93bb4d6578e63068021d5f1f45679b665318eb7492cc74a67e6426515e6dd546"
assert_hash ".work-receipts/077-proposal-create-xq-deliver-gate-update.json" "73310d4ef8e09a73b5dec5c9816ca073683b30d17df387d260c645f06e74061a"
assert_hash ".work-receipts/080-proposal-apply-xq-deliver-gate-update.json" "79d6d6c3977c1c3a2b7a96bdffa2d3a82d9e2b773f6d59e164c511a0738272ab"

assert_work_field "XQ-INPUT" "status" "completed"
assert_work_field "XQ-INPUT" "next_action" "本工作已完成；85/90/10 分钟、单投影和三组桌面约束继续作为返检与后续现场确认的来源，不再重复执行本工作。"
assert_work_field "XQ-CONTENT" "status" "completed"
assert_work_field "XQ-CONTENT" "next_action" "本工作已完成；现行内容安排提交独立返检。后续收到场地、设备或人员确认并触发内容变更时，由内容执行者或明确接续者补齐成品材料和受影响安排。"
assert_work_field "XQ-VENUE" "status" "completed"
assert_work_field "XQ-VENUE" "summary" "已形成并本地验证现场保障方案，明确 85/90/10 分钟边界、单投影与三组桌面条件及进场门槛；原执行会话已结束，实际场地和设备事实仍未确认。"
assert_work_field "XQ-VENUE" "next_action" "本工作已完成；原现场执行者不再继续处理。由协调者另行指派后续现场核验人，在活动日期、人数和内容材料明确后取得时间、场地、设备与现场人员回执，并把结果交给独立返检。"
assert_work_field "XQ-MERGE" "status" "completed"
assert_work_field "XQ-MERGE" "summary" "已完成首次汇总；本轮已按独立复核修正分工现行状态及 XQ-INPUT、XQ-CONTENT、XQ-VENUE、XQ-MERGE 的摘要或下一步，保留原稿、历次分工与失败记录并形成返检包。"
assert_work_field "XQ-MERGE" "next_action" "本工作及本轮整改均已完成，认领已释放；固定整改版本已交回独立返检。由运营方另派不同独立参与者返检 XQ-R01/XQ-R02；仅在其再次完成 XQ-REVIEW 后才认领 XQ-DELIVER。"

review_line="$(rtk rg -F '"id":"XQ-REVIEW"' "$ledger")"
rtk proxy printf '%s\n' "$review_line" | rtk rg -Fq '"summary":"首次独立复核已完成并提出 XQ-R01/XQ-R02；执行者整改和最终本地复验均已完成，当前等待不同独立参与者返检。"'
rtk proxy printf '%s\n' "$review_line" | rtk rg -Fq '"next_action":"XQ-MERGE 整改固定版本已就绪；由运营方另派不同独立参与者认领本工作，逐项返检分工现行状态、四个 completed 工作项文字、原稿与失败保留及现场未知边界。返检通过后再次完成本工作；未通过则保留问题并退回整改。"'

deliver_line="$(rtk rg -F '"id":"XQ-DELIVER"' "$ledger")"
rtk proxy printf '%s\n' "$deliver_line" | rtk rg -Fq '"status":"planned"'
rtk proxy printf '%s\n' "$deliver_line" | rtk rg -Fq '"summary":"尚未开始；最终交付受 XQ-REVIEW 独立返检门禁阻断。"'
rtk proxy printf '%s\n' "$deliver_line" | rtk rg -Fq '"next_action":"等待不同独立参与者返检通过并再次完成 XQ-REVIEW；此前不认领、不生成最终交付。条件满足后，由最终交付角色核对返检回执、固定版本和未闭合事项，再整理活动筹备交付包。"'

rtk rg -Fq 'AWR 会话 `01M21WYFWXQHZNK0BJTYVD8N1K` 已在 project revision 165 结束，当前无活动认领' "$project/deliverables/xingqiao-ownership.md"
if rtk rg -q 'AWR 会话仍为活动状态' "$project/deliverables/xingqiao-ownership.md"; then
  rtk proxy printf '%s\n' 'current ownership still claims an ended session is active' >&2
  exit 1
fi
for item in VEN-01 VEN-02 EQP-01 EQP-02 PPL-01 MAT-01 MAT-02 HDO-01 REV-01; do
  rtk rg -Fq "| $item |" "$project/deliverables/xingqiao-independent-results.md"
done
rtk rg -Fq '只有返检明确通过并再次完成 `XQ-REVIEW`，才可认领 `XQ-DELIVER`' "$project/deliverables/xingqiao-independent-results.md"
if rtk proxy test -e "$project/deliverables/xq-delivery.md"; then
  rtk proxy printf '%s\n' 'final delivery artifact exists before independent return-check passed' >&2
  exit 1
fi

status_json="$(rtk proxy "$awr" --project "$project" --json status)"
rtk proxy printf '%s\n' "$status_json" | rtk rg -q '"current_total": 0'
rtk proxy printf '%s\n' "$status_json" | rtk rg -q '"ready_count": 1'
rtk proxy printf '%s\n' "$status_json" | rtk rg -q '"blocked_count": 1'
rtk proxy printf '%s\n' "$status_json" | rtk rg -Fq '"external_key": "XQ-REVIEW"'

for session in 01M22BK3JB8VGGGBC2AWHSHGT2 01M22DGDJ081585RHWQSGBA30N 01M22DZQE75MZE6XA856WS6P34 01M22E2T0MNSH5DCQKW43DR8M6; do
  session_json="$(rtk proxy "$awr" --project "$project" --json session show "$session")"
  rtk proxy printf '%s\n' "$session_json" | rtk rg -q '"status": "ended"'
done

rtk proxy printf '%s\n' 'return-check handoff verification passed; completed=4 review=ready delivery=blocked active_work=0 independent_return_check=pending final_delivery=not_started'
