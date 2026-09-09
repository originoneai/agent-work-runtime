#!/bin/sh
set -eu

project="/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/parallel-ownership/project"
awr="/Users/mac/Documents/originone/agent-work-running/target/release/awr"
owner_session="01M21WYFWXQHZNK0BJTYVD8N1K"

agenda_total="$(rtk proxy awk -F, 'NR > 1 { sum += $2 } END { print sum }' "$project/materials/agenda.csv")"
content_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-content.md" | rtk proxy awk '{print $1}')"
venue_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-venue.md" | rtk proxy awk '{print $1}')"
owner_checkpoint="$(rtk proxy "$awr" session show --project "$project" "$owner_session" --json | rtk proxy jq -r '.session.last_checkpoint_id // ""')"

rtk proxy test "$agenda_total" = "85"
rtk proxy test "$content_sha" = "42a801e823f1f0e510e054fa48f6e8a35e93f1874ddae1eadda09e87284eaaf3"
rtk proxy test "$venue_sha" = "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"
rtk proxy test "$owner_checkpoint" = "01M221RY7F976FXCBXCGYFRVBH"

rtk rg -q '^## 内容侧已形成回复，待现场接收确认$' "$project/deliverables/xingqiao-content.md"
rtk rg -q '目前没有证据表明它们已实际发送给现场负责人' "$project/deliverables/xingqiao-content.md"
if rtk rg -q '^## 已回传现场执行者$' "$project/deliverables/xingqiao-content.md"; then
  rtk proxy printf '%s\n' 'misleading handoff heading remains'
  exit 1
fi

rtk rg -q '^## 第三轮接收确认跟进（内容负责人已回应）$' "$project/deliverables/xingqiao-collaboration-review.md"
rtk rg -q 'FOLLOW-01.*内容负责人已回应；实际发送仍未确认' "$project/deliverables/xingqiao-collaboration-review.md"
rtk rg -q 'FOLLOW-02.*未确认接收' "$project/deliverables/xingqiao-collaboration-review.md"
rtk rg -q 'FOLLOW-06.*已回应并处理措辞问题' "$project/deliverables/xingqiao-collaboration-review.md"
rtk rg -q '关闭只表示内容负责人已回应并处理措辞问题' "$project/deliverables/xingqiao-collaboration-review.md"

rtk proxy printf '%s\n' "proofread2 followup verification passed; content_sha=$content_sha venue_sha=$venue_sha owner_checkpoint=$owner_checkpoint"
