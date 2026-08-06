#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-01 / -02 / -08 / -12
#
# OpenAPI 3.1 structural validation for openapi/openapi.yaml:
#   * YAML loads with aliases (Psych 3.x, Ruby 2.6 compatible).
#   * Every internal "$ref" resolves against the same document.
#   * The 173 operations each carry tags / security / x-permission / x-csrf /
#     responses and a unique operationId.
#   * components.schemas are structurally sane (no response-shaped schemas,
#     no empty enum members).
#   * The operation coverage manifest (todo/openapi-operation-coverage.json)
#     has a row for every operation with all required fields present.
#
# Every failure names the operationId/schema/path and a repair entry
# (M00-CONTRACT-12). Exits non-zero on any structural violation.

require "json"
require "yaml"

ROOT = File.expand_path("..", __dir__)
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
COVERAGE_PATH = File.join(ROOT, "todo", "openapi-operation-coverage.json")
HTTP_METHODS = %w[get post put patch delete head options trace].freeze
REQUIRED_COVERAGE_FIELDS = %w[
  operation_id method path primary_tag milestone work_package priority
  contract_status implementation_status owner handler tests evidence
].freeze

errors = []
warnings = []

# --- YAML load -------------------------------------------------------------

begin
  document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
rescue Psych::Exception => e
  abort "check-openapi: cannot parse #{OPENAPI_PATH}: #{e.class}: #{e.message}"
end
abort "check-openapi: #{OPENAPI_PATH} is empty or not a mapping" unless document.is_a?(Hash)

# --- Document-level structure ----------------------------------------------

version = document["openapi"]
if version.nil?
  errors << "openapi.yaml is missing the `openapi` version key (must declare 3.1.x)"
elsif !version.to_s.start_with?("3.1")
  errors << "openapi.yaml declares #{version.inspect}; M00-CONTRACT-01 requires the OpenAPI 3.1 dialect"
end

%w[info paths components].each do |top|
  errors << "openapi.yaml is missing top-level `#{top}`" unless document.key?(top)
end
errors << "openapi.yaml info.version must be present" unless document.dig("info", "version").is_a?(String)
errors << "openapi.yaml servers is missing" unless document.key?("servers") && document["servers"].is_a?(Array)

dialect = document["jsonSchemaDialect"]
warnings << "jsonSchemaDialect is not declared; the 3.1 default schema dialect applies" if dialect.nil?

# --- Internal $ref resolution ----------------------------------------------

# Collect every "$ref" location so a broken pointer is reported with context.
def walk_nodes(node, path, &block)
  case node
  when Hash
    block.call(node, path)
    node.each do |key, value|
      walk_nodes(value, path + [key.to_s], &block) if value.is_a?(Hash) || value.is_a?(Array)
    end
  when Array
    node.each_with_index do |value, index|
      walk_nodes(value, path + [index.to_s], &block) if value.is_a?(Hash) || value.is_a?(Array)
    end
  end
end

