#!/usr/bin/env ruby
# frozen_string_literal: true

# M01-JOBS-07：数据库写事务内禁止外部 IO 静态扫描。
#
# 规则：`backend/src` 中任何包含“事务原语”的源文件，不得引用外部 IO 依赖
# （SMTP/lettre、S3/aws-sdk、HTTP 客户端 reqwest、image、ffmpeg、AI Provider
# SDK）。事务代码只做数据库读写与内存计算；外部 IO 必须放在独立 adapter/
# worker 执行层，绝不在 BEGIN..COMMIT 之间调用（docs/JOBS.md §4.2）。
#
# 用法：`ruby scripts/check-tx-io.rb`；失败退出码 1。

BACKEND_SRC = File.expand_path("../backend/src", __dir__)

# 事务原语：出现任一即视为该文件包含数据库写事务代码。
TX_PATTERNS = [
  /_in_tx\b/, # outbox::enqueue_in_tx / consume_in_tx / mark_sent_in_tx
  /\bTransaction\b/, # sqlx::Transaction 或事务类型别名
  /\.begin\(/, # 事务开始
].freeze

# 外部 IO 依赖标记（禁止出现在事务文件中）。
# 依赖名粒度，避免命中注释中的普通单词（如 smtp/video/sharp）。
IO_PATTERNS = [
  /\blettre\b/, # SMTP 邮件
  /aws[-_]?sdk/, # AWS SDK（S3 等）
  /\breqwest\b/, # HTTP 客户端（AI/视频 Provider 调用）
  /\bimage::/, # 图片处理（image crate）
  /\bffmpeg\b/, # 视频转码
  /\bopenai\b/, # AI Provider
  /\banthropic\b/, # AI Provider
].freeze

def scan
  failures = []
  files = Dir.glob(File.join(BACKEND_SRC, "**", "*.rs")).sort
  files.each do |file|
    content = File.read(file)
    next unless TX_PATTERNS.any? { |p| p.match?(content) }

    IO_PATTERNS.each do |pat|
      failures << "#{file}: 事务文件中发现外部 IO 标记 #{pat}" if pat.match?(content)
    end
  end
  failures
end

failures = scan
if failures.empty?
  puts "事务 IO 边界 OK（backend/src 事务文件无 SMTP/S3/AI/视频/图片处理依赖）"
  exit 0
else
  $stderr.puts failures
  $stderr.puts "错误：数据库写事务内禁止调用外部 IO（SMTP/S3/AI/视频/图片处理），见 docs/JOBS.md §4.2"
  exit 1
end
