#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-09 / -12
#
# Bidirectional comparison of the axum route registry (backend/src/**/*.rs)
# against the operation coverage manifest (todo/openapi-operation-coverage.json,
# itself synced from openapi/openapi.yaml):
#
#   * contract operations with NO axum route -> "契约无实现"
#     (hard failure when the operation claims baseline_only/in_progress/
#      implemented/verified/blocked; informational for not_started);
#   * axum routes with NO contract operation -> "实现无契约"
#     (hard failure, except the documented non-contract endpoints
#      /readyz and /api/v1/openapi.json);
#   * method/path drift between the two registries.
#
# Normalization: `{param}` braces are collapsed and the `/api/v1` prefix is
# stripped on both sides. Reads backend/src read-only. Failing output names
# operationIds/paths with a repair entry.

require "json"
require "yaml"

ROOT = File.expand_path("..", __dir__)
BACKEND_SRC = File.join(ROOT, "backend", "src")
COVERAGE_PATH = File.join(ROOT, "todo", "openapi-operation-coverage.json")
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
ROUTE_METHODS = %w[get post put patch delete head options trace].freeze

# Documented endpoints that intentionally live outside the 172-operation
# contract (docs/API.md §1).
# `robots.txt` / `sitemap.xml` 是 Web 标准端点（搜索引擎/抓取器直接访问），
# 不进入 /api/v1 契约；由 M08-FEEDS 路由提供并记录在 docs/CRAWLER-POLICY.md §7。
# M12/M13 内部管理/运营端点（Marketplace 审批/对账/紧急停用、Plugin 管理、
# /metrics、Marketplace 确认页视图）为领域管理接口，不在冻结 193-op 契约内，
# 记录于 docs/MARKETPLACE.md §12 与 docs/PLUGIN.md。
DOCUMENTED_NON_CONTRACT = {
  "readyz" => %w[GET],
  "openapi.json" => %w[GET],
  "robots.txt" => %w[GET],
  "sitemap.xml" => %w[GET],
  "metrics" => %w[GET],
  "marketplace/checkout-intents/{p}" => %w[GET],
  "admin/marketplace/clients/{p}" => %w[GET PATCH],
  "admin/marketplace/clients/{p}/emergency-disable" => %w[POST],
  "admin/marketplace/offers" => %w[GET],
  "admin/marketplace/reconciliation/run" => %w[POST],
  "admin/marketplace/refunds/{p}/retry" => %w[POST],
  "admin/marketplace/webhook-deliveries" => %w[GET],
  "admin/marketplace/webhook-deliveries/{p}/replay" => %w[POST],
  "admin/plugins" => %w[GET POST],
  "admin/plugins/{p}" => %w[GET DELETE],
  "admin/plugins/{p}/disable" => %w[POST],
  "admin/plugins/{p}/enable" => %w[POST],
  "admin/plugins/{p}/metrics" => %w[GET],
  "admin/plugins/{p}/settings" => %w[PATCH],
  "admin/plugins/capabilities" => %w[GET]
}.freeze

def normalize_path(path)
  normalized = path.to_s
                  .sub(%r{\A/api/v1}, "")
                  .gsub(/\{[a-zA-Z_][a-zA-Z0-9_]*\}/, "{p}")
                  .gsub(%r{\A/+}, "")
                  .chomp("/")
  normalized.empty? ? "/" : normalized
end

def line_number(text, index)
  text[0...index].count("\n") + 1
end

