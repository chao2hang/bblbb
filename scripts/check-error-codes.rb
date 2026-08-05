#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-03 / -12
#
# Bidirectional comparison of OpenAPI Problem codes and
# docs/ERROR-CODES.md (the stable machine-code registry):
#
#   * docs -> internal consistency (unique snake_case codes, valid HTTP status).
#   * openapi.yaml -> every `code` enum / `x-error-code(s)` extension value
#     extracted from the schema tree.
#   * openapi codes absent from docs            -> FAIL (docs missing a row).
#   * docs codes not enumerated in openapi.yaml -> FAIL (registry is not bound
#     to the contract; Problem.code is `type: string` today).
#
# Differences are reported with the exact code and a repair entry
# (M00-CONTRACT-12). Exits non-zero on any difference.

require "yaml"

ROOT = File.expand_path("..", __dir__)
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
ERROR_CODES_PATH = File.join(ROOT, "docs", "ERROR-CODES.md")

errors = []
warnings = []

# --- docs/ERROR-CODES.md ---------------------------------------------------

rows = []
if File.file?(ERROR_CODES_PATH)
  File.readlines(ERROR_CODES_PATH, chomp: true).each_with_index do |line, index|
    match = line.match(/\A\|\s*`([a-z0-9_]+)`\s*\|\s*(\d{3})\s*\|/)
    next unless match

    rows << { code: match[1], http: match[2], line: index + 1 }
  end
else
  abort "check-error-codes: missing #{ERROR_CODES_PATH}"
end

if rows.empty?
  errors << "docs/ERROR-CODES.md has no parseable `code | HTTP` table rows"
else
  seen = {}
  rows.each do |row|
    if seen.key?(row[:code])
      errors << "docs/ERROR-CODES.md line #{row[:line]}: duplicate code `#{row[:code]}` (first at line #{seen[row[:code]]})"
    else
      seen[row[:code]] = row[:line]
    end
    unless row[:code].match?(/\A[a-z][a-z0-9_]*\z/)
      errors << "docs/ERROR-CODES.md line #{row[:line]}: code `#{row[:code]}` is not lowercase snake_case"
    end
    unless (100..599).cover?(row[:http].to_i)
      errors << "docs/ERROR-CODES.md line #{row[:line]}: code `#{row[:code]}` has out-of-range HTTP status #{row[:http]}"
    end
  end
end
doc_codes = rows.map { |row| row[:code] }.uniq

# --- openapi.yaml code extraction -------------------------------------------

document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)

def walk(node, path, &block)
  case node
  when Hash
    block.call(node, path)
    node.each { |key, value| walk(value, path + [key.to_s], &block) if value.is_a?(Hash) || value.is_a?(Array) }
  when Array
    node.each_with_index { |value, index| walk(value, path + [index.to_s], &block) if value.is_a?(Hash) || value.is_a?(Array) }
  end
end

openapi_codes = {}
walk(document, []) do |node, path|
  if node.key?("code") && node["code"].is_a?(Hash) && node["code"].key?("enum")
    node["code"]["enum"].each do |value|
      next unless value.is_a?(String) && !value.empty?

      openapi_codes[value] ||= []
      openapi_codes[value] << "schema #{path.join('/')}.code.enum"
    end
  end
  %w[x-error-code x-error-codes].each do |extension|
    next unless node.key?(extension)

    Array(node[extension]).each do |value|
      next unless value.is_a?(String) && !value.empty?

      openapi_codes[value] ||= []
      openapi_codes[value] << "#{extension} at #{path.join('/')}"
    end
  end
end

# --- Bidirectional comparison -----------------------------------------------

problem_schema = document.dig("components", "schemas", "Problem")
if problem_schema.nil?
  errors << "openapi.yaml has no components/schemas/Problem; every problem+json response references it (repair: openapi.yaml)"
else
  code_schema = problem_schema.dig("properties", "code")
  if code_schema.nil?
    errors << "Problem.code property is missing (repair: openapi.yaml Problem.properties.code)"
  elsif code_schema["enum"].is_a?(Array)
    warnings << "Problem.code now enumerates #{code_schema['enum'].length} code(s) in openapi.yaml"
  end
end

openapi_codes.keys.sort.each do |code|
  next if doc_codes.include?(code)

  errors << "OpenAPI code `#{code}` (seen at #{openapi_codes[code].join(', ')}) is missing from docs/ERROR-CODES.md (repair: add a row: `| `#{code}` | ... |`)"
end

missing_from_openapi = doc_codes.reject { |code| openapi_codes.key?(code) }
if missing_from_openapi.any?
  errors << "docs/ERROR-CODES.md defines #{missing_from_openapi.length} code(s) that openapi.yaml does not enumerate: #{missing_from_openapi.join(', ')} " \
            "(repair: bound the registry in openapi.yaml — add an enum or x-error-codes to Problem.code; semantic change, needs main-agent approval)"
end

# --- Report -----------------------------------------------------------------

if errors.empty?
  puts "Error codes OK: #{doc_codes.length} documented codes, #{openapi_codes.length} enumerated in OpenAPI, no missing/spelling/deprecation diffs"
  warnings.each { |warning| puts "WARN: #{warning}" }
  exit 0
else
  warn "check-error-codes FAILED with #{errors.length} difference(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
