#!/usr/bin/env ruby
# frozen_string_literal: true

# M01-AUDIT-07：领域事件名称与 payload version 与 docs/EVENT-CATALOG.md 机械比对。
#
# 事实来源：docs/EVENT-CATALOG.md 的事件目录表。
# 代码来源：backend/src/events.rs 的注册表（常量 + all_event_types）。
#
# 失败条件：
# - 目录中的事件未注册到 events.rs（缺失）；
# - events.rs 注册了目录没有的事件（漂移）；
# - payload version（`.v<major>`）不一致。
#
# 用法：`ruby scripts/check-event-catalog.rb`；失败退出码 1。

ROOT = File.expand_path("..", __dir__)
CATALOG = File.join(ROOT, "docs", "EVENT-CATALOG.md")
REGISTRY = File.join(ROOT, "backend", "src", "events.rs")

def catalog_events
  content = File.read(CATALOG)
  # 事件目录表行：`| event_type | ... |`
  content.scan(/\| `([a-z][a-z0-9_.]*\.v\d+)` \|/).flatten
end

def registry_events
  content = File.read(REGISTRY)
  # 常量定义 `pub const NAME: &str = "xxx.v1";`
  content.scan(/pub const \w+: &str = "([a-z][a-z0-9_.]*\.v\d+)";/).flatten
end

def version_of(event)
  event[/\.v(\d+)$/, 1]&.to_i
end

catalog = catalog_events
registry = registry_events

failures = []
if catalog.empty?
  failures << "#{CATALOG}: 未能从事件目录解析出任何 event_type"
end

# 目录 → 代码：目录中的每个事件都必须注册
catalog.each do |event|
  unless registry.include?(event)
    failures << "#{event}: 目录已登记但 events.rs 未注册"
  end
end

# 代码 → 目录：不允许漂移
registry.each do |event|
  unless catalog.include?(event)
    failures << "#{event}: events.rs 已注册但目录未登记（漂移）"
  end
end

# 版本一致性（目录与代码相同事件名，版本号必须一致）
catalog.each do |event|
  next unless registry.include?(event)
  cv = version_of(event)
  rv = version_of(event)
  if cv != rv
    failures << "#{event}: payload version 不一致（目录 #{cv} vs 代码 #{rv}）"
  end
end

if failures.empty?
  puts "事件目录 OK: #{catalog.length}/#{registry.length} 事件与 payload version 一致"
  exit 0
else
  $stderr.puts failures
  $stderr.puts "错误：领域事件名称/payload version 与 docs/EVENT-CATALOG.md 不一致，见 backend/src/events.rs"
  exit 1
end
