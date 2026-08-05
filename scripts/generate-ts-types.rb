#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-07 / -10 / -12
#
# Generates TypeScript types from openapi/openapi.yaml components.schemas into
# the frozen v1 output directory:
#
#   frontend/src/lib/api/generated/v1/{types.ts,enums.ts,index.ts,README.md}
#
# Rules:
#   * Deterministic output: `--check` regenerates in memory and fails when the
#     checked-in files drift (reproducible-diff gate; generated files are not
#     hand-editable).
#   * The v1/ directory is a frozen baseline: regeneration always writes to
#     v1/; a future v2 contract must use a new directory (see
#     docs/API-COMPATIBILITY.md).
#   * Handles objects, $ref, allOf (intersection), oneOf/anyOf (union),
#     arrays, enums, const, nullable `type: [x, null]`, additionalProperties.
#   * Emits valid, strict-mode-compilable TypeScript; it is self-contained and
#     not imported by the hand-written client until M00-FRONTEND-03.

require "yaml"

ROOT = File.expand_path("..", __dir__)
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
OUT_DIR = File.join(ROOT, "frontend", "src", "lib", "api", "generated", "v1")

warnings = []

document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
schemas = document.fetch("components", {}).fetch("schemas", {})

IDENTIFIER = /\A[a-zA-Z_$][a-zA-Z0-9_$]*\z/

def quote_key(name)
  name.match?(IDENTIFIER) ? name : name.inspect
end

