#!/bin/sh
set -eu

project="/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/parallel-ownership/project"
venue_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-venue.md" | rtk proxy awk '{print $1}')"
content_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-content.md" | rtk proxy awk '{print $1}')"

rtk proxy test "$venue_sha" = "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"
rtk proxy test "$content_sha" = "7fc9fc339952ecccfa71b2602810e0657191247c7b64346c071f5ae4a75d8eff"

rtk rg -q '^## 现场保障接续者实际接收记录（2026-09-09）$' "$project/deliverables/xingqiao-ownership.md"
rtk rg -q 'AWR 接续会话：`01M22688HDB0K4K691RF9CB8TC`' "$project/deliverables/xingqiao-ownership.md"
rtk rg -q '接续者已从项目当前共享交接稿逐项读取并确认收到 `C→V-01` 至 `C→V-04`' "$project/deliverables/xingqiao-ownership.md"
rtk rg -q '当前没有计划层面的设备占用冲突' "$project/deliverables/xingqiao-ownership.md"
rtk rg -q '人数、桌面容量、场地尺寸、视线和安全通道尚未核验' "$project/deliverables/xingqiao-ownership.md"
rtk rg -q '不是实机或场地验收' "$project/deliverables/xingqiao-ownership.md"
rtk rg -q '`XQ-MERGE` 与正式 `XQ-REVIEW` 不提前启动' "$project/deliverables/xingqiao-ownership.md"

rtk proxy printf '%s\n' "venue successor verification passed; venue_sha=$venue_sha content_sha=$content_sha"
