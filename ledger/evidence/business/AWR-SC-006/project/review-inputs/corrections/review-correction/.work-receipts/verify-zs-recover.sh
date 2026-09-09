#!/bin/sh
set -eu

rtk proxy test -s deliverables/zhusheng-recovery-work.md
rtk proxy test -s deliverables/zhusheng-consistency.md
rtk proxy test -s deliverables/zhusheng-change-proposals.md
rtk proxy shasum -a 256 -c .work-receipts/preserved-handover-inputs.sha256

archive_hash=$(rtk proxy shasum -a 256 materials/archive-confirmation.md | rtk proxy awk '{print $1}')
role_hash=$(rtk proxy shasum -a 256 materials/role-confirmation.md | rtk proxy awk '{print $1}')
rtk proxy test "$archive_hash" = "03fd800c15f3571df42ec52f69073f9230425e0eb1aaa6019ec5fdfa5b60cf5c"
rtk proxy test "$role_hash" = "8117dffe9cf6088adf357e9d0c404d1f4f547c430dd520cf1095d8cb27bf1da5"

rtk proxy jq -e '
  ([.proposals[] | select(.id == "01M21X4H7GSP7DY19MTHYDBXST")] | length) == 1 and
  ([.proposals[] | select(.id == "01M21X5MHW7RFN8PBE4J86GACM")] | length) == 1 and
  ([.proposals[] | select(.id == "01M21X67QFPQ55YSMJEX96HYV7")] | length) == 1 and
  ([.proposals[] | select(
    (.id == "01M21X4H7GSP7DY19MTHYDBXST" or
     .id == "01M21X5MHW7RFN8PBE4J86GACM" or
     .id == "01M21X67QFPQ55YSMJEX96HYV7") and
    .status == "rejected"
  )] | length) == 3
' .work-receipts/zs-recover-proposal-inventory.json

for receipt in \
  .work-receipts/cr-01-after-disposition.json \
  .work-receipts/cr-02-after-disposition.json \
  .work-receipts/cr-03-after-disposition.json
do
  rtk proxy jq -e '.proposal.status == "rejected" and .apply_attempt == null' "$receipt"
done

rtk proxy jq -e '
  ([.events[] | select(.type == "work.completed" and .id == "01M220936P02WQGAD119TQ246V")] | length) == 1
' .work-receipts/zs-compare-history-after-completion.json
rtk proxy jq -e '
  ([.events[] | select(.type == "work.progressed" and .id == "01M220DTPGKYR02FB2F11VCE5S")] | length) == 1
' .work-receipts/zs-recover-history-current.json

rtk proxy jq -e '
  .work.status == "in_progress" and
  (.acceptance | length) == 2
' .work-receipts/zs-recover-work-current.json
rtk proxy rg -q '"id":"ZS-COMPARE".*"status":"completed"' work-ledger.yaml
rtk proxy rg -q '"id":"ZS-RECOVER".*"status":"in_progress"' work-ledger.yaml
if rtk proxy rg -q '资料归档按内容类型分目录|负责角色按值班与资料分工|遗留问题保留已知影响和下一步' work-ledger.yaml; then
  exit 1
fi

rtk proxy rg -q '没有顺序覆盖、重复应用或静默丢弃' deliverables/zhusheng-recovery-work.md
rtk proxy rg -q '具体遗留问题清单仍未提供' deliverables/zhusheng-recovery-work.md
rtk proxy rg -q '三份旧原子提案均已显式标记为 `rejected`' deliverables/zhusheng-consistency.md
rtk proxy rg -q '已处置的原子提案' deliverables/zhusheng-change-proposals.md
rtk proxy git diff --check
rtk proxy printf '%s\n' 'ZS recovery no-duplicate/no-loss verification passed'
