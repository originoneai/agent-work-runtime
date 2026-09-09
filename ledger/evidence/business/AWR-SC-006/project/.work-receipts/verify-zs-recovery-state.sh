#!/bin/sh
set -eu

rtk proxy test -s deliverables/zhusheng-recovery-work.md
rtk proxy shasum -a 256 -c .work-receipts/preserved-handover-inputs.sha256

rtk proxy jq -e '
  .session.id == "01M21WZXPSGZ77369CSGXCB8CX" and
  .session.status == "interrupted" and
  .resumed_successor.id == "01M21XP8VZ1YJRNSS6BJH2V4HB" and
  ([.claims[] | select(.status == "released")] | length) == 1
' .work-receipts/zs-compare-interrupted-session-after-resume.json

rtk proxy jq -e '
  .session.id == "01M21XP8VZ1YJRNSS6BJH2V4HB" and
  .session.status == "active" and
  .checkpoint.id == "01M21Y1W5N3N6VEEC0AQGGT2N9" and
  ([.claims[] | select(.id == "01M21XP8VZ19C36AM6A0HG2QRW" and .status == "active")] | length) == 1
' .work-receipts/zs-compare-recovery-final-session.json

rtk proxy jq -e '.context.complete == true' .work-receipts/zs-compare-recovery-final-bootstrap.json
rtk proxy jq -e '
  .completeness.complete == true and
  .work_context.identity.work_item_key == "ZS-COMPARE" and
  .work_context.context_hash == "ab4beb6621addd2f7e510eefea145ea90b86d4c19b0c6cd138d7600c8acae841"
' .work-receipts/zs-compare-recovery-context.json

rtk proxy jq -e '
  (.proposals | length) == 3 and
  ([.proposals[] | select(.status == "ready" and .binding_valid == true)] | length) == 3 and
  (([.proposals[].id] | sort) == ([
    "01M21X4H7GSP7DY19MTHYDBXST",
    "01M21X5MHW7RFN8PBE4J86GACM",
    "01M21X67QFPQ55YSMJEX96HYV7"
  ] | sort))
' .work-receipts/zs-compare-recovery-final-proposals.json

rtk proxy jq -e '
  .project_revision == 48 and
  ([.current[] | select(.external_key == "ZS-COMPARE" and .status == "in_progress")] | length) == 1
' .work-receipts/zs-compare-recovery-final-status.json

rtk proxy rg -q '"database_ok": true' .work-receipts/zs-recovery-final-doctor.log
rtk proxy rg -q '"foreign_key_violations": 0' .work-receipts/zs-recovery-final-doctor.log

if rtk proxy rg -q '资料归档按内容类型分目录|负责角色按值班与资料分工|遗留问题保留已知影响和下一步' work-ledger.yaml; then
  exit 1
fi

rtk proxy rg -q '不是权威台账、提案批准、应用回执或独立复核结论' deliverables/zhusheng-recovery-work.md
rtk proxy rg -q '无法从 AWR 还原' deliverables/zhusheng-recovery-work.md
rtk proxy git diff --check
rtk proxy printf '%s\n' 'ZS interruption recovery verification passed'
