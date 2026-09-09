#!/bin/sh
set -eu

project="/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/parallel-ownership/project"
content_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-content.md" | rtk proxy awk '{print $1}')"
venue_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-venue.md" | rtk proxy awk '{print $1}')"
snapshot_sha="$(rtk proxy shasum -a 256 "$project/.work-receipts/xingqiao-content-before-successor-status-update.md" | rtk proxy awk '{print $1}')"

rtk proxy test "$content_sha" = "7fc0ccf93f8f8aa6b8d881c91c6f61dc32e63ed979f6cc120f5ba9086eb253c5"
rtk proxy test "$venue_sha" = "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"
rtk proxy test "$snapshot_sha" = "7fc9fc339952ecccfa71b2602810e0657191247c7b64346c071f5ae4a75d8eff"

rtk rg -q '现场接续者 `xq-venue-successor` 已通过项目共享交接稿确认收到 `C→V-01` 至 `C→V-04`' "$project/deliverables/xingqiao-content.md"
rtk rg -q 'AWR 会话 `01M22688HDB0K4K691RF9CB8TC`' "$project/deliverables/xingqiao-content.md"
rtk rg -q '^## 内容侧回复已由接续者确认项目内接收$' "$project/deliverables/xingqiao-content.md"
rtk rg -q '这里的“收到”仅指项目内接收' "$project/deliverables/xingqiao-content.md"
rtk rg -q '计划层面无投影占用冲突' "$project/deliverables/xingqiao-content.md"
rtk rg -q '场地、设备、人员、成品材料、独立复核与最终交付仍待闭合' "$project/deliverables/xingqiao-content.md"
rtk rg -q '协调者须另行指派后续现场核验人' "$project/deliverables/xingqiao-content.md"
rtk rg -q '外部聊天或发送凭证' "$project/deliverables/xingqiao-content.md"
rtk rg -q '独立复核尚未发生' "$project/deliverables/xingqiao-content.md"

if rtk rg -q '接续岗位已在 .*实际接续者尚未建立 AWR 会话或确认接收' "$project/deliverables/xingqiao-content.md"; then
  rtk proxy printf '%s\n' 'stale successor status remains in current content artifact' >&2
  exit 1
fi

rtk proxy printf '%s\n' "content handoff verification passed; content_sha=$content_sha venue_sha=$venue_sha snapshot_sha=$snapshot_sha"
