#!/bin/sh
set -eu

rtk proxy test -s deliverables/zhusheng-consistency.md
rtk proxy test -s deliverables/zhusheng-change-proposals.md
rtk proxy shasum -a 256 -c .work-receipts/preserved-handover-inputs.sha256

archive_hash=$(rtk proxy shasum -a 256 materials/archive-confirmation.md | rtk proxy awk '{print $1}')
rtk proxy test "$archive_hash" = "03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c"
rtk proxy rg -q '使用说明、交接记录和问题跟踪三个目录' materials/archive-confirmation.md
rtk proxy rg -q '负责角色仍需协调者确认' materials/archive-confirmation.md

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

rtk proxy rg -q '^?? materials/archive-confirmation.md$' .work-receipts/zs-compare-git-status.txt
rtk proxy rg -q '新增归档确认尚未进入 Git' deliverables/zhusheng-consistency.md
rtk proxy rg -q '未批准、未应用、无 apply attempt' deliverables/zhusheng-consistency.md
rtk proxy rg -q '独立复核与最终交付均未发生' deliverables/zhusheng-consistency.md

consistency_hash=$(rtk proxy shasum -a 256 deliverables/zhusheng-consistency.md | rtk proxy awk '{print $1}')
report_hash=$(rtk proxy shasum -a 256 .work-receipts/zs-compare-evidence-report.json | rtk proxy awk '{print $1}')
rtk proxy test "$consistency_hash" = "0571c415f20395f12543126dcea1fc43b6fca332d05af1f69864b0ce3bb314a8"
rtk proxy test "$report_hash" = "79260e32596a6bafbdc9bff0890f74b31c7148546ab826a7765f2acb5883c7c8"
rtk proxy jq -e '
  .evidence.external_key == "zs-compare-handover-20260909" and
  .evidence.level == "locally_verified" and
  .content_hash_verified == true
' .work-receipts/zs-compare-evidence-show.json
rtk proxy jq -e '
  (.proposals | length) == 3 and
  ([.proposals[] | select(.status == "ready" and .binding_valid == true)] | length) == 3
' .work-receipts/zs-compare-proposals-after-evidence.json
rtk proxy jq -e '
  .completeness.complete == true and
  (.completeness.evidence_gaps | length) == 0 and
  .work_context.identity.work_item_key == "ZS-COMPARE"
' .work-receipts/zs-compare-context-current-evidence.json
rtk proxy git diff --check
rtk proxy printf '%s\n' 'ZS saved-versus-effective handover verification passed'
