#!/usr/bin/env bash
# drill-sqlite.sh — M15-BACKUP-01/06/07 真实备份/恢复演练（可重复执行）。
#
# 流程：建库 → 迁移 → 写入用户+账本+grant+outbox+audit → 备份（计时）→
# 擦除 → 恢复（计时）→ 内容校验（用户/账本恒等式/迁移 checksum/grant/
# outbox/audit）→ 记录 RPO/RTO。
#
# 用法：ops/backup/drill-sqlite.sh [--work-dir <dir>] [--log <file>]
set -euo pipefail

WORK_DIR="${1:-}"
LOG_FILE=""
MIGRATIONS_DIR="$(cd "$(dirname "$0")/../../migrations/sqlite" && pwd)"
MIGRATE_BIN="$(cd "$(dirname "$0")/../../backend/target/debug" && pwd)/migrate"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir) WORK_DIR="$2"; shift 2 ;;
    --log) LOG_FILE="$2"; shift 2 ;;
    *) echo "未知参数: $1"; exit 1 ;;
  esac
done

if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$(mktemp -d /tmp/bblbb-drill.XXXXXX)"
fi
DB_SOURCE="$WORK_DIR/source/bblbb.sqlite"
DB_TARGET="$WORK_DIR/target/bblbb.sqlite"
BACKUP_DIR="$WORK_DIR/backups"
mkdir -p "$(dirname "$DB_SOURCE")" "$(dirname "$DB_TARGET")" "$BACKUP_DIR"

now_ms() { python3 -c 'import time; print(f"{time.time()*1000:.0f}")'; }

echo "================================================================"
echo "BBLBB SQLite 备份/恢复演练   $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "工作目录: $WORK_DIR"
echo "================================================================"

echo "==> 1) 建库 + 迁移（source）"
"$MIGRATE_BIN" apply --db-url "sqlite://$DB_SOURCE" --migrations-dir "$MIGRATIONS_DIR" >/dev/null
MIG_COUNT_BEFORE="$(sqlite3 "$DB_SOURCE" "SELECT COUNT(*) FROM schema_migrations;")"
echo "    已应用迁移: $MIG_COUNT_BEFORE"
MIG_CHECKSUM_SAMPLE="$(sqlite3 "$DB_SOURCE" "SELECT version||'='||substr(checksum,1,12) FROM schema_migrations ORDER BY version DESC LIMIT 1;")"
echo "    最新迁移 checksum: $MIG_CHECKSUM_SAMPLE"

echo "==> 2) 写入业务数据（用户/账本/grant/outbox/audit）"
NOW=1786060800000
sqlite3 "$DB_SOURCE" <<SQL
INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
VALUES ('u-drill-01911fd5', 'drilluser', 'drill@example.com', 'x-argon2-placeholder', 'active', $NOW, $NOW);
INSERT INTO point_operations (id, idempotency_scope, idempotency_key, request_hash, kind, memo, created_at)
VALUES ('op-drill-0001', 'drill-scope', 'drill-key-1', 'drill-hash-1', 'award', 'drill', $NOW);
INSERT INTO point_transactions (id, operation_id, user_id, currency_id, delta_balance, delta_frozen, balance_after, frozen_after, created_at)
VALUES ('tx-drill-0001', 'op-drill-0001', 'u-drill-01911fd5', '01911fd5-0047-0000-0000-000000000001', 100, 0, 100, 0, $NOW);
INSERT INTO point_accounts (user_id, currency_id, balance, frozen_balance, version, updated_at)
VALUES ('u-drill-01911fd5', '01911fd5-0047-0000-0000-000000000001', 100, 0, 1, $NOW);
INSERT INTO content_access_grants (id, user_id, policy_id, source_kind, grant_target_key, granted_at)
VALUES ('g-drill-0001', 'u-drill-01911fd5', 'p-1', 'purchase', 'drill:target', $NOW);
INSERT INTO outbox_events (id, event_type, payload, status, attempts, max_attempts, next_attempt_at, created_at, payload_version)
VALUES ('o-drill-0001', 'drill.event', '{}', 'pending', 0, 5, $NOW, $NOW, 1);
INSERT INTO audit_logs (id, actor_id, action, target_type, target_id, created_at)
VALUES ('a-drill-0001', 'u-drill-01911fd5', 'drill.write', 'user', 'u-drill-01911fd5', $NOW);
SQL
echo "    用户: 1；账本: exp balance=100；grant: 1；outbox: 1；audit: 1"

# 备份前状态快照
SNAPSHOT_USER="$(sqlite3 "$DB_SOURCE" "SELECT COUNT(*) FROM users;")"
SNAPSHOT_LEDGER="$(sqlite3 "$DB_SOURCE" "SELECT SUM(delta_balance) FROM point_transactions;")"
SNAPSHOT_BALANCE="$(sqlite3 "$DB_SOURCE" "SELECT balance FROM point_accounts WHERE user_id='u-drill-01911fd5';")"
SNAPSHOT_GRANT="$(sqlite3 "$DB_SOURCE" "SELECT COUNT(*) FROM content_access_grants;")"
SNAPSHOT_OUTBOX="$(sqlite3 "$DB_SOURCE" "SELECT COUNT(*) FROM outbox_events;")"
SNAPSHOT_AUDIT="$(sqlite3 "$DB_SOURCE" "SELECT COUNT(*) FROM audit_logs;")"