# Extract `{ path:, methods:, file:, line: }` from one Rust file.
def extract_routes(text, file)
  routes = []
  offset = 0
  while (match = /\.route\(/.match(text, offset))
    start = match.offset(0).first + ".route(".length
    depth = 1
    index = start
    while depth.positive? && index < text.length
      case text[index]
      when "(" then depth += 1
      when ")" then depth -= 1
      end
      index += 1
    end
    call = text[start...index - 1]
    path = call[/"([^"]+)"/, 1]
    if path
      methods = call.scan(/\b(#{ROUTE_METHODS.join('|')})\s*\(/).flatten.map(&:upcase).uniq
      routes << { path: path, methods: methods, file: file, line: line_number(text, match.offset(0).first) }
    end
    offset = match.offset(0).last
  end
  routes
end

# --- Load backend routes ------------------------------------------------------

backend_routes = []
Dir[File.join(BACKEND_SRC, "**", "*.rs")].sort.each do |file|
  next if file.include?("/target/")

  text = File.read(file)
  extract_routes(text, file).each { |route| backend_routes << route }
end

# Merge routes by normalized path.
backend_by_path = Hash.new { |hash, key| hash[key] = [] }
backend_routes.each do |route|
  backend_by_path[normalize_path(route[:path])] << route
end

# --- Load coverage / contract -------------------------------------------------

coverage = JSON.parse(File.read(COVERAGE_PATH))
operations = coverage.fetch("operations")
index = {}
operations.each { |entry| index[entry.fetch("operation_id")] = entry }

errors = []
informational = []

contract_by_path = Hash.new { |hash, key| hash[key] = [] }
operations.each do |entry|
  contract_by_path[normalize_path(entry.fetch("path"))] << entry
end

# --- Direction 1: contract operations without a route -------------------------

operations.sort_by { |entry| entry.fetch("path") }.each do |entry|
  operation_id = entry.fetch("operation_id")
  method = entry.fetch("method")
  path = normalize_path(entry.fetch("path"))
  status = entry.fetch("implementation_status")

  registered = backend_by_path[path]
  methods_on_path = registered.flat_map { |route| route[:methods] }.uniq
  location = registered.empty? ? nil : "#{registered.first[:file]}:#{registered.first[:line]}"

  if registered.empty?
    if %w[baseline_only in_progress implemented verified blocked].include?(status)
      errors << "#{operation_id} (#{method} #{entry.fetch('path')}) claims #{status} but has no axum route (repair: register the route in backend/src/routes/*.rs or set implementation_status back to not_started)"
    else
      informational << "#{operation_id} (#{method} #{entry.fetch('path')}) has no axum route yet (status: #{status})"
    end
    next
  end

  unless methods_on_path.include?(method)
    errors << "#{operation_id} (#{method} #{entry.fetch('path')}) route exists at #{location} but exposes #{methods_on_path.join('/')} (repair: add the missing method handler or fix the coverage entry)"
  end
end

# --- Direction 2: routes without a contract operation --------------------------

backend_by_path.keys.sort.each do |path|
  backend_by_path[path].each do |route|
    contract_matches = contract_by_path[path]
    methods = route[:methods].empty? ? ["(any)"] : route[:methods]
    missing_methods = methods.reject do |method|
      contract_matches.any? { |entry| entry.fetch("method") == method }
    end
    next if missing_methods.empty?

    key = "#{path} (#{missing_methods.join('/')})"
    if DOCUMENTED_NON_CONTRACT.key?(path) && (methods - DOCUMENTED_NON_CONTRACT[path]).empty?
      informational << "#{route[:file]}:#{route[:line]} registers documented non-contract endpoint /#{path} (#{methods.join('/')})"
    else
      errors << "#{route[:file]}:#{route[:line]} registers /#{path} (#{missing_methods.join('/')}) with no contract operation (repair: add the operation to openapi.yaml or register the route in the coverage manifest)"
    end
  end
end

# --- Report -------------------------------------------------------------------

puts "axum 路由 vs 覆盖清单双向比对 (M00-CONTRACT-09)"
puts "后端路由: #{backend_routes.length} 条; 契约操作: #{operations.length} 条"
puts
informational.each { |message| puts "INFO: #{message}" }

if errors.empty?
  puts
  puts "route-coverage OK: no contract-without-implementation (for claimed statuses), no implementation-without-contract, no method/path drift"
  exit 0
else
  warn
  warn "check-route-coverage FAILED with #{errors.length} difference(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
