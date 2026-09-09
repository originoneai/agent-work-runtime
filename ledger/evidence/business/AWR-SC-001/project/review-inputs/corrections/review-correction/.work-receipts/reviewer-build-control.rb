require "json"
require "digest"

def load_json(path)
  JSON.parse(File.read(path))
end

def sha256(path)
  Digest::SHA256.file(path).hexdigest
end

manifest = load_json(".work-receipts/reviewer-reviewed-files.json")
verification = load_json(".work-receipts/reviewer-verification.json")
session_start = load_json(".work-receipts/reviewer-session-start.json")
session_end = load_json(".work-receipts/reviewer-session-end.json")
final_status = load_json(".work-receipts/reviewer-final-status.json")
final_work = load_json(".work-receipts/reviewer-final-work-show.json")
lookup = load_json("review-inputs/reference-lookup-process-record.json")

turns = %w[initial round-1 round-2].map do |phase|
  record_path = "review-inputs/#{phase}/turn-record.json"
  record = load_json(record_path)
  {
    phase: phase,
    record: record_path,
    record_sha256: sha256(record_path),
    client_task_id: record["client_task_id"],
    actual_user_turn: record["actual_user_turn"],
    source_sha256: record["source_sha256"],
    artifact_sha256: record["artifact_sha256"],
    native_transcript_sha256: record["native_transcript_sha256"],
    events_sha256: record["events_sha256"]
  }
end

receipt_paths = %w[
  .work-receipts/reviewer-session-start.json
  .work-receipts/reviewer-context-compile.json
  .work-receipts/reviewer-progress.json
  .work-receipts/reviewer-reviewed-files.json
  .work-receipts/reviewer-verification.json
  .work-receipts/reviewer-artifact-add.json
  .work-receipts/reviewer-evidence-draft.json
  .work-receipts/reviewer-evidence-add.json
  .work-receipts/reviewer-completion.json
  .work-receipts/reviewer-work-complete.json
  .work-receipts/reviewer-checkpoint.json
  .work-receipts/reviewer-session-end.json
  .work-receipts/reviewer-final-status.json
  .work-receipts/reviewer-final-work-show.json
  .work-receipts/reviewer-work-qh-input.json
  .work-receipts/reviewer-work-qh-brief.json
]
receipt_paths.each { |path| abort("missing #{path}") unless File.file?(path) }

