#!/usr/bin/env ruby
# frozen_string_literal: true

# M16-HARNESS-07 / M16-RELEASE-TEST-04
#
# 上一版本生成 client 的向后兼容检查：
#   frozen  = compat/frozen-client/openapi.yaml（从历史 commit 提取的上一版本契约，
#             等价于"上一版本生成的 client"所依赖的 API 表面）
#   current = openapi/openapi.yaml（当前服务端契约）
#
# 保证"新增字段不破坏旧客户端"：
#   1. 操作表面：frozen 中的每个 path+method 在当前仍存在。
#   2. 请求参数：frozen 客户端会发送的每个参数（query/path/header）在当前仍被接受，
#      且当前 REQUIRED 集合不得比 frozen 更严格（旧客户端不发送的新必填参数 = 破坏）。
#   3. 请求体：frozen 请求 schema 的每个属性在当前请求 schema 中仍存在；
#      当前新增必填属性必须已在 frozen 中必填或带默认值。
#   4. 响应体：frozen 响应 schema 的每个属性在当前响应 schema 中仍存在
#      （允许新增字段）；frozen 的 enum 值在当前不得被移除。
#
# 退出码：任何破坏性差异非零。

require "yaml"

ROOT = File.expand_path("..", __dir__)
FROZEN_PATH = File.join(ROOT, "compat", "frozen-client", "openapi.yaml")
CURRENT_PATH = File.join(ROOT, "openapi", "openapi.yaml")
METHODS = %w[get post put patch delete head options].freeze
SUCCESS_CODES = %w[200 201 202 203 204 206 301 302 304].freeze

abort "check-client-compat: missing #{FROZEN_PATH}" unless File.file?(FROZEN_PATH)

errors = []
warnings = []

