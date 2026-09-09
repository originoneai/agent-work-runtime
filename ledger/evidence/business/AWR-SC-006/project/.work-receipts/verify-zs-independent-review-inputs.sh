#!/bin/sh
set -eu

rtk proxy shasum -a 256 -c .work-receipts/zs-independent-review-inputs.sha256
rtk proxy jq -e '.proposal.status == "rejected" and .apply_attempt == null' .work-receipts/cr-01-after-disposition.json
rtk proxy jq -e '.proposal.status == "rejected" and .apply_attempt == null' .work-receipts/cr-02-after-disposition.json
rtk proxy jq -e '.proposal.status == "rejected" and .apply_attempt == null' .work-receipts/cr-03-after-disposition.json
rtk proxy test ! -e deliverables/zs-independent-review.md
rtk proxy test ! -e deliverables/zs-delivery.md
rtk proxy git diff --check
rtk proxy printf '%s\n' 'ZS independent review input verification passed; review and delivery are not prefilled'
