#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-06 / -12
#
# Generates the write-operation requirements checklist and fails on
# undeclared requirements:
#
#   * Idempotency-Key: every POST must declare `Idempotency-Key` (or a
#     documented exemption). PATCH/PUT must carry `If-Match` or a
#     `version`/`expected_*_version` request-body field.
#   * Cache-Control: every write response that can carry session, management
#     or personal data must declare `Cache-Control: private, no-store`
#     (docs/FRONTEND.md M00-FRONTEND-06). No operation declares one today.
#   * Audit: sensitive writes (admin/moderation/shop/activity/user/oauth/
#     download/billing classes) require audit logging per
#     docs/PERMISSION-MATRIX.md; enforcement lives in M01-AUDIT and is
#     reported here because openapi.yaml has no x-audit extension yet.
#   * CSRF: sessionCookie writes must declare x-csrf: true.
#
# Output lists exact operationIds with a repair entry; exits non-zero while
# any undeclared requirement exists.

require "yaml"

ROOT = File.expand_path("..", __dir__)
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
WRITE_METHODS = %w[post put patch delete].freeze

# Permission classes that make a write "sensitive" (audit/cache-control).
SENSITIVE_PREFIXES = %w[
  admin. moderation. shop. activity. user. role. tag. board. oauth_client.
  download_billing. session.revoke_own attachment.upload authenticated
].freeze

errors = []
rows = []

document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
idempotency_ref = "#/components/parameters/IdempotencyKey"
if_match_ref = "#/components/parameters/IfMatch"

def parameter_names(operation)
  names = []
  Array(operation["parameters"]).each do |parameter|
    if parameter.is_a?(Hash) && parameter["$ref"]
      names << parameter["$ref"]
    elsif parameter.is_a?(Hash) && parameter["name"]
      names << parameter["name"]
    end
  end
  names
end