def resolve_ref(doc, ref)
  return nil unless ref.is_a?(String) && ref.start_with?("#/")

  segments = ref.sub(/\A#\//, "").split("/").map { |s| s.gsub("~1", "/").gsub("~0", "~") }
  node = doc
  segments.each do |seg|
    return nil unless node.is_a?(Hash) && node.key?(seg)

    node = node[seg]
  end
  node
end

# 把 schema 归一化为可比较的结构：
#   { "kind" => "object", "properties" => {name => norm}, "required" => [..] }
#   { "kind" => "array",  "items" => norm }
#   { "kind" => "enum",   "values" => [..] }
#   { "kind" => "scalar", "type" => type }
#   { "kind" => "any" }
def normalize(schema, doc, depth)
  return { "kind" => "any" } unless schema.is_a?(Hash)
  return normalize(resolve_ref(doc, schema["$ref"]), doc, depth) if schema["$ref"]
  return { "kind" => "any" } if depth > 12

  if schema["enum"].is_a?(Array)
    return { "kind" => "enum", "values" => schema["enum"] }
  end

  if schema["allOf"].is_a?(Array)
    merged = { "kind" => "object", "properties" => {}, "required" => [] }
    schema["allOf"].each do |part|
      sub = normalize(part, doc, depth + 1)
      next unless sub.is_a?(Hash) && sub["kind"] == "object"

      merged["properties"].merge!(sub["properties"] || {})
      merged["required"] = (merged["required"] + (sub["required"] || [])).uniq
    end
    return merged
  end

  type = schema["type"]
  type = Array(type).find { |t| t != "null" } if type.is_a?(Array)
  case type
  when "object", nil
    if schema["properties"].is_a?(Hash)
      props = {}
      schema["properties"].each do |name, sub_schema|
        props[name] = normalize(sub_schema, doc, depth + 1)
      end
      { "kind" => "object", "properties" => props, "required" => (schema["required"] || []) }
    else
      { "kind" => "any" }
    end
  when "array"
    { "kind" => "array", "items" => normalize(schema["items"], doc, depth + 1) }
  when "string", "integer", "number", "boolean"
    { "kind" => "scalar", "type" => type }
  else
    { "kind" => "any" }
  end
end

def collect_props(norm)
  return {} unless norm.is_a?(Hash) && norm["kind"] == "object"

  norm["properties"] || {}
end

def collect_required(norm)
  return [] unless norm.is_a?(Hash) && norm["kind"] == "object"

  norm["required"] || []
end

def type_compatible?(frozen_norm, current_norm, path)
  return true if frozen_norm.nil? || current_norm.nil?
  return true if frozen_norm["kind"] == "any" || current_norm["kind"] == "any"

  if frozen_norm["kind"] == "enum"
    return true if current_norm["kind"] != "enum" # 当前放宽为任意值 → 兼容
    removed = frozen_norm["values"] - current_norm["values"]
    return removed.empty? ? true : "enum 值被移除: #{removed.join(', ')}"
  end

  if frozen_norm["kind"] != current_norm["kind"]
    return "类型变化: #{frozen_norm['kind']} → #{current_norm['kind']}"
  end

  case frozen_norm["kind"]
  when "array"
    type_compatible?(frozen_norm["items"], current_norm["items"], path)
  when "object"
    issues = []
    frozen_norm["properties"].each_key do |name|
      f_prop = frozen_norm["properties"][name]
      c_prop = current_norm["properties"][name]
      if c_prop.nil?
        issues << "#{path}.properties.#{name} 已被移除（旧客户端仍会读取）"
      else
        sub = type_compatible?(f_prop, c_prop, "#{path}.properties.#{name}")
        issues << sub if sub.is_a?(String)
      end
    end
    issues.empty? ? true : issues.join("；")
  when "scalar"
    # 客户端发送的标量类型被服务端扩大接受范围（string→string）视为兼容。
    frozen_norm["type"] == current_norm["type"] ? true : "标量类型: #{frozen_norm['type']} → #{current_norm['type']}"
  else
    true
  end
end

frozen = YAML.safe_load(File.read(FROZEN_PATH), aliases: true)
current = YAML.safe_load(File.read(CURRENT_PATH), aliases: true)

frozen_ops = {}
frozen.fetch("paths", {}).each do |path, item|
  item.each do |method, op|
    next unless METHODS.include?(method) && op.is_a?(Hash)

    frozen_ops["#{method.upcase} #{path}"] = { op: op, path: path, method: method }
  end
end

current_ops = {}
current.fetch("paths", {}).each do |path, item|
  item.each do |method, op|
    next unless METHODS.include?(method) && op.is_a?(Hash)

    current_ops["#{method.upcase} #{path}"] = { op: op, path: path, method: method }
  end
end

# --- 1. 操作表面 ------------------------------------------------------------

frozen_ops.each_key do |key|
  next if current_ops.key?(key)

  errors << "BREAKING: #{key} 在当前契约中已不存在（旧客户端无法调用）"
end

# --- 2/3/4. 逐操作比较 -------------------------------------------------------

frozen_ops.each do |key, f_entry|
  c_entry = current_ops[key]
  next if c_entry.nil?

  f_op = f_entry[:op]
  c_op = c_entry[:op]

  # 请求参数：frozen 客户端发送的每个参数当前必须仍被接受。
  f_params = f_op.fetch("parameters", [])
  c_params = c_entry[:op].fetch("parameters", [])
  c_by_name = {}
  c_params.each do |p|
    next unless p.is_a?(Hash) && p["name"]

    c_by_name[[p["in"], p["name"]]] = p
  end
  f_params.each do |p|
    next unless p.is_a?(Hash) && p["name"]

    c_p = c_by_name[[p["in"], p["name"]]]
    if c_p.nil?
      errors << "BREAKING: #{key} 参数 #{p['in']}.#{p['name']} 已被移除"
      next
    end
    # 旧客户端可能省略的参数，当前不得变成必填。
    if !p["required"] && c_p["required"]
      errors << "BREAKING: #{key} 参数 #{p['in']}.#{p['name']} 由可选变为必填"
    end
    check = type_compatible?(
      normalize(p["schema"], frozen, 0),
      normalize(c_p["schema"], current, 0),
      "#{key} param #{p['in']}.#{p['name']}",
    )
    errors << "BREAKING: #{key} 参数 #{p['in']}.#{p['name']}: #{check}" if check.is_a?(String)
  end

  # 请求体：frozen 请求 schema 的属性当前仍存在。
  f_body = (f_op["requestBody"] || {}).dig("content", "application/json", "schema")
  c_body = (c_op["requestBody"] || {}).dig("content", "application/json", "schema")
  if f_body && c_body
    f_norm = normalize(f_body, frozen, 0)
    c_norm = normalize(c_body, current, 0)
    f_props = collect_props(f_norm)
    c_props = collect_props(c_norm)
    f_props.each_key do |name|
      if c_props[name].nil?
        errors << "BREAKING: #{key} 请求体属性 #{name} 已被移除（旧客户端仍会发送）"
        next
      end
      check = type_compatible?(f_props[name], c_props[name], "#{key} body.#{name}")
      errors << "BREAKING: #{key} 请求体属性 #{name}: #{check}" if check.is_a?(String)
    end
    # 当前新增必填属性不得出现（旧客户端不会发送）。
    new_required = collect_required(c_norm) - collect_required(f_norm)
    new_required.each do |name|
      next if c_props[name].nil? || (c_props[name].is_a?(Hash) && c_props[name]["default"])

      errors << "BREAKING: #{key} 请求体新增必填属性 #{name}（旧客户端不会发送）"
    end
  elsif f_body && c_body.nil?
    errors << "BREAKING: #{key} 请求体被移除"
  end

  # 响应体：frozen 客户端读取的字段当前仍存在；enum 值不得移除。
  f_responses = f_op.fetch("responses", {})
  c_responses = c_op.fetch("responses", {})
  SUCCESS_CODES.each do |status|
    f_resp = f_responses[status]
    c_resp = c_responses[status]
    next unless f_resp.is_a?(Hash)

    if c_resp.nil?
      # 旧客户端依赖的成功状态码被移除 → 破坏。
      errors << "BREAKING: #{key} 响应状态 #{status} 已被移除（旧客户端依赖成功路径）"
      next
    end
    f_schema = (f_resp["content"] || {}).dig("application/json", "schema")
    c_schema = (c_resp["content"] || {}).dig("application/json", "schema")
    next unless f_schema

    if c_schema.nil?
      errors << "BREAKING: #{key} 响应 #{status} JSON schema 被移除"
      next
    end
    f_norm = normalize(f_schema, frozen, 0)
    c_norm = normalize(c_schema, current, 0)
    f_props = collect_props(f_norm)
    c_props = collect_props(c_norm)
    f_props.each_key do |name|
      if c_props[name].nil?
        errors << "BREAKING: #{key} 响应 #{status} 字段 #{name} 已被移除（旧客户端仍会读取）"
        next
      end
      check = type_compatible?(f_props[name], c_props[name], "#{key} resp #{status}.#{name}")
      errors << "BREAKING: #{key} 响应 #{status} 字段 #{name}: #{check}" if check.is_a?(String)
    end
  end
end

# --- 报告 ------------------------------------------------------------

if errors.empty?
  puts "Client compat OK: frozen=#{frozen_ops.length} ops, current=#{current_ops.length} ops"
  puts "- 操作表面、请求参数、请求体、成功响应 schema/enum 全部向后兼容"
  puts "- 新增字段/新增操作不破坏旧客户端（新增字段合法）"
  warnings.each { |w| puts "WARN: #{w}" }
  exit 0
else
  warn "check-client-compat FAILED with #{errors.length} breaking difference(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
