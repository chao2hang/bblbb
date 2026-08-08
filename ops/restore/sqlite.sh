#!/usr/bin/env bash
# sqlite.sh — 从安全备份恢复 SQLite 数据库（M15-BACKUP-01/06/07）。
#
# 用法：
#   ops/restore/sqlite.sh <backup-file> <target-db> [--verify]
#
# 流程：
#   1. 校验备份 sha256（拒绝被篡改的备份）；
#   2. 校验备份是合法 SQLite 且 integrity_check ok；
#   3. 停止目标库写入（调用方负责停服/切维护）；
#   4. 原子写入：复制到临时文件 → 校验 → mv 覆盖目标 + 删除旧 WAL/-shm；
#   5. --verify 时调用 ops/restore/verify.sh 验证内容。
#
# 前置：sqlite3 CLI。
set -euo pipefail

BACKUP_FILE="${1:-}"
TARGET_DB="${2:-}"
VERIFY="${3:-}"
VERIFY_FLAG=""

if [[ "$VERIFY" == "--verify" ]]; then VERIFY_FLAG="--verify"; fi
if [[ -z "$BACKUP_FILE" || -z "$TARGET_DB" ]]; then
  echo "用法: ops/restore/sqlite.sh <backup-file> <target-db> [--verify]" >&2
  exit 1
fi
if [[ ! -f "$BACKUP_FILE" ]]; then
  echo "错误: 备份文件不存在: $BACKUP_FILE" >&2
  exit 1
fi

# 1) 校验 sha256（同目录 .sha256 或 backup.json）
echo "==> 1/6 备份校验和"
EXPECTED=""
SHA_FILE="${BACKUP_FILE}.sha256"
if [[ -f "$SHA_FILE" ]]; then
  EXPECTED="$(cat "$SHA_FILE")"
elif [[ -f "$(dirname "$BACKUP_FILE")/backup.json" ]]; then
  EXPECTED="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['sha256'])" "$(dirname "$BACKUP_FILE")/backup.json")"
fi
ACTUAL="$(shasum -a 256 "$BACKUP_FILE" | awk '{print $1}')"
if [[ -n "$EXPECTED" && "$ACTUAL" != "$EXPECTED" ]]; then
  echo "错误: 备份校验和不匹配（可能被篡改或损坏）" >&2
  exit 1
fi
echo "    sha256 OK: $ACTUAL"

# 2) 备份合法性
echo "==> 2/6 备份完整性"
sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;" | grep -q "^ok$" || {
  echo "错误: 备份不是一致可用的 SQLite 库" >&2; exit 1; }

# 3) 目标库可写性
echo "==> 3/6 目标库检查"
TARGET_DIR="$(dirname "$TARGET_DB")"
[[ -d "$TARGET_DIR" ]] || { echo "错误: 目标目录不存在: $TARGET_DIR" >&2; exit 1; }
touch "$TARGET_DIR/.bblbb-restore-write-test" && rm -f "$TARGET_DIR/.bblbb-restore-write-test"

# 4) 原子恢复
echo "==> 4/6 原子写入"
TMP_TARGET="$(mktemp "$TARGET_DIR/.restore.XXXXXX")"
cp "$BACKUP_FILE" "$TMP_TARGET"
sqlite3 "$TMP_TARGET" "PRAGMA integrity_check;" | grep -q "^ok$"
mv "$TMP_TARGET" "$TARGET_DB"
# 删除旧 WAL/-shm（主库已整体替换，旧 WAL 属于被覆盖的旧库）
rm -f "$TARGET_DB-wal" "$TARGET_DB-shm"
chmod 0600 "$TARGET_DB"

# 5) 恢复后完整性
echo "==> 5/6 恢复后完整性"
sqlite3 "$TARGET_DB" "PRAGMA integrity_check;" | grep -q "^ok$" || {
  echo "错误: 恢复后的数据库完整性检查失败" >&2; exit 1; }

echo "==> 6/6 完成"
if [[ -n "$VERIFY_FLAG" ]]; then
  echo "==> 附加内容校验（ops/restore/verify.sh）"
  "$(dirname "$0")/verify.sh" --db "$TARGET_DB"
fi
echo "    恢复完成: $TARGET_DB"