def resolve_pointer(doc, pointer)
  return nil unless pointer.is_a?(String) && pointer.start_with?("#/")

  segments = pointer.sub(/\A#\//, "").split("/").map { |segment| segment.gsub("~1", "/").gsub("~0", "~") }
  node = doc
  segments.each do |segment|
    return nil unless node.is_a?(Hash) && node.key?(segment)

    node = node[segment]
  end
  node
end

def ref_context(path)
  compact = path.join("/")
  compact[0, 180]
end

walk_nodes(document, []) do |node, path|
  next unless node.key?("$ref")

  ref = node["$ref"]
  if !ref.is_a?(String) || !ref.start_with?("#/")
    errors << "openapi.yaml #{ref_context(path)}: external or non-pointer $ref #{ref.inspect} is not supported by the internal-ref contract"
    next
  end
  target = resolve_pointer(document, ref)
  errors << "openapi.yaml #{ref_context(path)}: $ref #{ref} does not resolve" if target.nil?
end

# --- Operations ------------------------------------------------------------

operations = {}
document.fetch("paths", {}).each do |path, path_item|
  next unless path_item.is_a?(Hash)

  path_item.each do |method, operation|
    next unless HTTP_METHODS.include?(method) && operation.is_a?(Hash)

    operation_id = operation["operationId"]
    location = "#{method.upcase} #{path}"
    if operation_id.nil? || operation_id.to_s.empty?
      errors << "#{location} is missing operationId (repair: add operationId to openapi.yaml)"
      next
    end
    if operations.key?(operation_id)
      errors << "duplicate operationId #{operation_id} at #{location} and #{operations[operation_id][:location]} (repair: rename one operationId)"
      next
    end

    # M00-CONTRACT-02: every operation must declare tags / security /
    # x-permission / x-csrf / responses.
    errors << "#{operation_id} (#{location}) is missing tags" unless operation["tags"].is_a?(Array) && !operation["tags"].empty?
    errors << "#{operation_id} (#{location}) is missing security" unless operation.key?("security") && operation["security"].is_a?(Array)
    permission = operation["x-permission"]
    errors << "#{operation_id} (#{location}) is missing x-permission" unless permission.is_a?(String) && !permission.empty?
    csrf = operation["x-csrf"]
    errors << "#{operation_id} (#{location}) has non-boolean x-csrf #{csrf.inspect}" unless [true, false].include?(csrf)
    responses = operation["responses"]
    errors << "#{operation_id} (#{location}) responses must be a non-empty mapping" unless responses.is_a?(Hash) && !responses.empty?
    if responses.is_a?(Hash)
      responses.each_key do |status|
        next if status == "default" || status.to_s.match?(/\A\d{3}\z/)

        errors << "#{operation_id} (#{location}) has invalid response status key #{status.inspect}"
      end
    end

    # Unknown security scheme names must fail.
    operation.fetch("security", []).each do |requirement|
      next unless requirement.is_a?(Hash)

      requirement.each_key do |scheme|
        next if document.dig("components", "securitySchemes", scheme)

        errors << "#{operation_id} (#{location}) references unknown security scheme #{scheme}"
      end
    end

    operations[operation_id] = {
      location: location,
      method: method.upcase,
      path: path,
      tag: operation.fetch("tags", []).first,
      permission: permission,
      csrf: csrf
    }
  end
end

errors << "expected 184 operations, got #{operations.length} (repair: freeze a new baseline or fix a duplicate)" unless operations.length == 184

# Every declared tag must be registered in the top-level tags list.
registered_tags = document.fetch("tags", []).map { |tag| tag["name"] }
operations.each_value do |operation|
  next if registered_tags.include?(operation[:tag])

  errors << "#{operation[:tag].inspect} tag used by #{operation[:location]} is not registered in top-level tags"
end

# --- components.schemas structural sanity ----------------------------------

schemas = document.dig("components", "schemas")
unless schemas.is_a?(Hash)
  abort "check-openapi: components.schemas is missing"
end

# A schema component root must not look like a response/request object
# (copy/paste of a components/responses entry into components/schemas).
schemas.each do |schema_name, schema|
  next unless schema.is_a?(Hash)

  schema_markers = %w[type $ref allOf anyOf oneOf enum const properties items not]
  if !schema_markers.any? { |marker| schema.key?(marker) } && (schema.key?("content") || schema.key?("description"))
    errors << "components/schemas/#{schema_name} is not a schema (has `content`/`description` without a type marker); repair: move it to components/responses or give it a real type"
  end
end

# No enum may contain an empty member anywhere in the schema tree.
walk_nodes(schemas, ["components", "schemas"]) do |node, path|
  next unless node.is_a?(Hash) && node.key?("enum") && node["enum"].is_a?(Array)

  node["enum"].each do |member|
    if member.nil?
      errors << "#{ref_context(path)} has an empty/null enum member (repair: remove the blank `- ` line in openapi.yaml)"
    elsif member.is_a?(String) && member.empty?
      errors << "#{ref_context(path)} has an empty enum member (repair: remove the blank `- ` line in openapi.yaml)"
    end
  end
end

# --- Coverage manifest (M00-CONTRACT-08) ------------------------------------

if File.file?(COVERAGE_PATH)
  begin
    coverage = JSON.parse(File.read(COVERAGE_PATH))
    rows = coverage.fetch("operations")
    errors << "coverage manifest expected_operations must be 184, got #{coverage.fetch('expected_operations')}" unless coverage.fetch("expected_operations") == 184
    errors << "coverage manifest has #{rows.length} rows, expected 184" unless rows.length == 184

    seen = {}
    rows.each_with_index do |entry, index|
      missing = REQUIRED_COVERAGE_FIELDS.reject { |field| entry.key?(field) }
      unless missing.empty?
        errors << "coverage manifest row #{index + 1} missing fields: #{missing.join(', ')}"
        next
      end

      operation_id = entry["operation_id"]
      errors << "coverage manifest row #{index + 1} duplicates operation #{operation_id}" if seen.key?(operation_id)
      seen[operation_id] = true

      contract = operations[operation_id]
      if contract.nil?
        errors << "coverage manifest row #{index + 1}: #{operation_id} is absent from openapi.yaml"
      elsif contract[:method] != entry["method"] || contract[:path] != entry["path"]
        errors << "#{operation_id}: coverage method/path #{entry['method']} #{entry['path']} disagrees with OpenAPI #{contract[:method]} #{contract[:path]}"
      end
    end
    operations.each_key do |operation_id|
      errors << "coverage manifest is missing operation #{operation_id}" unless seen.key?(operation_id)
    end
  rescue JSON::ParserError => e
    errors << "coverage manifest is not valid JSON: #{e.message}"
  end
else
  errors << "coverage manifest #{COVERAGE_PATH} is missing"
end

# --- Report ----------------------------------------------------------------

if errors.empty?
  puts "OpenAPI OK: #{operations.length} operations, unique operationIds, internal $refs resolve, schemas structurally sound"
  puts "Tags: #{registered_tags.length} registered; operations all declare tags/security/x-permission/x-csrf/responses"
  puts "Manifest: #{operations.length}/#{operations.length} coverage rows present with all fields"
  warnings.each { |warning| puts "WARN: #{warning}" }
  exit 0
else
  warn "check-openapi FAILED with #{errors.length} error(s):"
  errors.each { |error| warn "- #{error}" }
  warn "修复入口：openapi.yaml 语义变更需主代理批准；文档/脚本差异可自行修复。"
  exit 1
end
