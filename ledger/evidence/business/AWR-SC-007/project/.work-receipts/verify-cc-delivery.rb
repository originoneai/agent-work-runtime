require "digest"
require "json"
require "open3"

root = File.expand_path("..", __dir__)
manifest_path = File.join(root, "deliverables/cc-delivery.md")
review_verification_path = File.join(root, ".work-receipts/reviewer-independent-verification.json")
archive_relative_path = "deliverables/chengchuan-public-delivery.zip"
archive_path = File.join(root, archive_relative_path)

manifest = File.read(manifest_path)
allowed_references = [
  "chengchuan-public-report.md",
  "../materials/public-summary.csv",
  "../materials/log-notice.md",
  "../materials/operations.jsonl",
  "../materials/access-register.md",
  "chengchuan-access-gaps.md",
  "chengchuan-log-summary.md",
  "cc-public-verification.json",
  "cc-independent-review.md",
  "../.work-receipts/reviewer-independent-verification.json"
]
references = manifest.scan(/\]\(([^)]+)\)/).flatten.uniq
unresolved_links = references.reject do |reference|
  File.file?(File.expand_path(reference, File.dirname(manifest_path)))
end

required_text = [
  "对外交付主件",
  "可以对外使用的范围",
  "仍未核实的事项",
  "后续责任",
  "默认外发包不附送",
  "内部追溯和审计",
  "受限原件保持未读",
  "通过且无需返工"
]
missing_required_text = required_text.reject { |text| manifest.include?(text) }

sensitive_pattern = /awr-live-|public-batch-|01M[0-9A-HJKMNP-TV-Z]{23}|(?<![0-9a-f])[0-9a-f]{40,64}(?![0-9a-f])|\/Users\/|(?:api[_-]?key|password|secret|bearer)\s*[:=]|-----BEGIN [A-Z ]*PRIVATE KEY-----|[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}|(?<!\d)1[3-9]\d{9}(?!\d)|(?:\d{1,3}\.){3}\d{1,3}/i
sensitive_matches = manifest.scan(sensitive_pattern)

review = JSON.parse(File.read(review_verification_path))
review_passed = review.fetch("overall_verdict") == "pass" &&
  review.fetch("producer_rework_required") == false &&
  review.fetch("checks").all? { |check| check.fetch("passed") == true }

expected_members = [
  "deliverables/cc-delivery.md",
  "deliverables/chengchuan-public-report.md",
  "materials/public-summary.csv",
  "materials/log-notice.md"
]
member_stdout, member_stderr, member_status = Open3.capture3(
  "rtk", "unzip", "-Z", "-1", archive_relative_path, chdir: root
)
actual_members = member_stdout.lines.map(&:chomp)

archive_content_matches = member_status.success? && expected_members.all? do |member|
  content, _stderr, status = Open3.capture3(
    "rtk", "unzip", "-p", archive_relative_path, member, chdir: root
  )
  status.success? && content.b == File.binread(File.join(root, member))
end

expected_hashes = {
  "materials/access-register.md" => "b6e4e023be4e205b5a9ec6bae4c4f6a20d8d85b61b96d46e9245781f97decaf9",
  "materials/public-summary.csv" => "8465ff50f822730007c279f0f522f706dd37d879ca87ac87dab4cd219f25a2a5",
  "materials/log-notice.md" => "5651b4898510fda4da4211855415d4833f90acfc2ab20debdc0b4d8a366ea83d",
  "materials/operations.jsonl" => "058e7a7ed4c472d8e2e868e9e6aa57216944fe513bf3cb303d5bb31d73f35cf1",
  "deliverables/chengchuan-public-report.md" => "e09f87db8531d11d4fda7f7ce5e05440b978d4089be056d5ac1b9c5abfc39ecf",
  "deliverables/cc-independent-review.md" => "e7b52be7f59614f54271915dbea878504858ba476c52768360eaf860de612bd8",
  ".work-receipts/reviewer-independent-verification.json" => "aaedb7933db5c343854333c1fac71c4329cd4eadc19f92c1892c0e21611b10a4",
  "deliverables/cc-delivery.md" => "273cf67e16a54cc73ff2182b5f5d07a014b7901052a0dc05742e8acac6682e4b",
  "deliverables/chengchuan-public-delivery.zip" => "2d46994cf19f6314d3e81e583f7b10e0f2abe4a53f6cd7dda124f09b200c862e"
}
hash_mismatches = expected_hashes.each_with_object([]) do |(path, expected), mismatches|
  actual = Digest::SHA256.file(File.join(root, path)).hexdigest
  unless actual == expected
    mismatches << { "path" => path, "expected" => expected, "actual" => actual }
  end
end

passed = references.sort == allowed_references.sort &&
  unresolved_links.empty? &&
  missing_required_text.empty? &&
  sensitive_matches.empty? &&
  review_passed &&
  member_status.success? &&
  member_stderr.empty? &&
  actual_members == expected_members &&
  archive_content_matches &&
  hash_mismatches.empty?

puts JSON.generate(
  {
    "passed" => passed,
    "references" => references.length,
    "unresolved_links" => unresolved_links,
    "missing_required_text" => missing_required_text,
    "sensitive_matches" => sensitive_matches.length,
    "review_passed" => review_passed,
    "archive_members" => actual_members,
    "archive_content_matches" => archive_content_matches,
    "hash_mismatches" => hash_mismatches
  }
)

exit(passed ? 0 : 1)
