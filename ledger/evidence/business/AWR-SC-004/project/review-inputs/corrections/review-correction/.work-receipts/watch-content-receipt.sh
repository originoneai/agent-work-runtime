#!/bin/sh
set -eu

project="/Users/mac/Documents/originone/agent-work-running/.local/business-live-20260909-v1/parallel-ownership/project"
awr="/Users/mac/Documents/originone/agent-work-running/target/release/awr"
owner_session="01M21WYFWXQHZNK0BJTYVD8N1K"
baseline_content="42a801e823f1f0e510e054fa48f6e8a35e93f1874ddae1eadda09e87284eaaf3"
baseline_venue="3035f626d7ef00cd860d2fd4781513b76d9587b1e6c67924b549ce8865c828a6"
baseline_ledger="13a426a9dff824fe1f20daa8f5a781ad3b61f814f6e8c0dd839877b87c046c12"
baseline_checkpoint="01M22000PQ6KE0FQ4JB855MPEK"
poll=0

while [ "$poll" -lt 180 ]; do
  rtk proxy sleep 10
  poll=$((poll + 1))

  content_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-content.md" | rtk proxy awk '{print $1}')"
  venue_sha="$(rtk proxy shasum -a 256 "$project/deliverables/xingqiao-venue.md" | rtk proxy awk '{print $1}')"
  ledger_sha="$(rtk proxy shasum -a 256 "$project/work-ledger.yaml" | rtk proxy awk '{print $1}')"
  owner_checkpoint="$(rtk proxy "$awr" session show --project "$project" "$owner_session" --json | rtk proxy jq -r '.session.last_checkpoint_id // ""')"
  owner_status="$(rtk proxy "$awr" session show --project "$project" "$owner_session" --json | rtk proxy jq -r '.session.status')"

  if [ "$content_sha" != "$baseline_content" ] || [ "$venue_sha" != "$baseline_venue" ] || [ "$ledger_sha" != "$baseline_ledger" ] || [ "$owner_checkpoint" != "$baseline_checkpoint" ] || [ "$owner_status" != "active" ]; then
    rtk proxy printf '%s\n' "CONTENT_OWNER_UPDATE poll=$poll content_sha=$content_sha venue_sha=$venue_sha ledger_sha=$ledger_sha owner_checkpoint=$owner_checkpoint owner_status=$owner_status"
    exit 0
  fi

  if [ $((poll % 3)) -eq 0 ]; then
    rtk proxy printf '%s\n' "WAITING poll=$poll content_sha=$content_sha owner_checkpoint=$owner_checkpoint owner_status=$owner_status"
  fi
done

rtk proxy printf '%s\n' "WAIT_TIMEOUT polls=$poll"
exit 124
