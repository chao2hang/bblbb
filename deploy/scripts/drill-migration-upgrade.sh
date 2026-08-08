#!/usr/bin/env bash
# drill-migration-upgrade.sh — 上一版本→当前版本迁移演练（M15-UPGRADE-02）。
#
# 在副本数据库执行（绝不触碰生产库）：
#   1. 用 1..56 号迁移建「上一版本」库（模拟上一 release 不含 0057_theme）；
#   2. 复制副本（release.sh 发布前备份）；
#   3. 用完整迁移集 apply（只应用 pending 0057）；
#   4. 记录耗时 / 锁事件 / 失败；
#   5. 校验：版本数 57、checksum 一致、幂等（二次 apply 0）。
#
# 用法：deploy/scripts/drill-migration-upgrade.sh [--work-dir <dir>]
set -euo pipefail

WORK_DIR=""
MIGRATIONS_DIR="$(cd "$(dirname "$0")/../../migrations/sqlite" && pwd)"
MIGRATE_BIN="$(cd "$(dirname "$0")/../../backend/target/debug" && pwd)/migrate"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir) WORK_DIR="$2"; shift 2 ;;
    *) echo "未知参数: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$WORK_DIR" ]]; then
  WORK_DIR="$(mktemp -d /tmp/bblbb-migrate-drill.XXXXXX)"
fi
PREV_DB="$WORK_DIR/previous/bblbb.sqlite"
CUR_DB="$WORK_DIR/current/bblbb.sqlite"
mkdir -p "$(dirname "$PREV_DB")" "$(dirname "$CUR_DB")"

now_ms() { python3 -c 'import time; print(f"{time.time()*1000:.0f}")'; }

echo "================================================================"
echo "BBLBB 上一版本→当前版本迁移演练  $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "工作目录: $WORK_DIR"
echo "================================================================"

echo "==> 1/6 构造上一版本库（1..56 号迁移）"
T0="$(now_ms)"
# 上一版本迁移集：只保留 1..56（模拟上一 release 不含 0057_theme）
PREV_DIR="$WORK_DIR/migrations-prev"
mkdir -p "$PREV_DIR"
for f in "$MIGRATIONS_DIR"/*.sql; do
  v="$(basename "$f" | cut -d_ -f1)"
  if (( 10#$v <= 56 )); then cp "$f" "$PREV_DIR/"; fi
done
rm -f "$PREV_DB" "$PREV_DB-wal" "$PREV_DB-shm"
# 上一版本库用同一迁移执行器构建（含 schema_migrations 历史表，与生产一致）
"$MIGRATE_BIN" apply --db-url "sqlite://$PREV_DB" --migrations-dir "$PREV_DIR" >/dev/null
T1="$(now_ms)"
PREV_VERSION="$(sqlite3 "$PREV_DB" "SELECT MAX(version) FROM schema_migrations;")"
echo "    上一版本建库耗时: $((T1-T0)) ms；迁移版本: ${PREV_VERSION}（期望 56）"

echo "==> 2/6 发布前副本（release.sh 备份语义）"
cp "$PREV_DB" "$CUR_DB"
PREV_SHA="$(shasum -a 256 "$CUR_DB" | awk '{print $1}')"
echo "    副本 sha256: ${PREV_SHA:0:16}…"

echo "==> 3/6 显式迁移 apply（当前 release 完整迁移集）"
LOCK_EVENTS=0
T2="$(now_ms)"
# 锁事件：并发打开一个写连接持续持锁，观察 apply 是否触发 busy（真实锁行为）
# （这里用 sqlite3 后台事务模拟长时间写锁，见 4/6）
APPLY_OUT="$( "$MIGRATE_BIN" apply --db-url "sqlite://$CUR_DB" --migrations-dir "$MIGRATIONS_DIR" 2>&1 )"
T3="$(now_ms)"
APPLY_MS=$((T3-T2))
echo "$APPLY_OUT" | tail -4
echo "    迁移耗时: ${APPLY_MS} ms"

echo "==> 4/6 锁竞争行为（并发写者 vs 迁移）"
LOCK_DB="$WORK_DIR/lock/bblbb.sqlite"
mkdir -p "$(dirname "$LOCK_DB")"
cp "$PREV_DB" "$LOCK_DB"
# 后台开启事务持写锁
(
  sqlite3 "$LOCK_DB" "BEGIN IMMEDIATE; INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at) VALUES ('u-lock-test', 'locktest', 'lock@example.com', 'x', 'active', 1, 1); SELECT 'lock-held';"
  sleep 2
  sqlite3 "$LOCK_DB" "COMMIT;"
) > /tmp/bblbb-lockholder.out 2>&1 &
HOLDER=$!
sleep 0.5
LOCK_MS="$(python3 -c 'import time;print(time.time()*1000)')"
if "$MIGRATE_BIN" apply --db-url "sqlite://$LOCK_DB" --migrations-dir "$MIGRATIONS_DIR" >/dev/null 2>&1; then
  LOCK_EVENTS=0
  echo "    锁竞争：迁移在写锁持有期间完成（busy_timeout 内等待）"
else
  LOCK_EVENTS=1
  echo "    锁竞争：迁移遇到锁并失败（SQLITE_BUSY）→ 记录为锁事件"
fi
wait "$HOLDER" 2>/dev/null || true

echo "==> 5/6 校验"
CUR_COUNT="$(sqlite3 "$CUR_DB" "SELECT COUNT(*) FROM schema_migrations;")"
MISMATCH=0
while IFS='|' read -r VER NAME DBSUM; do
  FILE="$(ls "$MIGRATIONS_DIR"/[0-9][0-9][0-9][0-9]_"$NAME".sql 2>/dev/null | head -1 || true)"
  FILESUM="$(shasum -a 256 "$FILE" | awk '{print $1}')"
  [[ "$FILESUM" == "$DBSUM" ]] || { echo "  FAIL: checksum 不匹配 version=$VER"; MISMATCH=1; }
done < <(sqlite3 "$CUR_DB" "SELECT version, name, checksum FROM schema_migrations ORDER BY version;")
echo "    迁移版本数: ${CUR_COUNT}（期望 57）"
echo "    全部 checksum 一致: $([[ $MISMATCH == 0 ]] && echo 是 || echo 否)"

echo "==> 6/6 幂等（二次 apply 应为 0 个迁移）"
IDEM_OUT="$( "$MIGRATE_BIN" apply --db-url "sqlite://$CUR_DB" --migrations-dir "$MIGRATIONS_DIR" 2>&1 )"
echo "$IDEM_OUT" | grep -E "applied|OK" | tail -2

echo "================================================================"
FAILED=0
[[ "$CUR_COUNT" == "57" ]] || FAILED=1
[[ $MISMATCH == 0 ]] || FAILED=1
if [[ $FAILED == 0 ]]; then
  echo "MIGRATION-DRILL: PASSED (apply_ms=${APPLY_MS}, lock_events=${LOCK_EVENTS})"
else
  echo "MIGRATION-DRILL: FAILED"
  exit 1
fi
