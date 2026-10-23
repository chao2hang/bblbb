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
META_FILE="$(dirname "$BACKUP_FILE")/backup.json"
if [[ -f "$SHA_FILE" ]]; then
  EXPECTED="$(awk 'NR==1 {print $1}' "$SHA_FILE")"
elif [[ -f "$META_FILE" ]]; then
  EXPECTED="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['sha256'])" "$META_FILE")"
else
  echo "错误: 缺少校验和 sidecar 与 backup.json，拒绝恢复未验证备份" >&2
  exit 1
fi
[[ "$EXPECTED" =~ ^[a-fA-F0-9]{64}$ ]] || { echo "错误: 备份校验和格式无效" >&2; exit 1; }
if [[ -f "$SHA_FILE" && -f "$META_FILE" ]]; then
  META_EXPECTED="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['sha256'])" "$META_FILE")"
  [[ "$META_EXPECTED" == "$EXPECTED" ]] || { echo "错误: sidecar 与 backup.json 校验和不一致" >&2; exit 1; }
fi
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$BACKUP_FILE" | awk '{print $1}')"
else
  ACTUAL="$(shasum -a 256 "$BACKUP_FILE" | awk '{print $1}')"
fi
if [[ "$ACTUAL" != "$EXPECTED" ]]; then
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
[[ "$TARGET_DB" != */ ]] || { echo "错误: target-db 不能以斜杠结尾" >&2; exit 1; }
TARGET_DIR="$(dirname "$TARGET_DB")"
[[ -d "$TARGET_DIR" && ! -L "$TARGET_DIR" ]] || { echo "错误: 目标父目录不存在或为符号链接: $TARGET_DIR" >&2; exit 1; }
TARGET_DIR="$(cd "$TARGET_DIR" && pwd -P)"
TARGET_NAME="$(basename "$TARGET_DB")"
[[ "$TARGET_NAME" != "." && "$TARGET_NAME" != ".." && -n "$TARGET_NAME" ]] || { echo "错误: target-db 文件名无效" >&2; exit 1; }
TARGET_DB="$TARGET_DIR/$TARGET_NAME"
[[ ! -L "$TARGET_DB" && ( ! -e "$TARGET_DB" || -f "$TARGET_DB" ) ]] || { echo "错误: 目标数据库必须不存在或为普通文件，不接受目录/符号链接" >&2; exit 1; }
WRITE_TEST="$(mktemp "$TARGET_DIR/.bblbb-restore-write-test.XXXXXX")" || { echo "错误: 目标目录不可写: $TARGET_DIR" >&2; exit 1; }
[[ "$WRITE_TEST" == "$TARGET_DIR/".bblbb-restore-write-test.* && -f "$WRITE_TEST" && ! -L "$WRITE_TEST" ]] || { echo "错误: 写入测试路径校验失败" >&2; exit 1; }
rm -f -- "$WRITE_TEST"

# 4) 原子恢复；若复制/校验被中断，清理仅由本次 mktemp 创建的临时文件。
TMP_TARGET=""
cleanup_restore_temp() {
  if [[ -n "$TMP_TARGET" && "$TMP_TARGET" == "$TARGET_DIR"/.restore.* && -f "$TMP_TARGET" && ! -L "$TMP_TARGET" ]]; then
    rm -f -- "$TMP_TARGET"
  fi
}
trap cleanup_restore_temp EXIT
echo "==> 4/6 原子写入"
TMP_TARGET="$(mktemp "$TARGET_DIR/.restore.XXXXXX")"
[[ "$TMP_TARGET" == "$TARGET_DIR"/.restore.* && -f "$TMP_TARGET" && ! -L "$TMP_TARGET" ]] || { echo "错误: 恢复暂存路径校验失败" >&2; exit 1; }
cp "$BACKUP_FILE" "$TMP_TARGET"
sqlite3 "$TMP_TARGET" "PRAGMA integrity_check;" | grep -q "^ok$"
mv "$TMP_TARGET" "$TARGET_DB"
TMP_TARGET=""
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
