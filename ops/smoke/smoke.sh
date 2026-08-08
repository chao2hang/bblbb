#!/usr/bin/env bash
# smoke.sh — 发布后冒烟（M15-UPGRADE-07）。
#
# 覆盖：db（/healthz + /readyz）、登录、发帖、回复、附件（create+stream+complete）、
# 账本、管理 API 权限门。
#
# 前置：
#   - 运行中的后端（BASE_URL 默认 http://127.0.0.1:8080）
#   - 数据库可写（--db 用于标记 email_verified 与账本抽查）
#
# 用法：
#   ops/smoke/smoke.sh [--base-url <url>] [--db <sqlite-file>]
set -euo pipefail

BASE_URL="${BBLBB_SMOKE_BASE_URL:-http://127.0.0.1:8080}"
DB="${BBLBB_SMOKE_DB:-}"
BOARD_ID="01911fd5-f000-7561-a2a5-3dd6434157f0"   # 种子板块 general
PASS=0
FAIL=0
TS="$(date +%s)"
USERNAME="smoke$TS"
EMAIL="smoke$TS@example.com"
PASSWORD='Smoke-pass-123!'
# 与 PASSWORD 匹配的真实 argon2id 哈希（由后端注册生成后固化，供种子用户复用；
# 仅用于冒烟 DB，不承载生产凭据）
SEED_HASH='$argon2id$v=19$m=19456,t=2,p=1$sOtIYaUslkrvrQY2Y7eHQg$VZah1ZwehiLgYUy1VFGNIXK8TwV1k6+NX7OTSgMckEE'
JAR="$(mktemp)"

ok()   { echo "  ok: $*"; PASS=$((PASS+1)); }
bad()  { echo "  FAIL: $*" >&2; FAIL=$((FAIL+1)); }
req()  { # req <method> <path> [--json <data>] [--data <raw>] [-H <header>]
  local method="$1" path="$2"; shift 2
  local args=(-s -X "$method" -b "$JAR" -c "$JAR" -H "Accept: application/json")
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --json) args+=(-H "Content-Type: application/json" --data "$2"); shift 2 ;;
      --data) args+=(--data-binary "$2"); shift 2 ;;
      -H) args+=(-H "$2"); shift 2 ;;
      *) echo "req: 未知参数 $1" >&2; exit 1 ;;
    esac
  done
  curl "${args[@]}" "$BASE_URL$path"
}
json_field() { python3 -c "import json,sys;d=json.load(sys.stdin);print(d$1)"; }
http_status() { # http_status <method> <url> [curl args...]
  local method="$1"; local url="$2"; shift 2
  curl -s -o /dev/null -w '%{http_code}' -X "$method" -b "$JAR" -c "$JAR" -H "Accept: application/json" "$@" "$url"
}

echo "==> 1/7 db（healthz + readyz）"
HTTP="$(http_status GET "$BASE_URL/healthz")"
[[ "$HTTP" == "200" ]] && ok "/healthz 200" || bad "/healthz → $HTTP"
READY="$(req GET /readyz)"
echo "$READY" | grep -q '"status":"ok"' && ok "/readyz ok" || bad "/readyz 未就绪: $READY"

