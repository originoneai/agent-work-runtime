#!/bin/sh
set -eu

rtk proxy test -s deliverables/zhusheng-interruption-diagnosis.md
rtk proxy git diff --quiet HEAD -- materials/handover-draft.md materials/change-requests.csv
rtk proxy rg -q 'materials/handover-draft\.md' deliverables/zhusheng-interruption-diagnosis.md
rtk proxy rg -q 'materials/change-requests\.csv' deliverables/zhusheng-interruption-diagnosis.md
rtk proxy rg -q '按内容类型分目录' deliverables/zhusheng-interruption-diagnosis.md
rtk proxy rg -q '按值班与资料分工' deliverables/zhusheng-interruption-diagnosis.md
rtk proxy rg -q '保留影响和下一步' deliverables/zhusheng-interruption-diagnosis.md
rtk proxy rg -q '不能据此断言' deliverables/zhusheng-interruption-diagnosis.md

if rtk proxy rg -q '按内容类型分目录|按值班与资料分工|保留影响和下一步' work-ledger.yaml; then
  exit 1
fi

rtk proxy printf '%s\n' 'ZS-DIAG local verification passed'
