#!/usr/bin/env ruby
# frozen_string_literal: true

# M16-HARNESS-03
#
# 校验 reports/rc/state-machine-coverage.md 状态机迁移测试矩阵的引用真实性：
# 每个 `backend/tests/<file>.rs#<fn>`（或 `backend/src/<file>.rs#<fn>`）引用必须
# 真实存在（文件存在且包含 `fn <fn>` / `async fn <fn>`）。
#
# 退出码：任何缺失引用非零。

ROOT = File.expand_path("..", __dir__)
MATRIX_PATH = File.join(ROOT, "reports", "rc", "state-machine-coverage.md")

abort "check-state-machine-matrix: missing #{MATRIX_PATH}" unless File.file?(MATRIX_PATH)

errors = []
lines = File.readlines(MATRIX_PATH, chomp: true)

lines.each do |line|
  next unless line.start_with?("|") && line.include?("backend/")

  line.scan(%r{`(backend/(?:tests|src)/[^`#]+?\.rs)#([a-z_][a-z0-9_]*)`}) do |rel_path, fn_name|
    abs = File.join(ROOT, rel_path)
    unless File.file?(abs)
      errors << "#{rel_path}: 文件不存在"
      next
    end

    content = File.read(abs)
    unless content.match?(/(?:fn|async fn)\s+#{Regexp.escape(fn_name)}\b/)
      errors << "#{rel_path}##{fn_name}: 找不到测试函数"
    end
  end
end

if errors.empty?
  puts "State-machine matrix OK: 引用的状态机迁移测试文件与函数全部存在"
  exit 0
else
  warn "check-state-machine-matrix FAILED with #{errors.length} difference(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
