#!/bin/sh
set -eu

agenda_total="$(rtk proxy awk -F, 'NR > 1 { sum += $2 } END { print sum }' materials/agenda.csv)"
content_sha="$(rtk proxy shasum -a 256 deliverables/xingqiao-content.md | rtk proxy awk '{print $1}')"
venue_sha="$(rtk proxy shasum -a 256 deliverables/xingqiao-venue.md | rtk proxy awk '{print $1}')"

rtk proxy test "$agenda_total" = "85"
rtk proxy test "$content_sha" = "36c1427fadcba2c7c7749a0cf98e1e0f7ed8d725faa5c298ad1aa8d1edf5e462"
rtk proxy test "$venue_sha" = "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"
rtk proxy test -f .work-receipts/014-work-progress-xq-content-after-venue.json

rtk rg -q '第二轮增量快照（当前）' deliverables/xingqiao-collaboration-review.md
rtk rg -q '内容与现场回执在计划层面已对齐' deliverables/xingqiao-collaboration-review.md
rtk rg -q 'R2\.2-07' deliverables/xingqiao-collaboration-review.md
rtk rg -q 'COORD-01.*已处理' deliverables/xingqiao-collaboration-review.md
rtk rg -q 'COORD-06' deliverables/xingqiao-collaboration-review.md
rtk rg -q '已回传现场执行者.*尚缺接收证据' deliverables/xingqiao-collaboration-review.md
rtk rg -q '不修改原内容稿' deliverables/xingqiao-collaboration-review.md
rtk rg -q '正式独立复核未发生' deliverables/xingqiao-collaboration-review.md

rtk proxy printf '%s\n' "proofread2-r2 verification passed; agenda_total_minutes=$agenda_total content_sha=$content_sha venue_sha=$venue_sha"