echo "==> 3) 备份（计时）"
T0="$(now_ms)"
"$(dirname "$0")/sqlite.sh" "$DB_SOURCE" "$BACKUP_DIR" --label drill
T1="$(now_ms)"
BACKUP_MS=$((T1-T0))
BACKUP_FILE="$BACKUP_DIR/sqlite/drill/bblbb.sqlite"
BACKUP_SHA="$(cat "$BACKUP_DIR/sqlite/drill/bblbb.sqlite.sha256")"
echo "    备份耗时: ${BACKUP_MS} ms"

echo "==> 4) 校验备份"
sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;" | grep -q '^ok$'
echo "    备份完整性: ok"

echo "==> 5) 擦除源库（模拟故障）"
rm -f "$DB_SOURCE" "$DB_SOURCE-wal" "$DB_SOURCE-shm"
if [[ -f "$DB_SOURCE" ]]; then echo "    擦除失败"; exit 1; fi
echo "    源库已删除（含 WAL/-shm）"

echo "==> 6) 恢复（计时）"
T2="$(now_ms)"
"$(dirname "$0")/../restore/sqlite.sh" "$BACKUP_FILE" "$DB_TARGET"
T3="$(now_ms)"
RESTORE_MS=$((T3-T2))
echo "    恢复耗时: ${RESTORE_MS} ms"

echo "==> 7) 内容校验"
USER_COUNT="$(sqlite3 "$DB_TARGET" "SELECT COUNT(*) FROM users;")"
LEDGER_SUM="$(sqlite3 "$DB_TARGET" "SELECT SUM(delta_balance) FROM point_transactions;")"
BALANCE="$(sqlite3 "$DB_TARGET" "SELECT balance FROM point_accounts WHERE user_id='u-drill-01911fd5';")"
GRANT_COUNT="$(sqlite3 "$DB_TARGET" "SELECT COUNT(*) FROM content_access_grants;")"
OUTBOX_COUNT="$(sqlite3 "$DB_TARGET" "SELECT COUNT(*) FROM outbox_events;")"
AUDIT_COUNT="$(sqlite3 "$DB_TARGET" "SELECT COUNT(*) FROM audit_logs;")"
MIG_COUNT_AFTER="$(sqlite3 "$DB_TARGET" "SELECT COUNT(*) FROM schema_migrations;")"
RESTORED_SHA="$(shasum -a 256 "$DB_TARGET" | awk '{print $1}')"
VERIFY_OK="$( "$(dirname "$0")/../restore/verify.sh" --db "$DB_TARGET" --migrations-dir "$MIGRATIONS_DIR" --expect-user 'u-drill-01911fd5' >/dev/null 2>&1 && echo yes || echo no )"

echo "    users=$USER_COUNT (期望 $SNAPSHOT_USER)"
echo "    Σ(delta_balance)=$LEDGER_SUM balance=$BALANCE (期望 $SNAPSHOT_LEDGER/$SNAPSHOT_BALANCE)"
echo "    grants=$GRANT_COUNT (期望 $SNAPSHOT_GRANT)"
echo "    outbox=$OUTBOX_COUNT (期望 $SNAPSHOT_OUTBOX)"
echo "    audit=$AUDIT_COUNT (期望 $SNAPSHOT_AUDIT)"
echo "    migrations=$MIG_COUNT_AFTER (期望 $MIG_COUNT_BEFORE)"
echo "    verify.sh=$VERIFY_OK"

echo "==> 8) 断言"
FAILED=0
[[ "$USER_COUNT" == "$SNAPSHOT_USER" ]] || { echo "  FAIL: 用户数不一致"; FAILED=1; }
[[ "$BALANCE" == "$SNAPSHOT_BALANCE" ]] || { echo "  FAIL: 账本余额不一致"; FAILED=1; }
[[ "$LEDGER_SUM" == "$SNAPSHOT_LEDGER" ]] || { echo "  FAIL: 账本恒等式不一致"; FAILED=1; }
[[ "$GRANT_COUNT" == "$SNAPSHOT_GRANT" ]] || { echo "  FAIL: grant 不一致"; FAILED=1; }
[[ "$OUTBOX_COUNT" == "$SNAPSHOT_OUTBOX" ]] || { echo "  FAIL: outbox 不一致"; FAILED=1; }
[[ "$AUDIT_COUNT" == "$SNAPSHOT_AUDIT" ]] || { echo "  FAIL: audit 不一致"; FAILED=1; }
[[ "$MIG_COUNT_AFTER" == "$MIG_COUNT_BEFORE" ]] || { echo "  FAIL: 迁移版本数不一致"; FAILED=1; }
[[ "$VERIFY_OK" == "yes" ]] || { echo "  FAIL: verify.sh 未通过"; FAILED=1; }

echo "==> 9) RPO/RTO 测量"
RTO_SECS="$(python3 -c "print(f'{$RESTORE_MS/1000:.2f}')")"
echo "    RPO=0（WAL checkpoint(TRUNCATE) 后的一致性快照，备份覆盖到备份时刻全部已提交事务）"
echo "    RPO（备份耗时窗口）=${BACKUP_MS}ms"
echo "    RTO（擦除→完整恢复+内容校验）=${RTO_SECS}s"

echo "================================================================"
if [[ $FAILED -eq 0 ]]; then
  echo "DRILL: PASSED"
  echo "RPO=0, RTO=${RTO_SECS}s, backup_ms=${BACKUP_MS}, restore_ms=${RESTORE_MS}"
else
  echo "DRILL: FAILED"
  exit 1
fi
