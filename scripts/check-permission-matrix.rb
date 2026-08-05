#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-04 / -12 and M03-AUTHZ-11
#
# Three-way permission contract check between:
#
#   * backend/src/authz/mod.rs  -> PERMISSION_REGISTRY (68 items, source of truth)
#   * openapi/openapi.yaml      -> operation-level x-permission
#   * docs/PERMISSION-MATRIX.md -> action tables + operation-level appendix
#
# Directions checked (all fail the build on drift):
#   * OpenAPI -> matrix doc: every distinct x-permission value must appear in
#     the matrix document (M00-CONTRACT-04; unchanged).
#   * OpenAPI -> registry: every x-permission except identity markers
#     (`public` / `authenticated`) must be a registered permission
#     (M03-AUTHZ-11; a handler can never be gated on an unregistered name).
#   * registry -> matrix doc: every PERMISSION_REGISTRY name must be mentioned
#     in the matrix document (tables or appendix) -- the doc is the source the
#     registry was taken from (M03-AUTHZ-11).
#   * matrix doc -> registry: every operation-level appendix row that is a real
#     permission must be registered (catches stale doc rows, M03-AUTHZ-11).
#   * operations whose security/CSRF shape contradicts the matrix rules are
#     reported (sessionCookie write without x-csrf: true, or x-csrf: true
#     without sessionCookie).
#
# Exits non-zero on any missing permission or security/CSRF contradiction.

require "yaml"

ROOT = File.expand_path("..", __dir__)
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
MATRIX_PATH = File.join(ROOT, "docs", "PERMISSION-MATRIX.md")
AUTHZ_SRC = File.join(ROOT, "backend", "src", "authz", "mod.rs")
HTTP_METHODS = %w[get post put patch delete head options trace].freeze
# 身份级标记：不是 resource.action 权限，只表示匿名/已登录
IDENTITY_MARKERS = %w[public authenticated].freeze

errors = []
warnings = []

document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
matrix = File.file?(MATRIX_PATH) ? File.read(MATRIX_PATH) : ""
errors << "missing #{MATRIX_PATH}" if matrix.empty?

# --- PERMISSION_REGISTRY（backend/src/authz/mod.rs，唯一事实来源） -----------

def parse_registry(src)
  block = src[/PERMISSION_REGISTRY: &\[Permission\] = &\[(.*?)\n\];/m, 1]
  return nil unless block

  block.scan(/name: "([^"]+)"/).flatten
end

registry_src = File.file?(AUTHZ_SRC) ? File.read(AUTHZ_SRC) : ""
if registry_src.empty?
  errors << "missing #{AUTHZ_SRC}"
  registry = []
else
  registry = parse_registry(registry_src)
  if registry.nil? || registry.length < 60
    errors << "cannot parse PERMISSION_REGISTRY from #{AUTHZ_SRC} (format changed? expected >= 60 entries, got #{registry ? registry.length : 'nil'})"
    registry = []
  end
end

# --- operation-level x-permission inventory ---------------------------------

permission_usage = {}
operations_by_permission = Hash.new { |hash, key| hash[key] = [] }
write_methods = %w[POST PUT PATCH DELETE]

document.fetch("paths", {}).each do |path, path_item|
  next unless path_item.is_a?(Hash)

  path_item.each do |method, operation|
    next unless HTTP_METHODS.include?(method) && operation.is_a?(Hash)

    operation_id = operation["operationId"]
    permission = operation["x-permission"]
    if permission.nil? || permission.to_s.empty?
      errors << "#{operation_id} (#{method.upcase} #{path}) is missing x-permission"
      next
    end

    permission_usage[permission] ||= 0
    permission_usage[permission] += 1
    operations_by_permission[permission] << operation_id

    # Matrix rule: session write requests require CSRF; Bearer-only requests
    # that never use cookies must not demand CSRF.
    schemes = operation.fetch("security", []).flat_map { |requirement| requirement.keys if requirement.is_a?(Hash) }
    session_cookie = schemes.include?("sessionCookie")
    csrf = operation["x-csrf"]
    if write_methods.include?(method.upcase) && session_cookie && csrf != true
      errors << "#{operation_id} (#{method.upcase} #{path}) is a sessionCookie write with x-csrf: #{csrf.inspect} (repair: x-csrf must be true)"
    end
    if csrf == true && !session_cookie && !schemes.empty?
      warnings << "#{operation_id} (#{method.upcase} #{path}) declares x-csrf: true without sessionCookie security"
    end
    if operation["x-bearer-only"] == true && session_cookie
      errors << "#{operation_id} (#{method.upcase} #{path}) is x-bearer-only but lists sessionCookie security (repair: choose one identity channel)"
    end
  end
end

# --- 三向比较：OpenAPI / 注册表 / 矩阵文档 -------------------------------------

# 1) OpenAPI -> 矩阵文档：每个 x-permission 都必须出现在矩阵文档中
permission_usage.keys.sort.each do |permission|
  next if matrix.include?(permission)

  examples = operations_by_permission[permission].first(3).join(", ")
  errors << "x-permission `#{permission}` (used by #{permission_usage[permission]} operation(s): #{examples}#{operations_by_permission[permission].length > 3 ? ', …' : ''}) is not registered in docs/PERMISSION-MATRIX.md " \
            "(repair: add a row to the operation-level register in docs/PERMISSION-MATRIX.md)"
end

# 2) OpenAPI -> 注册表：除身份级标记外，x-permission 必须是已注册权限
real_used = (permission_usage.keys - IDENTITY_MARKERS).sort
(real_used - registry).each do |permission|
  examples = operations_by_permission[permission].first(3).join(", ")
  errors << "x-permission `#{permission}` (used by #{permission_usage[permission]} operation(s): #{examples}) is not in PERMISSION_REGISTRY " \
            "(repair: register it in backend/src/authz/mod.rs, then re-check the matrix doc)"
end

# 3) 注册表 -> 矩阵文档：每个注册权限都必须在矩阵文档中出现
#    （单段权限如 openid 也要算；身份标记 S/B/O/R 是大写不会误匹配）
(registry - matrix.scan(/`([a-z0-9_]+(?:\.[a-z0-9_]+)*)`/).flatten).each do |permission|
  errors << "PERMISSION_REGISTRY `#{permission}` is not mentioned in docs/PERMISSION-MATRIX.md " \
            "(repair: add it to an action table or the operation-level register)"
end

# 4) 矩阵文档 -> 注册表：operation 级附录行的真实权限必须是已注册权限
doc_appendix_permissions = matrix.scan(/^\| `([a-z0-9_.]+)` \|/).flatten.uniq - IDENTITY_MARKERS
(doc_appendix_permissions - registry).each do |permission|
  errors << "docs/PERMISSION-MATRIX.md appendix claims `#{permission}` but it is not in PERMISSION_REGISTRY " \
            "(repair: remove the stale row or register the permission first)"
end

# --- Report -----------------------------------------------------------------

if errors.empty?
  puts "Permission matrix OK: #{permission_usage.length} distinct x-permission values all registered in docs/PERMISSION-MATRIX.md"
  puts "Coverage: #{permission_usage.values.sum} operations carry an x-permission"
  puts "Registry cross-check OK: #{registry.length} PERMISSION_REGISTRY items ↔ OpenAPI (#{real_used.length} real) ↔ matrix doc (#{doc_appendix_permissions.length} appendix rows)"
  warnings.each { |warning| puts "WARN: #{warning}" }
  exit 0
else
  warn "check-permission-matrix FAILED with #{errors.length} difference(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
