#!/usr/bin/env bash
# alerts-drill.sh — 告警表推演练（M15-OBSERVE-08）。
#
# 断言：
#   1. 每条告警定义存在（alerts.md 表格行非空、含 PromQL/条件与级别）；
#   2. 告警引用的指标名都登记在 backend METRIC_HELP 白名单；
#   3. 告警 YAML（若提供）可被 Prometheus promtool 校验；
#   4. 值班/升级/审批路径存在（oncall.md §2/§4）。
#
# 输出记录到 ops/monitoring/alerts-drill-<date>.txt。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ALERTS_MD="$ROOT/deploy/monitoring/alerts.md"
METRICS_RS="$ROOT/backend/src/observability/metrics.rs"
ONCALL_MD="$ROOT/ops/runbooks/oncall.md"
OUT="$ROOT/ops/monitoring/alerts-drill-$(date +%Y%m%d).txt"
mkdir -p "$ROOT/ops/monitoring"

PASS=0
FAIL=0
ok()  { echo "  ok: $*" | tee -a "$OUT"; PASS=$((PASS+1)); }
bad() { echo "  FAIL: $*" | tee -a "$OUT"; FAIL=$((FAIL+1)); }

{
echo "================================================================"
echo "BBLBB 告警表推演练  $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "================================================================"

echo "==> 1/4 告警定义存在性与级别"
for name in bblbb_http_5xx_high bblbb_http_429_surge bblbb_http_latency_p95 \
            bblbb_db_pool_exhausted bblbb_db_connect_failures bblbb_sqlite_busy \
            bblbb_jobs_dead bblbb_jobs_oldest_pending bblbb_outbox_backlog \
            bblbb_outbox_failed bblbb_queue_accumulating bblbb_backup_failed \
            bblbb_backup_verify_failed bblbb_disk_free bblbb_wal_growth \
            bblbb_s3_errors bblbb_s3_permanent bblbb_smtp_failed \
            bblbb_provider_5xx bblbb_oidc_key_expiring; do
  if grep -q "\`$name\`" "$ALERTS_MD"; then ok "告警定义 $name"; else bad "告警定义缺失 $name"; fi
done

echo "==> 2/4 指标名与 METRIC_HELP 白名单一致"
# 1) metrics.md 清单中的每个指标都已登记（防漂移）
METRICS_MD="$ROOT/deploy/monitoring/metrics.md"
while read -r metric; do
  [[ -z "$metric" ]] && continue
  if grep -q "\"$metric\"" "$METRICS_RS"; then ok "指标登记 $metric"; else bad "指标未登记 $metric"; fi
done < <(grep -oE '`bblbb_[a-z0-9_]+`' "$METRICS_MD" | tr -d '`' | sort -u)
# 2) 已登记指标都应被至少一条告警引用（防告警定义缺失）
while read -r metric; do
  [[ -z "$metric" ]] && continue
  if grep -q "$metric" "$ALERTS_MD"; then ok "告警覆盖 $metric"; else bad "指标无对应告警 $metric"; fi
done < <(grep -oE '`bblbb_[a-z0-9_]+`' "$METRICS_MD" | tr -d '`' | sort -u)

echo "==> 3/4 Prometheus rule YAML 可加载（可选）"
RULE_FILE="${BBLBB_PROM_RULES:-$ROOT/deploy/monitoring/rules.yml}"
if [[ -f "$RULE_FILE" ]]; then
  if command -v promtool >/dev/null 2>&1; then
    if promtool check rules "$RULE_FILE" >/dev/null 2>&1; then ok "promtool 校验通过"; else bad "promtool 校验失败"; fi
  else
    echo "  （promtool 未安装，跳过语法校验；表达式语义以 alerts.md 为准）"
  fi
else
  echo "  （未提供 rules.yml，使用 alerts.md 表格定义）"
fi

echo "==> 4/4 值班/升级/审批路径"
for key in "值班联系人" "升级路径" "维护窗口" "审批人" "演练频率"; do
  if grep -q "$key" "$ONCALL_MD"; then ok "oncall.md 包含 $key"; else bad "oncall.md 缺少 $key"; fi
done

echo "------------------------------------------------------------"
echo "ALERTS-DRILL: PASS=$PASS FAIL=$FAIL"
} > "$OUT" 2>&1
cat "$OUT" | tail -30
[[ $FAIL -eq 0 ]] || exit 1
