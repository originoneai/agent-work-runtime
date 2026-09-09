#!/bin/sh
set -eu

rtk proxy shasum -a 256 -c .work-receipts/preserved-handover-inputs.sha256
rtk proxy jq -e '(.proposals | length) == 3 and ([.proposals[] | select(.status == "ready" and .binding_valid == true)] | length) == 3 and (([.proposals[].id] | sort) == (["01M21X4H7GSP7DY19MTHYDBXST", "01M21X5MHW7RFN8PBE4J86GACM", "01M21X67QFPQ55YSMJEX96HYV7"] | sort))' .work-receipts/proposal-ready-list.json
rtk proxy jq -e '(.acceptance | length) == 2' .work-receipts/zs-recover-after-proposals.json
rtk proxy rg -q '01M21X4H7GSP7DY19MTHYDBXST' deliverables/zhusheng-change-proposals.md
rtk proxy rg -q '01M21X5MHW7RFN8PBE4J86GACM' deliverables/zhusheng-change-proposals.md
rtk proxy rg -q '01M21X67QFPQ55YSMJEX96HYV7' deliverables/zhusheng-change-proposals.md

if rtk proxy rg -q '资料归档按内容类型分目录|负责角色按值班与资料分工|遗留问题保留已知影响和下一步' work-ledger.yaml; then
  exit 1
fi

rtk proxy git diff --check
rtk proxy printf '%s\n' 'ZS change proposal verification passed'