def body_property_names(operation, document)
  schema_ref = operation.dig("requestBody", "content", "application/json", "schema")
  return [] unless schema_ref.is_a?(Hash)

  schema = if schema_ref["$ref"].is_a?(String)
             document.dig(*schema_ref["$ref"].sub(/\A#\//, "").split("/"))
           else
             schema_ref
           end
  return [] unless schema.is_a?(Hash)

  schema.fetch("properties", {}).keys
end

def success_cache_control_declared?(operation, document)
  %w[200 201 202 204].any? do |status|
    response = operation.dig("responses", status)
    next false unless response.is_a?(Hash)

    # Resolve `$ref` responses (e.g. GenericSuccess) before inspecting headers.
    if response["$ref"].is_a?(String)
      response = document.dig(*response["$ref"].sub(/\A#\//, "").split("/"))
    end
    next false unless response.is_a?(Hash)

    headers = response["headers"]
    next false unless headers.is_a?(Hash)

    headers.keys.any? { |name| name.casecmp("Cache-Control").zero? }
  end
end

document.fetch("paths", {}).each do |path, path_item|
  next unless path_item.is_a?(Hash)

  path_item.each do |method, operation|
    next unless WRITE_METHODS.include?(method) && operation.is_a?(Hash)

    operation_id = operation["operationId"]
    method = method.upcase
    permission = operation["x-permission"].to_s
    parameters = parameter_names(operation)
    body_fields = body_property_names(operation, document)
    schemes = operation.fetch("security", []).flat_map { |req| req.keys if req.is_a?(Hash) }
    session_write = schemes.include?("sessionCookie")

    # --- Idempotency -------------------------------------------------------
    has_idempotency_key = parameters.any? { |name| name == "Idempotency-Key" || name.include?("IdempotencyKey") }
    idempotency_required = method == "POST"
    idempotency_ok = !idempotency_required || has_idempotency_key

    # --- If-Match / version ------------------------------------------------
    has_if_match = parameters.any? { |name| name == "If-Match" || name.include?("IfMatch") }
    has_version_field = body_fields.any? { |field| field == "version" || field.start_with?("expected_") }
    version_control_required = %w[PATCH PUT].include?(method)
    version_control_ok = !version_control_required || has_if_match || has_version_field

    # --- Cache-Control -----------------------------------------------------
    sensitive = SENSITIVE_PREFIXES.any? { |prefix| permission.start_with?(prefix) }
    cache_required = sensitive || method != "DELETE"
    cache_declared = success_cache_control_declared?(operation, document)
    cache_ok = !cache_required || cache_declared

    # --- Audit -------------------------------------------------------------
    audit_required = sensitive

    # --- CSRF --------------------------------------------------------------
    csrf_ok = !session_write || operation["x-csrf"] == true

    rows << {
      id: operation_id, method: method, path: path, permission: permission,
      idempotency_required: idempotency_required, idempotency_ok: idempotency_ok,
      version_required: version_control_required, version_ok: version_control_ok,
      cache_required: cache_required, cache_declared: cache_declared,
      audit_required: audit_required, csrf_ok: csrf_ok
    }

    if idempotency_required && !idempotency_ok
      errors << "#{operation_id} (#{method} #{path}) is a POST without Idempotency-Key (repair: add `$ref: #{idempotency_ref}` parameter or record a documented exemption)"
    end
    if version_control_required && !version_control_ok
      errors << "#{operation_id} (#{method} #{path}) modifies an existing resource without If-Match or a version field (repair: add `$ref: #{if_match_ref}` or an `expected_*_version` body field)"
    end
    if cache_required && !cache_declared
      errors << "#{operation_id} (#{method} #{path}) is a write without `Cache-Control: private, no-store` on its success response (repair: declare the response header in openapi.yaml or waive per endpoint)"
    end
    if session_write && operation["x-csrf"] != true
      errors << "#{operation_id} (#{method} #{path}) is a sessionCookie write with x-csrf: #{operation['x-csrf'].inspect} (repair: x-csrf must be true)"
    end
  end
end

# --- Checklist --------------------------------------------------------------

puts "写操作需求清单 (M00-CONTRACT-06) — #{rows.length} 个写操作"
puts "字段: 幂等(Idempotency-Key) | 版本控制(If-Match/expected_*_version) | Cache-Control(no-store) | 审计 | CSRF"
rows.sort_by { |row| [row[:method], row[:path]] }.each do |row|
  idem = row[:idempotency_required] ? (row[:idempotency_ok] ? "R:OK" : "R:MISS") : "-"
  ver = row[:version_required] ? (row[:version_ok] ? "R:OK" : "R:MISS") : "-"
  ccache = row[:cache_required] ? (row[:cache_declared] ? "R:OK" : "R:MISS") : "-"
  audit = row[:audit_required] ? "req" : "-"
  csrf = row[:csrf_ok] ? "ok" : "MISS"
  puts format("  %-44s %-6s %-6s %-6s %-5s %-6s %s", row[:id], row[:method], idem, ver, ccache, audit, csrf)
end

# --- Report -----------------------------------------------------------------

puts
idempotency_gaps = errors.grep(/without Idempotency-Key/)
version_gaps = errors.grep(/If-Match or a version/)
cache_gaps = errors.grep(/Cache-Control/)
csrf_gaps = errors.grep(/x-csrf/)

puts "汇总: 写操作 #{rows.length}；幂等缺口 #{idempotency_gaps.length}；版本控制缺口 #{version_gaps.length}；Cache-Control 缺口 #{cache_gaps.length}；CSRF 缺口 #{csrf_gaps.length}"
puts "审计: 无 x-audit 扩展；敏感写操作(admin/moderation/shop/activity/user/oauth 等)的审计要求由 docs/PERMISSION-MATRIX.md 规定、M01-AUDIT 实施，暂不作为契约级失败"

if errors.empty?
  puts "write-contract OK: all write operations declare their required contract"
  exit 0
else
  warn "check-write-contract FAILED with #{errors.length} undeclared requirement(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
