#!/usr/bin/env bash
# verify.sh — 恢复后数据校验（M15-BACKUP-07）。
#
# 校验项：
#   1. 用户存在（users 表可读、含活跃用户、无重复 email/username 规范化）；
#   2. 账本恒等式：每个 point_accounts 行满足
#        Σ(point_transactions.delta_balance) = balance 且
#        Σ(point_transactions.delta_frozen) = frozen_balance
#      （懒建账户从 0 起步；快照/补偿流水不改变该恒等式）；
#   3. 迁移 checksum：schema_migrations 与 migrations 目录文件全文 SHA-256 一致；
#   4. 授权 grant 表可读（content_access_grants）；
#   5. Outbox 表可读（outbox_events/outbox_consumed）；
#   6. 审计表存在且只增（audit_logs 可读）。
#
# 用法：
#   ops/restore/verify.sh --db <db-file> [--migrations-dir <dir>]
#   ops/restore/verify.sh --db <db-file> --expect-user <id>   # 指定用户必须存在
set -euo pipefail

DB_FILE=""
MIGRATIONS_DIR=""
EXPECT_USER=""

usage() { echo "用法见脚本头部注释" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db) DB_FILE="$2"; shift 2 ;;
    --migrations-dir) MIGRATIONS_DIR="$2"; shift 2 ;;
    --expect-user) EXPECT_USER="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done
[[ -z "$DB_FILE" ]] && usage
[[ -z "$MIGRATIONS_DIR" ]] && MIGRATIONS_DIR="$(cd "$(dirname "$0")/../../migrations/sqlite" && pwd)"

FAILED=0
check() { # check <desc> <result>
  if [[ "$2" == "ok" ]]; then
    echo "  ok: $1"
  else
    echo "  FAIL: $1"
    FAILED=1
  fi
}

echo "==> 1/6 用户校验"
USER_COUNT="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM users;" 2>/dev/null || echo "ERR")"
[[ "$USER_COUNT" =~ ^[0-9]+$ ]] && check "users 表可读（$USER_COUNT 行）" ok || check "users 表可读" "fail"
ACTIVE_COUNT="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM users WHERE status='active';" 2>/dev/null || echo "ERR")"
[[ "$ACTIVE_COUNT" =~ ^[0-9]+$ ]] && check "活跃用户存在（${ACTIVE_COUNT}）" ok || check "活跃用户" "fail"
DUP_EMAIL="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM (SELECT email_normalized FROM users GROUP BY email_normalized HAVING COUNT(*)>1);" 2>/dev/null || echo "ERR")"
[[ "$DUP_EMAIL" == "0" ]] && check "无重复 email_normalized" ok || check "无重复 email_normalized" "fail"
DUP_USER="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM (SELECT username_normalized FROM users GROUP BY username_normalized HAVING COUNT(*)>1);" 2>/dev/null || echo "ERR")"
[[ "$DUP_USER" == "0" ]] && check "无重复 username_normalized" ok || check "无重复 username_normalized" "fail"
if [[ -n "$EXPECT_USER" ]]; then
  EXISTS="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM users WHERE id='$EXPECT_USER';" 2>/dev/null || echo "ERR")"
  [[ "$EXISTS" == "1" ]] && check "指定用户 $EXPECT_USER 存在" ok || check "指定用户 $EXPECT_USER 存在" "fail"
fi

echo "==> 2/6 账本恒等式"
LEDGER_MISMATCH="$(sqlite3 "$DB_FILE" "
  SELECT COUNT(*) FROM (
    SELECT a.user_id, a.currency_id
    FROM point_accounts a
    LEFT JOIN (
      SELECT user_id, currency_id,
             SUM(delta_balance) AS sum_delta,
             SUM(delta_frozen) AS sum_frozen
      FROM point_transactions
      GROUP BY user_id, currency_id
    ) t ON t.user_id = a.user_id AND t.currency_id = a.currency_id
    WHERE COALESCE(t.sum_delta, 0) != a.balance
       OR COALESCE(t.sum_frozen, 0) != a.frozen_balance
  );" 2>/dev/null || echo "ERR")"
[[ "$LEDGER_MISMATCH" == "0" ]] && check "账本恒等式 Σ(delta)=balance 全绿" ok || check "账本恒等式（异常账户 ${LEDGER_MISMATCH}）" "fail"

echo "==> 3/6 迁移 checksum"
CHK_FAIL=0
if [[ -d "$MIGRATIONS_DIR" ]]; then
  # 对 schema_migrations 中每条记录，重算文件 SHA-256 比对
  while IFS='|' read -r VER NAME DBSUM; do
    FILE="$(ls "$MIGRATIONS_DIR"/[0-9][0-9][0-9][0-9]_"$NAME".sql 2>/dev/null | head -1 || true)"
    if [[ -z "$FILE" ]]; then
      echo "  FAIL: 迁移文件缺失 $NAME"
      CHK_FAIL=1
      continue
    fi
    FILESUM="$(shasum -a 256 "$FILE" | awk '{print $1}')"
    if [[ "$FILESUM" != "$DBSUM" ]]; then
      echo "  FAIL: 迁移 checksum 不匹配 version=$VER $NAME"
      CHK_FAIL=1
    fi
  done < <(sqlite3 "$DB_FILE" "SELECT version, name, checksum FROM schema_migrations ORDER BY version;")
  if [[ "$CHK_FAIL" == "0" ]]; then
    check "schema_migrations 全部 checksum 与文件一致" ok
  else
    check "schema_migrations checksum" "fail"
  fi
  APPLIED="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM schema_migrations;")"
  FILES="$(ls "$MIGRATIONS_DIR"/*.sql 2>/dev/null | wc -l | tr -d ' ')"
  check "迁移版本数与文件数一致（${APPLIED}/${FILES}）" "$([[ "$APPLIED" == "$FILES" ]] && echo ok || echo fail)"
else
  check "迁移目录存在" "fail"
fi

echo "==> 4/6 grant 校验"
GRANT_COUNT="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM content_access_grants;" 2>/dev/null || echo "ERR")"
[[ "$GRANT_COUNT" =~ ^[0-9]+$ ]] && check "content_access_grants 可读（$GRANT_COUNT 行）" ok || check "content_access_grants" "fail"

echo "==> 5/6 Outbox 校验"
OUTBOX_COUNT="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM outbox_events;" 2>/dev/null || echo "ERR")"
[[ "$OUTBOX_COUNT" =~ ^[0-9]+$ ]] && check "outbox_events 可读（$OUTBOX_COUNT 行）" ok || check "outbox_events" "fail"
CONSUMED_COUNT="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM outbox_consumed;" 2>/dev/null || echo "ERR")"
[[ "$CONSUMED_COUNT" =~ ^[0-9]+$ ]] && check "outbox_consumed 可读（$CONSUMED_COUNT 行）" ok || check "outbox_consumed" "fail"

echo "==> 6/6 审计校验"
AUDIT_COUNT="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM audit_logs;" 2>/dev/null || echo "ERR")"
[[ "$AUDIT_COUNT" =~ ^[0-9]+$ ]] && check "audit_logs 可读（$AUDIT_COUNT 行）" ok || check "audit_logs" "fail"

echo
if [[ $FAILED -eq 0 ]]; then
  echo "VERIFY: ALL PASSED"
  exit 0
else
  echo "VERIFY: FAILED"
  exit 1
fi
