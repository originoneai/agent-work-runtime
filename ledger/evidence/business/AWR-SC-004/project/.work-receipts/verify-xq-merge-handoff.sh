#!/bin/sh
set -eu

project="/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/parallel-ownership/project"
agenda_total="$(rtk proxy awk -F, 'NR > 1 { sum += $2 } END { print sum }' "$project/materials/agenda.csv")"
content_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-content.md" | rtk proxy awk '{print $1}')"
venue_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-venue.md" | rtk proxy awk '{print $1}')"
ownership_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-ownership.md" | rtk proxy awk '{print $1}')"
snapshot_sha="$(rtk proxy shasum -a 256 "$project/.work-receipts/xingqiao-content-before-successor-status-update.md" | rtk proxy awk '{print $1}')"
merge_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-independent-results.md" | rtk proxy awk '{print $1}')"

rtk proxy test "$agenda_total" = "85"
rtk proxy test "$content_sha" = "7fc0ccf93f8f8aa6b8d881c91c6f61dc32e63ed979f6cc120f5ba9086eb253c5"
rtk proxy test "$venue_sha" = "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"
rtk proxy test "$ownership_sha" = "2658295d0430fd4d63104c74e46e4f1c86a9b1d94c4e322d004c4932bcc0dbad"
rtk proxy test "$snapshot_sha" = "7fc9fc339952ecccfa71b2602810e0657191247c7b64346c071f5ae4a75d8eff"
rtk proxy test "$merge_sha" = "6060f7ed92faa5cdb44a95838f0d3d465b9f67012ee31f1c2be8883c89aff9c1"

rtk rg -q '现场接续者 `xq-venue-successor` 已通过项目共享交接稿确认收到 `C→V-01` 至 `C→V-04`' "$project/deliverables/xingqiao-content.md"
if rtk rg -q '接续岗位已在 .*实际接续者尚未建立 AWR 会话或确认接收' "$project/deliverables/xingqiao-content.md"; then
  rtk proxy printf '%s\n' 'stale successor status remains in current content artifact' >&2
  exit 1
fi

rtk rg -q '^# 星桥活动两项结果与可交接筹备方案$' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '实际交接链为：内容执行者形成四项内容到现场回复' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '至少短缺 5 分钟' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '计划层面没有设备占用冲突' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '^## 三、尚未闭合事项、后续角色与条件$' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '\| VEN-01 \| 场地/时间 \|' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '\| VEN-02 \| 场地/布局 \|' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '\| EQP-01 \| 设备/投影 \|' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '\| EQP-02 \| 设备/其他 \|' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '\| PPL-01 \| 人员 \|' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '\| MAT-01 \| 成品材料 \|' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '\| MAT-02 \| 现场物料 \|' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '\| REV-01 \| 独立复核 \|' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '启动条件.*关闭条件' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '^## 四、原稿、原回执和发现问题的保留$' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '修改前内容原稿快照' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '原现场检查点' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '`COORD-06`、`FOLLOW-01`—`FOLLOW-06`' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '不是 `xq-content-executor`、`xq-venue-executor`、`xq-venue-successor` 或 `xq-merge-coordinator`' "$project/deliverables/xingqiao-independent-results.md"
rtk rg -q '最高证据层级仍是本地验证' "$project/deliverables/xingqiao-independent-results.md"

if rtk proxy sh "$project/.work-receipts/verify-xq-venue.sh"; then
  rtk proxy printf '%s\n' 'historical venue checker unexpectedly matches the revised content artifact' >&2
  exit 1
fi

rtk proxy printf '%s\n' "merge handoff verification passed; agenda_total=$agenda_total content_sha=$content_sha venue_sha=$venue_sha merge_sha=$merge_sha historical_venue_checker=nonzero_as_recorded"