echo "==> 2/7 登录（预认证 CSRF + 种子用户 + login）"
# 冒烟用户直接种子（argon2 hash 与 PASSWORD 匹配；避免消耗注册 IP 限流
# 3 次/小时，保证可重复运行）。真实注册流程由验证邮件链路测试覆盖。
if [[ -n "$DB" ]]; then
  NOW="$(date +%s000)"
  # authz 决策读取 email_verified_at（enforce.rs）+ 24h 冷静期（ACCOUNT_COOLDOWN_MS）：
  # 种子用户的 email_verified_at 必须早于 24h 前，否则被 InCooldown 拒绝。
  # created_at 设为 30 天前：风险规则 new_user_grace=7d 不命中（rules.rs），
  # 否则新账号发帖进入 pending_review（draft），无法立即回复。
  VERIFIED_AT=$((NOW - 25*3600*1000))
  CREATED_AT=$((NOW - 30*24*3600*1000))
  sqlite3 "$DB" "INSERT OR IGNORE INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified, email_verified_at, created_at, updated_at, timezone)
     VALUES ('u-smoke-$TS', '$USERNAME', '$EMAIL', '$SEED_HASH', 'active', 1, $VERIFIED_AT, $CREATED_AT, $NOW, 'UTC');
     INSERT OR IGNORE INTO point_operations (id, idempotency_scope, idempotency_key, request_hash, kind, memo, created_at)
       VALUES ('op-smoke-$TS', 'smoke', 'smoke-key-$TS', 'smoke-hash-$TS', 'award', 'smoke', $NOW);
     INSERT OR IGNORE INTO point_transactions (id, operation_id, user_id, currency_id, delta_balance, delta_frozen, balance_after, frozen_after, created_at)
       VALUES ('tx-smoke-$TS', 'op-smoke-$TS', 'u-smoke-$TS', '01911fd5-0047-0000-0000-000000000001', 100, 0, 100, 0, $NOW);
     INSERT OR IGNORE INTO point_accounts (user_id, currency_id, balance, frozen_balance, version, updated_at)
       VALUES ('u-smoke-$TS', '01911fd5-0047-0000-0000-000000000001', 100, 0, 1, $NOW);"
  ok "种子用户 ${USERNAME}（argon2 密码哈希，注册限流不触发）"
else
  CSRF="$(req GET /api/v1/auth/csrf | json_field "['token']")"
  [[ -n "$CSRF" ]] && ok "预认证 CSRF token" || bad "CSRF token 获取失败"
  REG_HTTP="$(http_status POST "$BASE_URL/api/v1/auth/register" -H "X-CSRF-Token: $CSRF" -H "Content-Type: application/json" \
    --data "{\"username\":\"$USERNAME\",\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}")"
  [[ "$REG_HTTP" == "201" || "$REG_HTTP" == "200" ]] && ok "注册 $USERNAME → $REG_HTTP" || bad "注册 → $REG_HTTP"
fi
# 登录（预认证 CSRF 在 cookie jar 中；无 DB 时也先取 CSRF）
CSRF="${CSRF:-$(req GET /api/v1/auth/csrf | json_field "['token']")}"
LOGIN_HTTP="$(http_status POST "$BASE_URL/api/v1/auth/login" -H "X-CSRF-Token: $CSRF" -H "Content-Type: application/json" \
  --data "{\"identifier\":\"$USERNAME\",\"password\":\"$PASSWORD\"}")"
[[ "$LOGIN_HTTP" == "200" ]] && ok "登录 → 200（会话 cookie 已注入）" || bad "登录 → $LOGIN_HTTP"
SESS_CSRF="$(req GET /api/v1/auth/csrf | json_field "['token']")"
[[ -n "$SESS_CSRF" ]] && ok "会话 CSRF token" || bad "会话 CSRF 失败"

echo "==> 3/7 发帖"
# 正文必须带唯一 token：风险 duplicate_rule 在 7 天窗口内识别同指纹正文
# （其他作者）→ pending_review，重复正文会进人工队列导致无法回复。
POST_JSON="$(req POST /api/v1/posts --json "{\"type\":\"discussion\",\"title\":\"smoke post $TS\",\"markdown\":\"smoke body $TS\",\"board_id\":\"$BOARD_ID\",\"access_policy\":\"public\",\"client_request_id\":\"smoke-$TS-1\"}" -H "X-CSRF-Token: $SESS_CSRF")"
POST_ID="$(echo "$POST_JSON" | json_field "['id']" 2>/dev/null || echo "")"
if [[ -n "$POST_ID" && "$POST_ID" != "None" ]]; then
  ok "发帖 → $POST_ID"
else
  bad "发帖失败: $POST_JSON"
fi

