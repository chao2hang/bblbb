#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-TOOL-06 延续：Secret 扫描（make check-secrets 与 ops/security/scan.sh 同规则）。
#
# 扫描 backend/src、frontend/src、openapi 的生产代码路径：
#   * AKIA/sk-/ghp_ 凭据形态；
#   * 完整私钥块（BEGIN…END 或 ≥64 base64 正文），避免脱敏单测的截断示例误报；
#   * 剥离 #[cfg(test)] 模块块（测试 Fixture 不视为生产 Secret）。
# 退出码：发现任何匹配非零。

def strip_test_blocks(content)
  depth = 0
  in_test = false
  out = +""
  content.lines.each do |line|
    if line =~ /#\[cfg\(test\)\]/
      in_test = true
      depth = 0
      next
    end
    if in_test
      depth += line.count("{") - line.count("}")
      in_test = false if depth <= 0
      next
    end
    out << line
  end
  out
end

pat = /(AKIA[0-9A-Z]{16}|sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{36})/
key_header = /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/

def real_private_key?(line, following)
  return true if line =~ /-----END (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/

  following.join.scan(/[A-Za-z0-9+\/]{64,}/).any? { |chunk| chunk.length >= 64 }
end

hits = []
%w[backend/src frontend/src openapi].each do |dir|
  Dir.glob(File.join(dir, "**/*.{rs,ts,js,json,yaml,yml}")).each do |file|
    next if file.include?("/node_modules/") || file.include?("/build/")

    lines = strip_test_blocks(File.read(file)).lines
    lines.each_with_index do |line, index|
      if line =~ pat || (line =~ key_header && real_private_key?(line, lines[index, 20]))
        hits << "#{file}:#{index + 1}: #{line.strip}"
      end
    end
  end
end

if hits.empty?
  puts "OK: 未检测到已知 Secret 模式（AKIA/sk-/ghp_/完整私钥块；测试 Fixture 已剥离）"
  exit 0
else
  warn "FAIL: 检测到疑似 Secret（生产代码路径）："
  hits.each { |hit| warn "- #{hit}" }
  exit 1
end
