#!/usr/bin/env ruby
# frozen_string_literal: true

# M16-HARNESS-04
#
# 每个稳定 API Problem code（docs/ERROR-CODES.md 注册表）至少关联一个
# 后端测试 Fixture（backend/tests/ 或 backend/src/ 中直接引用该 code 的断言）
# 和一个前端映射（frontend/src/lib/errors.ts 的 MESSAGE_BY_CODE）。
#
# 语义：
#   * fixture = 该 code 字符串出现在 backend/tests/ 或 backend/src/（即
#     有测试或实现真正产生/断言这个稳定机器码）。
#   * frontend = errors.ts 的 MESSAGE_BY_CODE 覆盖该 code（problemMessage
#     命中稳定中文文案，而不是退化到 detail/status）。
#
# 退出码：任何 code 缺少 fixture 或前端映射时非零。

require "yaml"

ROOT = File.expand_path("..", __dir__)
ERROR_CODES_PATH = File.join(ROOT, "docs", "ERROR-CODES.md")
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
BACKEND_TESTS_DIR = File.join(ROOT, "backend", "tests")
BACKEND_SRC_DIR = File.join(ROOT, "backend", "src")
FRONTEND_ERRORS_PATH = File.join(ROOT, "frontend", "src", "lib", "errors.ts")
FRONTEND_GENERATED = File.join(ROOT, "frontend", "src", "lib", "api", "generated", "v1")

errors = []

# --- 1. 注册表 -----------------------------------------------------------------

rows = []
if File.file?(ERROR_CODES_PATH)
  File.readlines(ERROR_CODES_PATH, chomp: true).each do |line|
    match = line.match(/\A\|\s*`([a-z0-9_]+)`\s*\|\s*(\d{3})\s*\|/)
    rows << { code: match[1], http: match[2] } if match
  end
else
  abort "check-code-fixtures: missing #{ERROR_CODES_PATH}"
end
codes = rows.map { |row| row[:code] }

# --- 2. OpenAPI 枚举与实现一致（复用 check-error-codes 的方向） -----------------

document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
openapi_codes = document.dig("components", "schemas", "Problem", "properties", "code", "enum") || []
extra_in_openapi = openapi_codes - codes
extra_in_docs = codes - openapi_codes
if extra_in_openapi.any?
  errors << "OpenAPI 枚举了但 docs/ERROR-CODES.md 未登记的 code：#{extra_in_openapi.join(', ')}"
end
if extra_in_docs.any?
  errors << "docs/ERROR-CODES.md 登记了但 OpenAPI 未枚举的 code：#{extra_in_docs.join(', ')}"
end

# --- 3. 每个 code 的后端 Fixture -------------------------------------------------

def code_quoted(code)
  # 覆盖 "code" 与 'code' 两种字面量写法。
  ["\"#{code}\"", "'#{code}'"]
end

def fixture_candidates(code)
  quoted = code_quoted(code)
  files = []
  [BACKEND_TESTS_DIR, BACKEND_SRC_DIR].each do |dir|
    next unless Dir.exist?(dir)

    Dir.glob(File.join(dir, "**", "*.rs")).each do |file|
      content = File.read(file)
      next unless quoted.any? { |q| content.include?(q) }

      files << file.delete_prefix(ROOT + "/")
    end
  end
  files
end

fixture_report = {}
codes.each do |code|
  files = fixture_candidates(code)
  fixture_report[code] = files
  next unless files.empty?

  errors << "#{code}: 没有后端 Fixture/实现引用（backend/tests 与 backend/src 均无 #{code_quoted(code).join(' / ')}）"
end

# --- 4. 前端映射 -----------------------------------------------------------------

frontend_src = File.file?(FRONTEND_ERRORS_PATH) ? File.read(FRONTEND_ERRORS_PATH) : ""
frontend_codes = frontend_src.scan(/^\s{2}([a-z0-9_]+):/).flatten.uniq

codes.each do |code|
  next if frontend_codes.include?(code)

  errors << "#{code}: frontend/src/lib/errors.ts MESSAGE_BY_CODE 未映射"
end

# --- 5. 报告 --------------------------------------------------------------------

if errors.empty?
  puts "Code fixtures OK: #{codes.length} stable codes"
  puts "- 全部在 backend tests/src 中有 Fixture/实现引用"
  puts "- 全部在 frontend/src/lib/errors.ts 中有中文映射"
  puts "- docs/ERROR-CODES.md、OpenAPI Problem.code enum、backend、frontend 四方一致"
  fixture_report.each do |code, files|
    puts "  #{code}: #{files.length} fixture 文件"
  end
  exit 0
else
  warn "check-code-fixtures FAILED with #{errors.length} difference(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