echo "==> 4/7 回复"
if [[ -n "$POST_ID" ]]; then
  REPLY_JSON="$(req POST "/api/v1/posts/$POST_ID/comments" --json "{\"markdown\":\"smoke reply\",\"client_request_id\":\"smoke-$TS-2\"}" -H "X-CSRF-Token: $SESS_CSRF")"
  REPLY_ID="$(echo "$REPLY_JSON" | json_field "['id']" 2>/dev/null || echo "")"
  [[ -n "$REPLY_ID" && "$REPLY_ID" != "None" ]] && ok "回复 → $REPLY_ID" || bad "回复失败: $REPLY_JSON"
fi

echo "==> 5/7 附件（create + stream + complete）"
ATTACH_JSON="$(req POST /api/v1/attachments --json "{\"filename\":\"smoke.txt\",\"size\":11,\"declared_media_type\":\"text/plain\"}" -H "X-CSRF-Token: $SESS_CSRF" -H "Idempotency-Key: smoke-attach-$TS")"
ATTACH_ID="$(echo "$ATTACH_JSON" | json_field "['attachment']['id']" 2>/dev/null || echo "")"
if [[ -n "$ATTACH_ID" && "$ATTACH_ID" != "None" ]]; then
  ok "附件 create → $ATTACH_ID"
  UPLOAD_HTTP="$(http_status PUT "$BASE_URL/api/v1/attachments/$ATTACH_ID" --data "hello world" -H "Content-Type: text/plain" -H "X-CSRF-Token: $SESS_CSRF")"
  [[ "$UPLOAD_HTTP" == "200" ]] && ok "附件 stream → 200" || bad "附件 stream → $UPLOAD_HTTP"
  COMPLETE_HTTP="$(http_status POST "$BASE_URL/api/v1/attachments/$ATTACH_ID/complete" -H "X-CSRF-Token: $SESS_CSRF" -H "Content-Type: application/json" \
    --data "{\"client_request_id\":\"smoke-$TS-3\"}")"
  [[ "$COMPLETE_HTTP" == "200" ]] && ok "附件 complete → 200" || bad "附件 complete → $COMPLETE_HTTP"
else
  bad "附件 create 失败: $ATTACH_JSON"
fi

echo "==> 6/7 账本"
if [[ -n "$DB" ]]; then
  USER_ID="$(sqlite3 "$DB" "SELECT id FROM users WHERE username_normalized='$USERNAME';")"
  BALANCE="$(sqlite3 "$DB" "SELECT balance FROM point_accounts WHERE user_id='$USER_ID' AND currency_id='01911fd5-0047-0000-0000-000000000001';")"
  [[ -n "$BALANCE" ]] && ok "账本余额可读（exp=${BALANCE}）" || bad "账本账户不存在"
  SUM_DELTA="$(sqlite3 "$DB" "SELECT COALESCE(SUM(delta_balance),0) FROM point_transactions WHERE user_id='$USER_ID';")"
  [[ "$SUM_DELTA" == "$BALANCE" ]] && ok "账本恒等式 Σ(delta)=balance ($SUM_DELTA)" || bad "账本恒等式不成立"
  ACT_HTTP="$(http_status GET "$BASE_URL/api/v1/activity/summary")"
  [[ "$ACT_HTTP" == "200" ]] && ok "activity summary API → 200" || bad "activity summary → $ACT_HTTP"
fi

echo "==> 7/7 管理 API 权限门"
ADMIN_HTTP="$(http_status GET "$BASE_URL/api/v1/admin/users")"
if [[ "$ADMIN_HTTP" == "403" || "$ADMIN_HTTP" == "401" ]]; then
  ok "admin API 权限门生效（非 admin → ${ADMIN_HTTP}）"
else
  bad "admin API 权限门异常 → ${ADMIN_HTTP}（预期 403/401）"
fi

rm -f "$JAR"
echo "SMOKE: PASS=$PASS FAIL=$FAIL"
[[ $FAIL -eq 0 ]] || exit 1