def ref_schema_name(ref)
  ref.to_s.sub(/\A#\/components\/schemas\//, "")
end

def indent_spaces(level)
  "  " * level
end

# Convert a schema node to a TypeScript type expression.
def render_schema(node, schemas, level)
  return "unknown" unless node.is_a?(Hash)

  if node["$ref"]
    name = ref_schema_name(node["$ref"])
    return schemas.key?(name) ? name : "unknown /* unresolvable: #{node['$ref']} */"
  end

  if (composite = node["oneOf"] || node["anyOf"])
    parts = Array(composite).map { |part| render_schema(part, schemas, level) }.uniq
    return "unknown" if parts.empty?

    # Parenthesize only complex members so unions stay readable.
    rendered = parts.map do |part|
      part.match?(/[\s&|]/) ? "(#{part})" : part
    end
    return rendered.join(" | ")
  end

  if node["allOf"]
    parts = Array(node["allOf"]).map { |part| render_schema(part, schemas, level) }
    return parts.join(" & ")
  end

  types = Array(node["type"]).compact
  nullable = types.include?("null")
  types -= ["null"]

  base = case types.first
         when "object"
           render_object(node, schemas, level)
         when "array"
           items = node["items"]
           inner = items.is_a?(Hash) ? render_schema(items, schemas, level) : "unknown"
           "Array<#{inner}>"
         when "string"
           if node["enum"]
             render_enum_union(node["enum"])
           elsif node["const"]
             render_const(node["const"])
           else
             "string"
           end
         when "integer", "number"
           "number"
         when "boolean"
           "boolean"
         when "null", nil
           "null"
         else
           if node["properties"] || node["required"]
             render_object(node, schemas, level)
           elsif node["enum"]
             render_enum_union(node["enum"])
           elsif node["const"]
             render_const(node["const"])
           elsif node["allOf"]
             Array(node["allOf"]).map { |part| render_schema(part, schemas, level) }.join(" & ")
           elsif node["additionalProperties"].is_a?(Hash)
             "Record<string, #{render_schema(node['additionalProperties'], schemas, level)}>"
           else
             "Record<string, unknown>"
           end
         end

  nullable && base != "unknown" && base != "null" ? "#{base} | null" : base
end

def render_enum_union(values)
  literals = Array(values).compact.map(&:inspect)
  literals.empty? ? "string" : literals.uniq.join(" | ")
end

def render_const(value)
  value.inspect
end

def render_object(node, schemas, level)
  properties = node.fetch("properties", {})
  required = Array(node["required"])

  if properties.empty?
    additional = node["additionalProperties"]
    return "Record<string, #{render_schema(additional, schemas, level)}>" if additional.is_a?(Hash)

    return "Record<string, unknown>"
  end

  lines = properties.map do |name, property_schema|
    type = render_schema(property_schema, schemas, level + 1)
    suffix = required.include?(name) ? "" : "?"
    "#{indent_spaces(level + 1)}#{quote_key(name)}#{suffix}: #{type};"
  end

  "{\n#{lines.join("\n")}\n#{indent_spaces(level)}}"
end

# --- Schema definitions -----------------------------------------------------

definitions = []
enum_definitions = []
schema_order = []

schemas.each_key do |name|
  schema = schemas[name]
  next unless schema.is_a?(Hash)

  schema_order << name

  if schema["allOf"]
    parts = Array(schema["allOf"]).map { |part| render_schema(part, schemas, 0) }
    definitions << "export type #{name} = #{parts.join(' & ')};"
  elsif (schema["type"] == "object" || schema["properties"] || schema["required"]) && !(schema["properties"] || {}).empty?
    body = render_object(schema, schemas, 0)
    definitions << "export interface #{name} #{body}"
  else
    type = render_schema(schema, schemas, 0)
    if type == "Record<string, unknown>" && (schema["description"] || schema["content"])
      warnings << "components/schemas/#{name} has no type markers; emitted as Record<string, unknown> (contract bug — see check-openapi.rb)"
    end
    definitions << "export type #{name} = #{type};"
  end
end

# --- Named enum unions ------------------------------------------------------

def walk_enums(node, path, out)
  case node
  when Hash
    out << [path, node["enum"]] if node["enum"].is_a?(Array)
    node.each do |key, value|
      walk_enums(value, path + [key.to_s], out) if value.is_a?(Hash) || value.is_a?(Array)
    end
  when Array
    node.each_with_index do |value, index|
      walk_enums(value, path + [index.to_s], out) if value.is_a?(Hash) || value.is_a?(Array)
    end
  end
end

enum_entries = []
walk_enums(schemas, ["schemas"], enum_entries)
used_names = {}
enum_entries.each do |path, values|
  meaningful = path.reject { |segment| segment == "schemas" || segment.match?(/\A\d+\z/) || segment == "properties" || segment == "allOf" }
  raw_name = meaningful.join("_")
  raw_name = raw_name.gsub(/[^a-zA-Z0-9_]/, "_")
  candidate = raw_name.split("_").reject(&:empty?).each_with_index.map do |part, index|
    index.zero? ? part : part.capitalize
  end.join
  candidate = "Enum_#{candidate}" if candidate.empty?
  if used_names.key?(candidate)
    candidate = "#{candidate}_#{used_names[candidate]}"
  end
  used_names[candidate] = used_names.fetch(candidate, 0) + 1

  literals = Array(values).compact.map(&:inspect)
  next if literals.empty?

  enum_definitions << "export type #{candidate} = #{literals.uniq.join(' | ')};"
end

# --- File assembly ----------------------------------------------------------

header = <<~HEADER
  /**
   * AUTO-GENERATED — DO NOT EDIT.
   * Source: openapi/openapi.yaml (components.schemas)
   * Generator: scripts/generate-ts-types.rb
   *
   * Frozen baseline: contract version 1.0.0 (see README.md in this directory).
   * Regenerate with: ruby scripts/generate-ts-types.rb
   * Verify no drift with: ruby scripts/generate-ts-types.rb --check
   */
HEADER

types_ts = +""
types_ts << header
types_ts << "\n"
definitions.each { |definition| types_ts << "#{definition}\n" }
types_ts << "\n"

enums_ts = +""
enums_ts << header
enums_ts << "\n"
enum_definitions.each { |definition| enums_ts << "#{definition}\n" }

index_ts = <<~INDEX
  /**
   * AUTO-GENERATED — DO NOT EDIT. Frozen contract v1 entry point.
   * Not wired into the hand-written client until M00-FRONTEND-03.
   */
  export * from "./types";
  export * from "./enums";
INDEX

readme_md = <<~README
  # Frozen v1 API client types

  > 冻结基线：`openapi/openapi.yaml` v1.0.0（2026-08-04 冻结）。

  ## 内容

  - `types.ts` — 由 `components.schemas` 生成的 TypeScript interface/type。
  - `enums.ts` — 由 OpenAPI `enum` 声明的命名联合类型。
  - `index.ts` — 统一导出入口。

  ## 规则（M00-CONTRACT-07 / -10 / -11）

  - **禁止手工修改**：这些文件只能由
    `ruby scripts/generate-ts-types.rb` 重新生成；`--check` 模式在 CI 校验
    无漂移（可复现 diff 检查）。
  - **v1 为冻结版本**：本目录永远对应上一正式发布契约，不做就地变更。
    破坏性契约变更进入 v2 时必须生成新的
    `frontend/src/lib/api/generated/v2/`，保留 v1 目录供向后兼容编译与
    Fixture 测试。
  - 生成文件当前不被 hand-written client（`frontend/src/lib/api/client.ts`）
    引用；M00-FRONTEND-03 接入后方成为 API DTO 唯一类型来源。
README

files = {
  "types.ts" => types_ts,
  "enums.ts" => enums_ts,
  "index.ts" => index_ts,
  "README.md" => readme_md
}

# --- Reproducible-diff check / write ----------------------------------------

if ARGV.include?("--check")
  drift = []
  files.each do |name, content|
    path = File.join(OUT_DIR, name)
    if !File.file?(path)
      drift << "#{name} is missing (regenerate with ruby scripts/generate-ts-types.rb)"
    elsif File.read(path) != content
      drift << "#{name} is stale/drifted (regenerate with ruby scripts/generate-ts-types.rb)"
    end
  end
  if drift.empty?
    puts "TS types OK: frozen v1 files match the current contract (reproducible)"
    warnings.each { |warning| puts "WARN: #{warning}" }
    exit 0
  else
    warn "generate-ts-types --check FAILED:"
    drift.each { |message| warn "- #{message}" }
    exit 1
  end
else
  require "fileutils"
  FileUtils.mkdir_p(OUT_DIR)
  files.each do |name, content|
    File.write(File.join(OUT_DIR, name), content)
  end
  puts "Wrote frozen v1 TS types (#{schema_order.length} schemas, #{enum_definitions.length} named enums) to frontend/src/lib/api/generated/v1/"
  warnings.each { |warning| puts "WARN: #{warning}" }
end
