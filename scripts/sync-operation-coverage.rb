#!/usr/bin/env ruby
# frozen_string_literal: true

require "json"
require "yaml"

ROOT = File.expand_path("..", __dir__)
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
JSON_PATH = File.join(ROOT, "todo", "openapi-operation-coverage.json")
MARKDOWN_PATH = File.join(ROOT, "todo", "OPENAPI-COVERAGE.md")
HTTP_METHODS = %w[get post put patch delete head options trace].freeze


def assignment_for(tag, path, operation_id)
  case tag
  when "Health"
    ["M0", "M00-BACKEND", "P0"]
  when "Auth"
    package = if operation_id == "getCsrfToken" || operation_id == "login" || operation_id == "logout"
                "M02-SESSION"
              else
                "M02-IDENTITY"
              end
    ["M2", package, "P0"]
  when "Users"
    ["M3", "M03-PROFILE", "P1"]
  when "Roles"
    ["M3", "M03-AUTHZ", "P0"]
  when "Boards", "Tags"
    ["M3", "M03-BOARDS", "P1"]
  when "Posts", "Drafts", "Revisions"
    ["M4", "M04-POSTS", "P1"]
  when "Comments"
    ["M4", "M04-COMMENTS", "P1"]
  when "Moderation"
    ["M5", "M05-CASES", "P0"]
  when "Notifications"
    ["M5", "M05-NOTIFY", "P1"]
  when "Attachments"
    package = path.include?("download") ? "M06-DOWNLOAD" : "M06-UPLOAD"
    ["M6", package, "P0"]
  when "Download Billing"
    ["M6", "M06-DOWNLOAD", "P0"]
  when "Shop"
    ["M7", "M07-SHOP", "P1"]
  when "Activity"
    ["M7", "M07-LEVELS", "P1"]
  when "Search"
    ["M8", "M08-INDEX", "P1"]
  when "Feeds"
    ["M8", "M08-FEEDS", "P1"]
  when "AI"
    ["M9", "M09-SUGGESTIONS", "P1"]
  when "Video"
    ["M10", "M10-VIDEO", "P0"]
  when "OAuth"
    ["M11", "M11-PROTOCOL", "P0"]
  when "OAuth Clients"
    ["M11", "M11-CONSENT", "P0"]
  when "Marketplace"
    ["M12", "M12-CHECKOUT", "P0"]
  when "Themes"
    ["M13", "M13-THEME", "P1"]
  when "Admin"
    return ["M6", "M06-QUOTA", "P0"] if path.include?("/storage") || path.include?("attachment-quota")
    return ["M5", "M05-SANCTIONS", "P0"] if path.include?("/moderation")
    return ["M12", "M12-CLIENTS", "P0"] if path.include?("/marketplace")
    return ["M9", "M09-GATEWAY", "P0"] if path.include?("/ai/") || path.end_with?("/ai/config")
    return ["M10", "M10-VIDEO", "P0"] if path.include?("/video/") || path.end_with?("/video/policies")
    return ["M13", "M13-THEME", "P1"] if path.include?("/themes")

    ["M13", "M13-ADMIN", "P0"]
  else
    raise "No roadmap assignment for tag #{tag.inspect} (#{operation_id}, #{path})"
  end
end


def load_existing
  return {} unless File.file?(JSON_PATH)

  JSON.parse(File.read(JSON_PATH)).fetch("operations", []).each_with_object({}) do |entry, index|
    index[entry.fetch("operation_id")] = entry
  end
end


def operations_from(document, existing)
  operations = []

  document.fetch("paths").each do |path, item|
    item.each do |method, operation|
      next unless HTTP_METHODS.include?(method) && operation.is_a?(Hash)

      operation_id = operation.fetch("operationId")
      tag = operation.fetch("tags").first
      milestone, package, priority = assignment_for(tag, path, operation_id)
      previous = existing[operation_id]
      unchanged = previous && previous["method"] == method.upcase && previous["path"] == path
      baseline = operation_id == "getHealth"
      implementation_status = unchanged ? previous.fetch("implementation_status", "not_started") : (baseline ? "baseline_only" : "not_started")
      owner = unchanged ? previous.fetch("owner", "unassigned") : "unassigned"
      handler = unchanged ? previous["handler"] : nil
      tests = unchanged ? previous.fetch("tests", []) : []
      evidence = unchanged ? previous["evidence"] : nil

      if baseline
        owner = "platform" if owner == "unassigned"
        handler ||= "backend/src/routes/health.rs::healthz"
        tests = ["backend/tests/http.rs::healthz_returns_ok_and_request_id"] if tests.empty?
        evidence ||= "baseline commit 5e17fa3; cargo fmt, clippy and 4 Rust tests passed on 2026-08-04"
      end

      operations << {
        "operation_id" => operation_id,
        "method" => method.upcase,
        "path" => path,
        "primary_tag" => tag,
        "milestone" => milestone,
        "work_package" => package,
        "priority" => priority,
        "contract_status" => "frozen",
        "implementation_status" => implementation_status,
        "owner" => owner,
        "handler" => handler,
        "tests" => tests,
        "evidence" => evidence
      }
    end
  end

  operations.sort_by { |entry| [entry.fetch("milestone").sub("M", "").to_i, entry.fetch("primary_tag"), entry.fetch("path"), entry.fetch("method")] }
