#!/bin/sh
set -eu

agenda_total="$(rtk proxy awk -F, 'NR > 1 { sum += $2 } END { print sum }' materials/agenda.csv)"
rtk proxy test "$agenda_total" = "85"

rtk rg -q '场地开放九十分钟' materials/venue.md
rtk rg -q '布置和收场另需预留十分钟' materials/venue.md
rtk rg -q '仅有一台投影设备' materials/venue.md
rtk rg -q '分组讨论需要三组桌面' materials/venue.md

rtk rg -q '执行者 A：活动内容安排' deliverables/xingqiao-ownership.md
rtk rg -q '执行者 B：现场保障' deliverables/xingqiao-ownership.md
rtk rg -q '需要执行者 B 配合' deliverables/xingqiao-ownership.md
rtk rg -q '需要执行者 A 配合' deliverables/xingqiao-ownership.md
rtk rg -q '没有说明布置与收场各占多少' deliverables/xingqiao-ownership.md
rtk rg -q '当前未发生' deliverables/xingqiao-ownership.md

rtk proxy printf '%s\n' "XQ-INPUT verification passed; agenda_total_minutes=$agenda_total"
