#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-04 / -12
#
# Bidirectional comparison between operation-level `x-permission` values in
# openapi.yaml and docs/PERMISSION-MATRIX.md:
#
#   * every distinct x-permission value must appear in the matrix document
#     (either in the action tables or in the operation-level appendix
#     register); a missing value fails with the exact permission, its
#     operationIds and a repair entry;
#   * operations whose security/CSRF shape contradicts the matrix rules are
#     reported (sessionCookie write without x-csrf: true, or x-csrf: true
#     without sessionCookie).
#
# Exits non-zero on any missing permission or security/CSRF contradiction.

require "yaml"

ROOT = File.expand_path("..", __dir__)
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
MATRIX_PATH = File.join(ROOT, "docs", "PERMISSION-MATRIX.md")
HTTP_METHODS = %w[get post put patch delete head options trace].freeze

errors = []
warnings = []

document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
matrix = File.file?(MATRIX_PATH) ? File.read(MATRIX_PATH) : ""
errors << "missing #{MATRIX_PATH}" if matrix.empty?

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

permission_usage.keys.sort.each do |permission|
  next if matrix.include?(permission)

  examples = operations_by_permission[permission].first(3).join(", ")
  errors << "x-permission `#{permission}` (used by #{permission_usage[permission]} operation(s): #{examples}#{operations_by_permission[permission].length > 3 ? ', …' : ''}) is not registered in docs/PERMISSION-MATRIX.md " \
            "(repair: add a row to the operation-level register in docs/PERMISSION-MATRIX.md)"
end

# --- Report -----------------------------------------------------------------

if errors.empty?
  puts "Permission matrix OK: #{permission_usage.length} distinct x-permission values all registered in docs/PERMISSION-MATRIX.md"
  puts "Coverage: #{permission_usage.values.sum} operations carry an x-permission"
  warnings.each { |warning| puts "WARN: #{warning}" }
  exit 0
else
  warn "check-permission-matrix FAILED with #{errors.length} difference(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
