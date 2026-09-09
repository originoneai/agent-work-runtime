require "json"
require "digest"

expected = {
  "RULES.md" => "05c7d1e94f96e3f6bdaccfd8790895a07e11472fe95542c046eb570456ad218d",
  "deliverables/qinghe-brief.md" => "31b843562c3918b531d774e1ab19b7b6912db627de6ce2dc4ba37515fa1cfccd",
  "deliverables/qinghe-dependencies.md" => "b3bbb599f1ef5e3f15ffe1b2e149bb1edc5d218c470eab30186744167071161c",
  "deliverables/qinghe-context-reference.md" => "8b9508d2965a6d97fd6b76c6afc5f3acad15f52ac326c9819633ad7da19571d3",
  "review-inputs/initial/turn-record.json" => "1b3165cca558196c903eabf8e56e1ec0fffec8d0592b8a5896b82507df71b066",
  "review-inputs/round-1/turn-record.json" => "6c611669ab4742323067a085651643cb8508525909023cb5981e0523511207a3",
  "review-inputs/round-2/turn-record.json" => "7965f3a8d622470c9fd18a78481d280658880359e4ee90a00f260bb12c590b53",
  "review-inputs/reference-lookup-process-record.json" => "0aa782e30d7ac1f2f2b9ba65f9c2224a07404d3111ea5353f1ff0f3015722920"
}

expected.each do |path, sha256|
  abort("missing #{path}") unless File.file?(path)
  actual = Digest::SHA256.file(path).hexdigest
  abort("hash mismatch #{path}: #{actual}") unless actual == sha256
end

review_path = "deliverables/qh-independent-review.md"
abort("missing review") unless File.file?(review_path)
review = File.read(review_path)
%w[QH-R01 QH-R02 QH-R03 QH-R04 QH-R05].each do |finding|
  abort("missing finding #{finding}") unless review.include?(finding)
end
[
  "当前交接包不通过进入最终交付",
  "不确认内容目录",
  "实际复核者：`luna_worker`",
  "仍待外部确认"
].each do |required|
  abort("missing review boundary #{required}") unless review.include?(required)
end

brief = File.read("deliverables/qinghe-brief.md")
[
  "对外目录只列已核实的文档标题",
  "不展示个人联系人",
  "域名未确认前不承诺具体切换时间",
  "内容目录草案完成"
].each do |required|
  abort("brief missing #{required}") unless brief.include?(required)
end

context_reference = File.read("deliverables/qinghe-context-reference.md")
abort("context-reference unexpectedly current") unless context_reference.include?("AWR project revision：20")
abort("context-reference lacks old rules fingerprint") unless context_reference.include?("14fb573ef193e475f1f6d6bf54cfcf59888646150cee3ea0719de39733327386")
abort("context-reference already contains revised rules") if context_reference.include?(expected.fetch("RULES.md"))

dependencies = File.read("deliverables/qinghe-dependencies.md")
abort("dependencies unexpectedly current") unless dependencies.include?("AWR project revision 20")
abort("dependencies does not retain initial sequencing") unless dependencies.include?("`QH-BRIEF` 才能开始")

rounds = %w[initial round-1 round-2].map do |phase|
  JSON.parse(File.read("review-inputs/#{phase}/turn-record.json"))
end
abort("executor native thread mismatch") unless rounds.all? { |r| r["client_task_id"] == "01a08379-ef89-7173-917d-274662891bcb" }
abort("current brief differs from sealed round-2") unless rounds.last.dig("artifact_sha256", "deliverables/qinghe-brief.md") == expected.fetch("deliverables/qinghe-brief.md")

lookup = JSON.parse(File.read("review-inputs/reference-lookup-process-record.json"))
abort("scope deviation not retained") unless lookup["kind"] == "retained_reference_lookup_scope_deviation"
abort("scope assessment was prefilled") unless lookup["independent_assessment"] == "pending"

input_work = JSON.parse(File.read(".work-receipts/reviewer-work-qh-input.json"))
brief_work = JSON.parse(File.read(".work-receipts/reviewer-work-qh-brief.json"))
[input_work, brief_work].each do |work|
  levels = work.fetch("evidence").map { |e| [e["level"], e["currency"]] }
  abort("missing complete current evidence") unless levels.include?(["locally_verified", "current"])
  abort("missing locator-only unknown evidence") unless levels.include?(["unknown", "unknown"])
end

executor_artifacts = %w[
  deliverables/qinghe-brief.md
  deliverables/qinghe-context-reference.md
  deliverables/qinghe-dependencies.md
]
abort("executor artifacts changed during review") unless executor_artifacts.all? { |p| Digest::SHA256.file(p).hexdigest == expected.fetch(p) }
abort("final delivery was created") if File.exist?("deliverables/qh-delivery.md")

materials = Dir.glob("materials/*").select { |p| File.file?(p) }.sort
expected_materials = %w[materials/backlog.csv materials/request.md materials/week-window.md]
abort("unexpected material inventory") unless materials == expected_materials

result = {
  version: 1,
  work_item: "QH-REVIEW",
  reviewer: "luna_worker",
  reviewer_session: "01M21VK36SJ9KXRB3634VZ1FPS",
  executor_client_thread: "01a08379-ef89-7173-917d-274662891bcb",
  source_sha: "cd44a161471cee94fd4f797955327a4fd995cf3a",
  command: "rtk proxy ruby .work-receipts/reviewer-verify.rb",
  scope: ["QH-REVIEW"],
  verified_at: (Time.now.to_f * 1000).to_i,
  verdict: "requires_executor_changes",
  review_artifact: {
    locator: review_path,
    sha256: Digest::SHA256.file(review_path).hexdigest
  },
  input_manifest: {
    locator: ".work-receipts/reviewer-reviewed-files.json",
    sha256: Digest::SHA256.file(".work-receipts/reviewer-reviewed-files.json").hexdigest
  },
  checks: [
    {
      name: "当前来源、依赖、生效规则与历史修订独立核对",
      passed: true,
      details: "复核记录逐项判定简报、专用依赖清单、上下文引用、台账状态、目录前置条件、AWR 证据及技术查找范围偏离；结论为需原执行者整改，不批准进入最终交付。",
      criteria: ["独立核对来源、依赖和生效规则，逐条记录修改要求。"]
    },
    {
      name: "产物、来源指纹与未知事项边界",
      passed: true,
      details: "已绑定复核文件与逐文件 SHA-256 清单；三份说明、域名、旧页面停用日期、目录内容和切换时间均保留为未确认，且未创建最终交付。",
      criteria: ["保留实际产物与来源引用，无法确认的内容显式说明。"]
    }
  ],
  finding_results: {
    "QH-R01" => "blocking",
    "QH-R02" => "blocking",
    "QH-R03" => "blocks_directory_and_final_delivery",
    "QH-R04" => "must_explain_or_resolve_before_final_delivery",
    "QH-R05" => "process_nonconformance_no_observed_business_contamination"
  },
  external_confirmations_made_by_reviewer: [],
  final_delivery_created: false,
  scenario_completion_claimed: false
}

puts JSON.pretty_generate(result)
