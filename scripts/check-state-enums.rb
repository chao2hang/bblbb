#!/usr/bin/env ruby
# frozen_string_literal: true

# M00-CONTRACT-05 / -12
#
# Compares the stable enums in openapi.yaml against docs/STATE-MACHINES.md
# and reports the Rust/TypeScript enum difference surface:
#
#   * OpenAPI enum values that STATE-MACHINES.md never mentions;
#   * documented state machines whose state field is unconstrained in the
#     OpenAPI (free-form `status`/`state` strings) -- these are the enums the
#     Rust and TypeScript layers must eventually mirror;
#   * a hard contradiction (doc-declared deprecated/removed value still
#     appearing in an OpenAPI enum) fails the check.
#
# Output is a difference report; it exits non-zero only on contradictions or
# on an empty enum member (also caught by check-openapi.rb).

require "yaml"

ROOT = File.expand_path("..", __dir__)
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
STATE_MACHINES_PATH = File.join(ROOT, "docs", "STATE-MACHINES.md")
BACKEND_SRC = File.join(ROOT, "backend", "src")
FRONTEND_SRC = File.join(ROOT, "frontend", "src")

errors = []
report = []

document = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
doc_text = File.file?(STATE_MACHINES_PATH) ? File.read(STATE_MACHINES_PATH) : ""
errors << "missing #{STATE_MACHINES_PATH}" if doc_text.empty?

# --- OpenAPI enum inventory -------------------------------------------------

def walk(node, path, out)
  case node
  when Hash
    out << [path.join("/"), node["enum"]] if node["enum"].is_a?(Array)
    node.each { |key, value| walk(value, path + [key.to_s], out) if value.is_a?(Hash) || value.is_a?(Array) }
  when Array
    node.each_with_index { |value, index| walk(value, path + [index.to_s], out) if value.is_a?(Hash) || value.is_a?(Array) }
  end
end

enum_locations = []
walk(document.dig("components", "schemas"), ["components/schemas"], enum_locations)

openapi_enum_values = {}
enum_locations.each do |location, values|
  values.each do |value|
    next if value.nil?

    if value.is_a?(String) && value.empty?
      errors << "#{location} has an empty enum member (contract bug; also reported by check-openapi.rb)"
      next
    end
    openapi_enum_values[value] ||= []
    openapi_enum_values[value] << location
  end
end

# --- STATE-MACHINES.md token inventory ---------------------------------------

# Collect backticked tokens and words inside fenced code blocks, so we can
# tell whether a documented state was mentioned at all.
doc_mentions = doc_text.scan(/`([a-z0-9_]+)`/).flatten
doc_text.scan(/```.*?```/m).each do |block|
  block.scan(/[a-z][a-z0-9_]*/).each { |word| doc_mentions << word }
end
doc_mentions.uniq!

# --- Hard contradiction: deprecated/removed states ---------------------------

# The document explicitly deprecates `status=closed`; if any OpenAPI enum
# still contains it, the contract contradicts the state machine.
if openapi_enum_values.key?("closed")
  errors << "OpenAPI still enumerates `closed` but STATE-MACHINES.md deprecates status=closed (repair: remove from the OpenAPI enum)"
end

# --- Difference report -------------------------------------------------------

report << "== OpenAPI 枚举 → STATE-MACHINES.md 覆盖 =="
enum_locations.each do |location, values|
  values = values.compact
  covered = values.all? { |value| doc_mentions.include?(value) }
  missing = values.reject { |value| doc_mentions.include?(value) }
  status = missing.empty? ? "covered" : "partial(#{missing.join(', ')})"
  report << format("  %-70s %-10s [%s]", location, status, values.join(' | '))
end

report << ""
report << "== STATE-MACHINES.md 状态机 → OpenAPI 约束 =="
# Fields that should carry an enum per the state-machine doc: [schema, property].
state_fields = {
  "Post.status" => ["Post", "status"], "Comment.status" => ["Comment", "status"],
  "Sanction kind" => ["SanctionCreate", "type"], "Checkout Intent" => ["CheckoutIntentCreate", "status"],
  "Purchase.status" => ["Purchase", "status"], "Refund.status" => ["RefundCreate", "status"],
  "AI Task.status" => ["AiTask", "status"], "AI Suggestion.status" => ["AiSuggestion", "status"],
  "Video Embed.status" => ["VideoEmbedCreate", "status"], "Shop Product.status" => ["ShopProduct", "status"],
  "Shop Order.status" => ["ShopOrderCreate", "status"], "Entitlement.status" => ["Entitlement", "status"],
  "Activity Claim.status" => ["ActivityClaim", "status"], "Job.status" => ["Job", "status"],
  "Outbox Event.status" => ["OutboxEvent", "status"], "Webhook Delivery.status" => ["WebhookDelivery", "status"],
  "User.status" => ["User", "status"], "Attachment.status" => ["Attachment", "status"],
  "Case.status" => ["ModerationCase", "status"], "Appeal.status" => ["Appeal", "status"]
}
state_fields.each do |machine, (schema_name, property_name)|
  constrained = enum_locations.any? do |location, _values|
    location.split("/").include?(schema_name) && location.end_with?("properties/#{property_name}")
  end
  report << format("  %-30s %s", machine, constrained ? "constrained" : "unconstrained (free-form string in OpenAPI)")
end

report << ""
report << "== Rust/TypeScript 枚举差异 =="
rust_enums = Dir[File.join(BACKEND_SRC, "**", "*.rs")].flat_map do |file|
  File.read(file).scan(/pub enum ([A-Z][A-Za-z0-9_]+)/).flatten
end
rust_domain_enums = rust_enums - %w[VerifyResult DatabaseKind]
ts_enums = Dir[File.join(FRONTEND_SRC, "**", "*.ts")].flat_map do |file|
  File.read(file).scan(/enum\s+([A-Za-z0-9_]+)/).flatten
end
if rust_domain_enums.empty?
  report << "  Rust: no domain state enums yet (only VerifyResult, DatabaseKind) -- OpenAPI is the source of truth"
else
  report << "  Rust domain enums: #{rust_domain_enums.join(', ')}"
end
if ts_enums.empty?
  report << "  TypeScript: no hand-written enums; generated union types live in frontend/src/lib/api/generated/v1/types.ts"
else
  report << "  TypeScript enums: #{ts_enums.join(', ')}"
end
report << "  OpenAPI enum count: #{enum_locations.length} declarations, #{openapi_enum_values.keys.length} distinct values"
report << "  (generated TS union types are produced by scripts/generate-ts-types.rb --check for reproducibility)"

# --- Report -----------------------------------------------------------------

puts "状态枚举比对报告 (M00-CONTRACT-05)"
puts report.join("\n")

if errors.empty?
  exit 0
else
  warn "check-state-enums FAILED with #{errors.length} contradiction(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
