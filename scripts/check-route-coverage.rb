#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-09 / -12
#
# Bidirectional comparison of the axum route registry (backend/src/**/*.rs)
# against the operation coverage manifest (todo/openapi-operation-coverage.json,
# itself synced from openapi/openapi.yaml):
#
#   * contract operations with NO axum route -> "契约无实现"
#     (hard failure when the operation claims baseline_only/in_progress/
#      implemented/verified/blocked; informational for not_started);
#   * axum routes with NO contract operation -> "实现无契约"
#     (hard failure, except the documented non-contract endpoints
#      /readyz and /api/v1/openapi.json);
#   * method/path drift between the two registries.
#
# Normalization: `{param}` braces are collapsed and the `/api/v1` prefix is
# stripped on both sides. Reads backend/src read-only. Failing output names
# operationIds/paths with a repair entry.

require "json"
require "yaml"

ROOT = File.expand_path("..", __dir__)
BACKEND_SRC = File.join(ROOT, "backend", "src")
COVERAGE_PATH = File.join(ROOT, "todo", "openapi-operation-coverage.json")
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
ROUTE_METHODS = %w[get post put patch delete head options trace].freeze

# Documented endpoints that intentionally live outside the 232-operation
# contract (docs/API.md §1).
# `robots.txt` / `sitemap.xml` 是 Web 标准端点（搜索引擎/抓取器直接访问），
# 不进入 /api/v1 契约；由 M08-FEEDS 路由提供并记录在 docs/CRAWLER-POLICY.md §7。
# M12/M13 内部管理/运营端点（Marketplace 审批/对账/紧急停用、Plugin 管理、
# /metrics、Marketplace 确认页视图）为领域管理接口，不在当前冻结 223-op 契约内，
# 记录于 docs/MARKETPLACE.md §12 与 docs/PLUGIN.md。
DOCUMENTED_NON_CONTRACT = {
  "readyz" => %w[GET],
  "openapi.json" => %w[GET],
  "robots.txt" => %w[GET],
  "sitemap.xml" => %w[GET],
  "metrics" => %w[GET],
  # 站点公开信息（全站文案统一，0065）：登录/注册页等前台文案的匿名只读
  # 投影，供第一方 SSR 使用；非冻结契约端点，记录于 docs/API.md §2。
  "site" => %w[GET],
  # 兴趣推荐流（发现页算法推送，2026-09）：匿名可读 + 登录个性化。v1 算法
  # 迭代期（画像权重/召回策略会持续调整），暂不进冻结契约，记录于 docs/API.md。
  "recommendations" => %w[GET],
  # 用户 @提及模糊匹配（回复/发帖 @ 自动完成，默认 5 个最相近用户）：
  # 仅限登录用户；非冻结契约端点，记录于 docs/API.md §2。
  "users/suggest" => %w[GET],
  # 互动表情收表情与明细列表（参考 Discourse 式右侧发表情、左侧收表情与弹窗明细）：
  "posts/{p}/reactions" => %w[GET POST],
  "comments/{p}/reactions" => %w[GET POST],
  "marketplace/checkout-intents/{p}" => %w[GET],
  "admin/marketplace/clients/{p}" => %w[GET PATCH],
  "admin/marketplace/clients/{p}/emergency-disable" => %w[POST],
  "admin/marketplace/offers" => %w[GET],
  "admin/marketplace/reconciliation/run" => %w[POST],
  "admin/marketplace/refunds/{p}/retry" => %w[POST],
  "admin/marketplace/webhook-deliveries" => %w[GET],
  "admin/marketplace/webhook-deliveries/{p}/replay" => %w[POST],
  "admin/plugins" => %w[GET POST],
  # Feature Flag 管理（M14-FLAGS）：GET 列表在冻结契约内（getAdminFeatureFlags）；
  # POST /admin/feature-flags 是有意保留的误用提示端点（400 提示改用
  # POST /admin/feature-flags/kill-switch，见 feature_flags.rs kill_switch_hint），
  # 不进冻结契约，记录于 docs/API.md §2。
  "admin/feature-flags" => %w[GET POST],
  "admin/plugins/{p}" => %w[GET DELETE],
  "admin/plugins/{p}/disable" => %w[POST],
  "admin/plugins/{p}/enable" => %w[POST],
  "admin/plugins/{p}/metrics" => %w[GET],
  "admin/plugins/{p}/settings" => %w[PATCH],
  "admin/plugins/capabilities" => %w[GET],
  # GAP-FIX 管理域（GAP-FIX-SPEC §二）：站点统计/BI/审计读取/系统设置/
  # 帖子管理动作/通知广播/角色分配/成就管理为运营管理接口，同 M12/M13
  # 先例不进入冻结契约，记录于 docs/OPERATIONS.md。
  "admin/stats" => %w[GET],
  "admin/stats/trend" => %w[GET],
  "admin/bi/metrics" => %w[GET],
  "admin/audit-logs" => %w[GET],
  "admin/settings" => %w[GET PATCH],
  "admin/posts" => %w[GET],
  "admin/posts/{p}/revisions" => %w[GET],
  "admin/posts/{p}/action" => %w[POST],
  "admin/notifications/broadcast" => %w[POST],
  "admin/notifications/outbox" => %w[GET],
  "admin/notifications/outbox/{p}/recall" => %w[POST],
  "admin/notifications/templates" => %w[GET],
  "admin/users/{p}/roles" => %w[POST],
  "admin/users/{p}/roles/{p}" => %w[DELETE],
  "admin/achievements" => %w[GET POST],
  "admin/achievements/{p}" => %w[PATCH DELETE],
  "admin/achievements/{p}/grant" => %w[POST],
  # 成就图标（不走 S3）：管理侧上传/移除（直写 storage_dir/achievements/ 本地
  # 磁盘）与公开读取端点；同上先例不进入冻结契约，记录于 docs/OPERATIONS.md
  # §19.8 与 docs/API.md §21.4。
  "achievements/{p}/icon" => %w[GET],
  "admin/achievements/{p}/icon" => %w[POST DELETE],
  # GAP-FIX 经济与个人域（GAP-FIX-SPEC §二）：积分流水/调整、附件管理、
  # 下载交易、标签合并为运营管理接口，同上先例。等级规则存档 CRUD
  # （GET/PATCH /admin/levels*）与经验方案投影（GET /admin/levels/scheme）
  # 于 2026-09 移除——等级体系合并为 LinuxDo 信任等级单轨，/admin/levels
  # 页面改读 admin/trust-levels 数据；attachment-quota 端点保留（冻结契约，
  # 不在本清单），档位键改为 users.trust_level 0–4。
  "admin/points/ledger" => %w[GET],
  "admin/points/adjust" => %w[POST],
  "admin/attachments" => %w[GET],
  "admin/attachments/{p}" => %w[DELETE],
  "admin/download-billing/transactions" => %w[GET],
  "admin/tags/{p}/merge" => %w[POST],
  # 个人域·私信撤回（2026-09）：2 分钟内本人撤回私信；非冻结契约扩展端点，记录于 docs/OPERATIONS.md §19.8。
  "conversations/{p}/messages/{p}/recall" => %w[POST],
  # GAP-FIX 个人域：本人附件列表 + 当前等级容量摘要（前台「我的附件」页
  # 数据源，GET /attachments）。后端扩展接口，同 M12/M13 先例不进入冻结
  # 契约，记录于 docs/API.md §12。POST /attachments 本身在契约内，此处
  # 一并列出是因为 axum 同路径方法链路由（get+post）按整路径登记。
  "attachments" => %w[GET POST],
  # 信任等级（M20-TRUST，2026-09）：LinuxDo 式 TL0–TL4。本人进度视图与
  # 阅读心跳为个人域扩展；每级规则视图/编辑/重置与手动授予为运营管理接口，
  # 同上先例不进入冻结契约，记录于 docs/TRUST-LEVELS.md §5 与 docs/API.md。
  # 2026-09 等级可配置化：PATCH 编辑单级规则（If-Match+reason）与 POST
  # reset（恢复内置 LinuxDo 默认）。
  "me/trust-level" => %w[GET],
  "me/trust-level/read-time" => %w[POST],
  "admin/trust-levels" => %w[GET],
  "admin/trust-levels/{p}" => %w[PATCH],
  "admin/trust-levels/{p}/reset" => %w[POST],
  "admin/users/{p}/trust-level" => %w[POST],
  # 昵称治理与黑名单：一键随机用户昵称 + 昵称黑名单管理。
  # 运营管理接口，记录于 docs/OPERATIONS.md。
  "admin/users/{p}/randomize-nickname" => %w[POST],
  "admin/nickname-blacklist" => %w[GET POST],
  "admin/nickname-blacklist/{p}" => %w[DELETE]
}.freeze