end


def markdown_for(payload)
  operations = payload.fetch("operations")
  tag_counts = Hash.new(0)
  milestone_counts = Hash.new(0)
  status_counts = Hash.new(0)
  operations.each do |entry|
    tag_counts[entry.fetch("primary_tag")] += 1
    milestone_counts[entry.fetch("milestone")] += 1
    status_counts[entry.fetch("implementation_status")] += 1
  end

  lines = []
  lines << "# OpenAPI operation 实现覆盖登记"
  lines << ""
  lines << "> 由 `ruby scripts/sync-operation-coverage.rb` 从 `openapi/openapi.yaml` 同步。"
  lines << "> 本表不是重复 TODO；每一行关联唯一工作包，完成状态以 JSON 中的实现、测试和证据字段为准。"
  lines << ""
  lines << "## 汇总"
  lines << ""
  lines << "- 契约操作：**#{operations.length}**"
  lines << "- 唯一 operationId：**#{operations.map { |entry| entry.fetch("operation_id") }.uniq.length}**"
  lines << "- 实现状态：#{status_counts.sort.map { |status, count| "`#{status}` #{count}" }.join("；")}"
  lines << "- 里程碑分配：#{milestone_counts.sort_by { |milestone, _| milestone.sub("M", "").to_i }.map { |milestone, count| "`#{milestone}` #{count}" }.join("；")}"
  lines << ""
  lines << "## 状态规则"
  lines << ""
  lines << "- `not_started`：只有冻结契约，尚无实现证据。"
  lines << "- `baseline_only`：已有骨架实现/测试，但尚未通过对应 v1 工作包全部门槛。"
  lines << "- `in_progress`：已指定 owner，handler 或测试尚不完整。"
  lines << "- `implemented`：handler 与领域测试完成，但跨库/E2E/安全证据尚未完整。"
  lines << "- `verified`：契约、权限、三数据库、客户端和专项测试证据全部完整。"
  lines << "- `blocked`：必须填写 blocker、owner、复查日期和解除条件（写在 evidence 字段或关联任务中）。"
  lines << ""
  lines << "## 逐项登记"
  lines << ""
  lines << "| operationId | Method | Path | Tag | Milestone / work package | Priority | Status | Owner |"
  lines << "|---|---:|---|---|---|---:|---|---|"
  operations.each do |entry|
    lines << "| `#{entry.fetch("operation_id")}` | `#{entry.fetch("method")}` | `#{entry.fetch("path")}` | #{entry.fetch("primary_tag")} | `#{entry.fetch("milestone")}` / `#{entry.fetch("work_package")}` | `#{entry.fetch("priority")}` | `#{entry.fetch("implementation_status")}` | `#{entry.fetch("owner")}` |"
  end
  lines << ""
  lines.join("\n")
end


document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
existing = load_existing
operations = operations_from(document, existing)
operation_ids = operations.map { |entry| entry.fetch("operation_id") }
raise "Duplicate operationId in OpenAPI" unless operation_ids.uniq.length == operation_ids.length
raise "Expected 172 operations, got #{operations.length}" unless operations.length == 172

payload = {
  "schema_version" => 1,
  "openapi_file" => "openapi/openapi.yaml",
  "expected_operations" => operations.length,
  "operations" => operations
}
json = JSON.pretty_generate(payload) + "\n"
markdown = markdown_for(payload)

if ARGV.include?("--check")
  abort "#{JSON_PATH} is stale; run ruby scripts/sync-operation-coverage.rb" unless File.file?(JSON_PATH) && File.read(JSON_PATH) == json
  abort "#{MARKDOWN_PATH} is stale; run ruby scripts/sync-operation-coverage.rb" unless File.file?(MARKDOWN_PATH) && File.read(MARKDOWN_PATH) == markdown
  puts "OpenAPI coverage OK: #{operations.length}/#{operations.length} operations assigned"
else
  File.write(JSON_PATH, json)
  File.write(MARKDOWN_PATH, markdown)
  puts "Synced #{operations.length} operations to todo coverage files"
end
