#!/usr/bin/env ruby
# frozen_string_literal: true

# M04-MARKDOWN-08：前端 HTML sink 静态检查。
#
# 规则（文档见 backend/src/content/markdown/rerender.rs 与
# frontend/src/lib/components/SafeHtml.svelte）：
# 1. `{@html}` 只允许出现在白名单组件（SafeHtml / JsonLd）；
#    该组件只接收**后端已清洗**的 HTML（ammonia allowlist 渲染产物）。
# 2. 生产源文件（frontend/src，排除测试）禁止任何 DOM HTML sink：
#    innerHTML/outerHTML 赋值、insertAdjacentHTML、document.write、
#    DOMParser/parseFromString、createContextualFragment、iframe srcdoc。
# 3. 测试文件（*.test.ts / *.test.svelte / *.spec.* 及 frontend/src/test/）
#    豁免——jsdom 测试夹具合法使用 innerHTML 搭建 DOM。
#
# 用法：`ruby scripts/check-html-sinks.rb`；失败退出码 1。

FRONTEND_SRC = File.expand_path("../frontend/src", __dir__)
# M04-MARKDOWN-08 白名单：{@html} 只允许出现在这些组件中——
# SafeHtml.svelte 只渲染后端已清洗的 HTML（ammonia allowlist 产物）；
# JsonLd.svelte 只输出静态 JSON-LD script（`</script` 内部转义）。
SAFE_HTML_FILES = [
  "frontend/src/lib/components/SafeHtml.svelte",
  "frontend/src/lib/components/JsonLd.svelte",
].freeze

# {@html 标签（含空格变体）；负向后看排除注释中的字面量
HTML_TAG = /(?<![\/\-\*])\{@html\b/

# 注释剥离：HTML 注释 / JS 块注释 / JS 行注释（生产源码无字面量 `//` 字符串，
# 见 check-html-sinks.rb 说明；避免把注释里的 {@html}/innerHTML 误判为真实 sink）
def strip_comments(content)
  content.gsub(%r{<!--[\s\S]*?-->}, '').gsub(%r{/\*[\s\S]*?\*/}, '').gsub(%r{//[^\n]*}, '')
end

# DOM HTML sink（生产代码禁止）
DOM_SINKS = [
  [/\.innerHTML\s*=/, "innerHTML 赋值"],
  [/\.outerHTML\s*=/, "outerHTML 赋值"],
  [/\.insertAdjacentHTML\s*\(/, "insertAdjacentHTML"],
  [/document\.write\s*\(/, "document.write"],
  [/\bDOMParser\s*\(/, "DOMParser 构造"],
  [/\.parseFromString\s*\(/, "DOMParser.parseFromString"],
  [/\.createContextualFragment\s*\(/, "createContextualFragment"],
  [/\bsrcdoc\s*=/, "iframe srcdoc"],
].freeze

# 测试文件豁免
def test_file?(rel)
  rel.start_with?("test/") ||
    rel.include?(".test.") ||
    rel.include?(".spec.") ||
    rel.include?("/testing/")
end

def safe_html_file?(file)
  SAFE_HTML_FILES.any? { |rel| File.expand_path(file) == File.expand_path("../#{rel}", __dir__) }
end

def scan
  failures = []
  files = Dir.glob(File.join(FRONTEND_SRC, "**", "*.{svelte,ts,js}")).sort
  files.each do |file|
    rel = file.delete_prefix("#{FRONTEND_SRC}/")
    # 注释不参与匹配（{@html} 或 innerHTML 出现在注释里不是 sink）
    content = strip_comments(File.read(file))

    # 规则 1：{@html} 仅白名单组件（仅 .svelte 模板文件中有语义；
    # .ts/.js 字符串里的 {@html} 只是文本，如测试用例描述）
    if file.end_with?(".svelte") && HTML_TAG.match?(content)
      next if safe_html_file?(file)

      failures << "#{rel}: 发现 {@html}——仅 #{SAFE_HTML_FILES.join(' / ')} 允许（M04-MARKDOWN-08）"
    end

    # 规则 2：生产代码 DOM sink（测试豁免）
    next if test_file?(rel)
    next if safe_html_file?(file) # 白名单组件本身只含受控 {@html}，无 DOM sink

    DOM_SINKS.each do |pat, label|
      failures << "#{rel}: 发现 #{label} sink（仅测试文件可用，M04-MARKDOWN-08）" if pat.match?(content)
    end
  end
  failures
end

failures = scan
if failures.empty?
  puts "HTML sink OK: {@html} 仅白名单组件（SafeHtml/JsonLd），生产源文件无 DOM HTML sink"
  exit 0
else
  $stderr.puts failures
  $stderr.puts "错误：前端 HTML 注入面未收口（见 M04-MARKDOWN-08 / SafeHtml.svelte / JsonLd.svelte）"
  exit 1
end
