#!/bin/sh
set -eu

agenda_total="$(rtk proxy awk -F, 'NR > 1 { sum += $2 } END { print sum }' materials/agenda.csv)"
content_sha="$(rtk proxy shasum -a 256 deliverables/xingqiao-content.md | rtk proxy awk '{print $1}')"
venue_sha="$(rtk proxy shasum -a 256 deliverables/xingqiao-venue.md | rtk proxy awk '{print $1}')"

rtk proxy test "$agenda_total" = "85"
rtk proxy test "$content_sha" = "a26b032ba3c1171b3056d46a4a2aab3d1d0a81167c7bfb31abe6645082cccd20"
rtk proxy test "$venue_sha" = "3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"

rtk rg -q 'xq-content-reviewer-2' deliverables/xingqiao-collaboration-review.md
rtk rg -q '不是 `XQ-REVIEW` 正式独立复核结论' deliverables/xingqiao-collaboration-review.md
rtk rg -q '基础内容一致；现场回执尚未被内容稿消费' deliverables/xingqiao-collaboration-review.md
rtk rg -q '至少短缺 5 分钟' deliverables/xingqiao-collaboration-review.md
rtk rg -q '逻辑预留已回执' deliverables/xingqiao-collaboration-review.md
rtk rg -q 'COORD-01' deliverables/xingqiao-collaboration-review.md
rtk rg -q 'COORD-05' deliverables/xingqiao-collaboration-review.md
rtk rg -q '本校对者只记录，不覆盖其产物' deliverables/xingqiao-collaboration-review.md
rtk rg -q '正式复核未发生' deliverables/xingqiao-collaboration-review.md

rtk proxy printf '%s\n' "proofread2 verification passed; agenda_total_minutes=$agenda_total content_sha=$content_sha venue_sha=$venue_sha"
