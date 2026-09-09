#!/bin/sh
set -eu

agenda_total="$(rtk proxy awk -F, 'NR > 1 { sum += $2 } END { print sum }' materials/agenda.csv)"
rtk proxy test "$agenda_total" = "85"

rtk rg -q '场地开放九十分钟' materials/venue.md
rtk rg -q '布置和收场另需预留十分钟' materials/venue.md
rtk rg -q '仅有一台投影设备' materials/venue.md
rtk rg -q '分组讨论需要三组桌面' materials/venue.md

rtk proxy test -f deliverables/xingqiao-content.md
rtk rg -q '90 分钟开放窗口与额外 10 分钟' deliverables/xingqiao-content.md
rtk rg -q '内容侧将回传现场的事项' deliverables/xingqiao-content.md

rtk rg -q 'T\+10.*T\+35' deliverables/xingqiao-venue.md
rtk rg -q '至少存在 5 分钟缺口' deliverables/xingqiao-venue.md
rtk rg -q '三组桌面必须可用' deliverables/xingqiao-venue.md
rtk rg -q '活动前 7 分钟' deliverables/xingqiao-venue.md
rtk rg -q '活动后 3 分钟' deliverables/xingqiao-venue.md
rtk rg -q 'deliverables/xingqiao-content.md' deliverables/xingqiao-venue.md
rtk rg -q '待内容负责人回复' deliverables/xingqiao-venue.md
rtk rg -q '不改写.*XQ-CONTENT' deliverables/xingqiao-venue.md
rtk rg -q '不是独立复核结论' deliverables/xingqiao-venue.md
rtk rg -q '实际现场验证证据' deliverables/xingqiao-venue.md

rtk proxy printf '%s\n' "XQ-VENUE verification passed; agenda_total_minutes=$agenda_total"
