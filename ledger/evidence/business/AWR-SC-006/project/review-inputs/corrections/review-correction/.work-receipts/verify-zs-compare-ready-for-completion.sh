#!/bin/sh
set -eu

rtk proxy test -s deliverables/zhusheng-consistency.md
rtk proxy test -s deliverables/zhusheng-change-proposals.md
rtk proxy shasum -a 256 -c .work-receipts/preserved-handover-inputs.sha256

archive_hash=$(rtk proxy shasum -a 256 materials/archive-confirmation.md | rtk proxy awk '{print $1}')
rtk proxy test "$archive_hash" = "03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c"
rtk proxy rg -q '使用说明、交接记录和问题跟踪三个目录' materials/archive-confirmation.md
rtk proxy test -s materials/role-confirmation.md
role_hash=$(rtk proxy shasum -a 256 materials/role-confirmation.md | rtk proxy awk '{print $1}')
rtk proxy test "$role_hash" = "8117dffe9cf6088adf357e9d0c404d1f4f547c430dd520cf1095d8cb27bf1da5"
rtk proxy rg -q '协调者确认值班人员维护交接记录' materials/role-confirmation.md
rtk proxy rg -q '资料管理员维护使用说明' materials/role-confirmation.md
rtk proxy rg -q '问题负责人维护问题跟踪' materials/role-confirmation.md

rtk proxy rg -q '角色级职责边界已有新增材料支持' deliverables/zhusheng-change-proposals.md
rtk proxy rg -q '值班人员维护交接记录' deliverables/zhusheng-consistency.md
rtk proxy rg -q '具体问题事实尚未提供' deliverables/zhusheng-consistency.md

for receipt in \
  .work-receipts/zs-compare-cr-01-current.json \
  .work-receipts/zs-compare-cr-02-current.json \
  .work-receipts/zs-compare-cr-03-current.json
do
  rtk proxy jq -e '.proposal.status == "ready" and .apply_attempt == null' "$receipt"
done

rtk proxy jq -e '
  ([.events[] | select(.type == "work.progressed" and .id == "01M21X100BNG52MGXNTZD17VBA")] | length) == 1
' .work-receipts/zs-compare-current-history.json
rtk proxy jq -e '
  ([.events[] | select(.type == "work.completed" and .id == "01M21WAZT1HX58ASPY3G5X95R7")] | length) == 1
' .work-receipts/zs-diag-current-history.json

rtk proxy rg -q '"id":"ZS-DIAG".*"status":"completed"' work-ledger.yaml
rtk proxy rg -q '"id":"ZS-COMPARE".*"status":"in_progress"' work-ledger.yaml

if rtk proxy rg -q '资料归档按内容类型分目录|负责角色按值班与资料分工|遗留问题保留已知影响和下一步' work-ledger.yaml; then
  exit 1
fi

rtk proxy git diff --check
rtk proxy printf '%s\n' 'ZS compare completion preflight passed'