def normalize_path(path)
  normalized = path.to_s
                  .sub(%r{\A/api/v1}, "")
                  .gsub(/\{[a-zA-Z_][a-zA-Z0-9_]*\}/, "{p}")
                  .gsub(%r{\A/+}, "")
                  .chomp("/")
  normalized.empty? ? "/" : normalized
end

def line_number(text, index)
  text[0...index].count("\n") + 1
end

# Extract `{ path:, methods:, file:, line: }` from one Rust file.
def extract_routes(text, file)
  routes = []
  offset = 0
  while (match = /\.route\(/.match(text, offset))
    start = match.offset(0).first + ".route(".length
    depth = 1
    index = start
    while depth.positive? && index < text.length
      case text[index]
      when "(" then depth += 1
      when ")" then depth -= 1
      end
      index += 1
    end
    call = text[start...index - 1]
    path = call[/"([^"]+)"/, 1]
    if path
      methods = call.scan(/\b(#{ROUTE_METHODS.join('|')})\s*\(/).flatten.map(&:upcase).uniq
      routes << { path: path, methods: methods, file: file, line: line_number(text, match.offset(0).first) }
    end
    offset = match.offset(0).last
  end
  routes
end

# --- Load backend routes ------------------------------------------------------

backend_routes = []
Dir[File.join(BACKEND_SRC, "**", "*.rs")].sort.each do |file|
  next if file.include?("/target/")

  text = File.read(file)
  extract_routes(text, file).each { |route| backend_routes << route }
end

# Merge routes by normalized path.
backend_by_path = Hash.new { |hash, key| hash[key] = [] }
backend_routes.each do |route|
  backend_by_path[normalize_path(route[:path])] << route
end

# --- Load coverage / contract -------------------------------------------------

coverage = JSON.parse(File.read(COVERAGE_PATH))
operations = coverage.fetch("operations")
index = {}
operations.each { |entry| index[entry.fetch("operation_id")] = entry }

errors = []
informational = []

contract_by_path = Hash.new { |hash, key| hash[key] = [] }
operations.each do |entry|
  contract_by_path[normalize_path(entry.fetch("path"))] << entry
end

# --- Direction 1: contract operations without a route -------------------------

operations.sort_by { |entry| entry.fetch("path") }.each do |entry|
  operation_id = entry.fetch("operation_id")
  method = entry.fetch("method")
  path = normalize_path(entry.fetch("path"))
  status = entry.fetch("implementation_status")

  registered = backend_by_path[path]
  methods_on_path = registered.flat_map { |route| route[:methods] }.uniq
  location = registered.empty? ? nil : "#{registered.first[:file]}:#{registered.first[:line]}"

  if registered.empty?
    if %w[baseline_only in_progress implemented verified blocked].include?(status)
      errors << "#{operation_id} (#{method} #{entry.fetch('path')}) claims #{status} but has no axum route (repair: register the route in backend/src/routes/*.rs or set implementation_status back to not_started)"
    else
      informational << "#{operation_id} (#{method} #{entry.fetch('path')}) has no axum route yet (status: #{status})"
    end
    next
  end

  unless methods_on_path.include?(method)
    errors << "#{operation_id} (#{method} #{entry.fetch('path')}) route exists at #{location} but exposes #{methods_on_path.join('/')} (repair: add the missing method handler or fix the coverage entry)"
  end
end

# --- Direction 2: routes without a contract operation --------------------------

backend_by_path.keys.sort.each do |path|
  backend_by_path[path].each do |route|
    contract_matches = contract_by_path[path]
    methods = route[:methods].empty? ? ["(any)"] : route[:methods]
    missing_methods = methods.reject do |method|
      contract_matches.any? { |entry| entry.fetch("method") == method }
    end
    next if missing_methods.empty?

    key = "#{path} (#{missing_methods.join('/')})"
    if DOCUMENTED_NON_CONTRACT.key?(path) && (methods - DOCUMENTED_NON_CONTRACT[path]).empty?
      informational << "#{route[:file]}:#{route[:line]} registers documented non-contract endpoint /#{path} (#{methods.join('/')})"
    else
      errors << "#{route[:file]}:#{route[:line]} registers /#{path} (#{missing_methods.join('/')}) with no contract operation (repair: add the operation to openapi.yaml or register the route in the coverage manifest)"
    end
  end
end

# --- Report -------------------------------------------------------------------

puts "axum 路由 vs 覆盖清单双向比对 (M00-CONTRACT-09)"
puts "后端路由: #{backend_routes.length} 条; 契约操作: #{operations.length} 条"
puts
informational.each { |message| puts "INFO: #{message}" }

if errors.empty?
  puts
  puts "route-coverage OK: no contract-without-implementation (for claimed statuses), no implementation-without-contract, no method/path drift"
  exit 0
else
  warn
  warn "check-route-coverage FAILED with #{errors.length} difference(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