record = {
  kind: "actual_independent_review_run",
  scenario: "project-onboarding",
  work_item: "QH-REVIEW",
  recorded_at_ms: (Time.now.to_f * 1000).to_i,
  reviewer: {
    actual_identity: session_start.dig("session", "agent_id"),
    provider: session_start.dig("session", "provider"),
    model: session_start.dig("session", "model"),
    awr_session_id: session_start.dig("session", "id"),
    awr_claim_id: session_start.dig("claim", "id"),
    session_status: session_end.dig("session", "status"),
    start_project_revision: session_start.dig("session", "start_project_revision"),
    end_project_revision: session_end.dig("session", "end_project_revision"),
    native_model_client_thread: nil,
    identity_note: "Review executed by the current luna_worker task; no separate model client or invented reviewer signature was used."
  },
  executor: {
    client: "Codex CLI",
    client_task_id: "01a08379-ef89-7173-917d-274662891bcb",
    reviewer_authorship_of_executor_artifacts: false
  },
  result: {
    review_work_completed: true,
    reviewed_package_verdict: verification["verdict"],
    reviewed_package_approved_for_final_delivery: false,
    qh_delivery_created: File.exist?("deliverables/qh-delivery.md"),
    scenario_completed: false,
    next_owner: "original executor",
    next_action: "Address QH-R01 through QH-R05, retain external unknowns, then prepare the final delivery and response-to-review trace."
  },
  current_authority_sha256: {
    "project.toml" => sha256("project.toml"),
    "GOALS.md" => sha256("GOALS.md"),
    "PLAN.md" => sha256("PLAN.md"),
    "RULES.md" => sha256("RULES.md"),
    "work-ledger.yaml" => sha256("work-ledger.yaml")
  },
  executor_handoff_work_ledger_sha256: manifest["executor_handoff_work_ledger_sha256"],
  review_snapshot_files_sha256: manifest["current_files"],
  historical_turns: turns,
  acceptance_conclusions: [
    {id: "current_sources", conclusion: "pass", basis: "Current AWR context loaded RULES r2 and ledger r9 as fresh; completeness is not treated as business approval."},
    {id: "brief_rule_revision", conclusion: "pass", basis: "The current brief limits the external directory to verified titles, excludes personal contacts and makes domain confirmation a prerequisite for any specific switch time."},
    {id: "unknowns", conclusion: "pass", basis: "Document files, title confirmations, domain approval and legacy retirement date remain explicitly unconfirmed."},
    {id: "dependencies_currency", conclusion: "fail", basis: "The dedicated dependency list remains at QH-INPUT/project revision 20 and omits round-2 gates."},
    {id: "context_reference_currency", conclusion: "fail", basis: "The context reference contains only the initial QH-INPUT session and old RULES fingerprint."},
    {id: "ledger_state_consistency", conclusion: "fail", basis: "Completed QH-INPUT and QH-BRIEF retain pre-completion next actions and in-progress summaries."},
    {id: "directory_review_prerequisite", conclusion: "fail_for_final_delivery", basis: "No directory draft or actual source documents/title confirmations were observed."},
    {id: "history_trace", conclusion: "pass", basis: "Three sealed records share the executor thread and current artifact hashes match round-2."},
    {id: "evidence_projection", conclusion: "needs_explanation", basis: "Complete current locally_verified evidence is retained alongside locator-only Unknown projections; Unknown entries are not treated as satisfied."},
    {id: "review_independence", conclusion: "pass", basis: "The reviewer session identity differs from the executor and the executor artifacts were unchanged."}
  ],
  findings: verification["finding_results"].map { |id, disposition| {id: id, disposition: disposition} },
  external_unknowns_retained: [
    "Actual files or links, versions, access state, externally publishable titles and responsibility information for the three documents",
    "Information administrator confirmation for the external domain",
    "Legacy page retirement date, confirming role, switch conditions and rollback arrangement",
    "Actual directory title, entry and personal-contact removal results",
    "Specific switch time, external access readiness and portal go-live state"
  ],
  process_deviation_assessment: {
    record: "review-inputs/reference-lookup-process-record.json",
    record_sha256: sha256("review-inputs/reference-lookup-process-record.json"),
    observed_out_of_scope_commands: lookup.dig("later_technical_reference_correction", "out_of_scope_reference_commands")&.length,
    conclusion: "process_nonconformance_no_observed_business_contamination",
    basis: "Returned material consists of generic CLI/schema examples, development verification dates and technical code excerpts. No future business prompt, rubric, expected answer or other scenario business result was observed in the retained output."
  },
  review_artifact: {
    path: "deliverables/qh-independent-review.md",
    sha256: sha256("deliverables/qh-independent-review.md"),
    awr_artifact_id: load_json(".work-receipts/reviewer-artifact-add.json").dig("artifact", "id")
  },
  evidence: {
    verification_report: ".work-receipts/reviewer-verification.json",
    verification_sha256: sha256(".work-receipts/reviewer-verification.json"),
    evidence_id: load_json(".work-receipts/reviewer-evidence-add.json").dig("evidence", "id"),
    explicit_evidence_level: load_json(".work-receipts/reviewer-evidence-add.json").dig("evidence", "level"),
    explicit_evidence_currency: final_work.fetch("evidence").find { |e| e["external_key"] == "QH-INDEPENDENT-REVIEW-20260909" }&.fetch("currency"),
    locator_only_unknown_retained: final_work.fetch("evidence").any? { |e| e["level"] == "unknown" }
  },
  awr_final_state: {
    project_revision: final_status["project_revision"],
    counts: final_status["counts"],
    suggested_work: final_status.dig("suggested_work", "external_key"),
    suggested_work_status: final_status.dig("suggested_work", "status"),
    review_status: final_work.dig("work", "status"),
    active_review_claims: final_work.dig("work", "active_claims"),
    checkpoint_id: session_end.dig("session", "last_checkpoint_id")
  },
  receipts_sha256: receipt_paths.to_h { |path| [path, sha256(path)] }
}

abort("unexpected reviewer identity") unless record.dig(:reviewer, :actual_identity) == "luna_worker"
abort("review session not ended") unless record.dig(:reviewer, :session_status) == "ended"
abort("final delivery unexpectedly exists") if record.dig(:result, :qh_delivery_created)
abort("unexpected next work") unless record.dig(:awr_final_state, :suggested_work) == "QH-DELIVER"

puts JSON.pretty_generate(record)
