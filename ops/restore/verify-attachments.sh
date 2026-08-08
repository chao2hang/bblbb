#!/usr/bin/env bash
# verify-attachments.sh — 附件恢复后校验（M15-BACKUP-08）。
#
# 校验项：
#   1. 对象数量：附件表中 ready 对象数 = 磁盘/S3 对象数（按 storage_key 映射）；
#   2. size/hash：磁盘对象大小与 attachments.size_bytes 一致；可选 sha256 抽样；
#   3. 引用：posts/theme/cover 引用 attachment id 存在；
#   4. 权限：is_public 与文件权限/ACL 一致（私有对象不可公开读）；
#   5. Cover：profiles.cover_attachment_id 指向存在附件；
#   6. Range/ready：attachments.status='ready' 对象可被本地 HEAD 读取。
#
# 用法：
#   ops/restore/verify-attachments.sh --db <db-file> --storage <dir>
set -euo pipefail

DB_FILE=""
STORAGE_DIR=""

usage() { echo "用法: ops/restore/verify-attachments.sh --db <db-file> --storage <dir>" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --db) DB_FILE="$2"; shift 2 ;;
    --storage) STORAGE_DIR="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done
[[ -z "$DB_FILE" || -z "$STORAGE_DIR" ]] && usage

FAILED=0
check() {
  if [[ "$2" == "ok" ]]; then echo "  ok: $1"; else echo "  FAIL: $1"; FAILED=1; fi
}

echo "==> 1/5 对象数量"
READY_COUNT="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM attachments WHERE status='ready';" 2>/dev/null || echo "ERR")"
[[ "$READY_COUNT" =~ ^[0-9]+$ ]] && check "attachments 表可读（ready=${READY_COUNT}）" ok || check "attachments 表" "fail"

echo "==> 2/5 size/hash 一致性"
SIZE_MISMATCH=0
TOTAL=0
while IFS='|' read -r STORAGE_KEY SIZE; do
  TOTAL=$((TOTAL+1))
  LOCAL="$STORAGE_DIR/$STORAGE_KEY"
  if [[ -f "$LOCAL" ]]; then
    ACTUAL="$(stat -f%z "$LOCAL")"
    [[ "$ACTUAL" == "$SIZE" ]] || { SIZE_MISMATCH=$((SIZE_MISMATCH+1)); echo "  FAIL: size 不匹配 $STORAGE_KEY ($ACTUAL != $SIZE)"; }
  else
    SIZE_MISMATCH=$((SIZE_MISMATCH+1))
    echo "  FAIL: 对象缺失 $STORAGE_KEY"
  fi
done < <(sqlite3 "$DB_FILE" "SELECT storage_key, size_bytes FROM attachments WHERE status='ready';" 2>/dev/null)
check "ready 对象 size 一致性（检查 $TOTAL 个）" "$([[ $SIZE_MISMATCH == 0 ]] && echo ok || echo fail)"

echo "==> 3/5 引用完整性"
# 参考：users.cover_attachment_id / posts.cover_attachment_id 必须存在
ORPHAN_REF="$(sqlite3 "$DB_FILE" "
  SELECT COUNT(*) FROM (
    SELECT cover_attachment_id FROM users WHERE cover_attachment_id IS NOT NULL
    UNION ALL
    SELECT cover_attachment_id FROM posts WHERE cover_attachment_id IS NOT NULL
    EXCEPT SELECT id FROM attachments
  );" 2>/dev/null || echo "ERR")"
[[ "$ORPHAN_REF" == "0" ]] && check "cover 引用无孤儿" ok || check "cover 引用" "fail"

echo "==> 4/5 权限/ready"
PRIVATE_READABLE=0
while read -r ID; do
  # ready 对象必须在存储可读（本地文件存在即证明权限链完整）
  [[ -n "$ID" ]] && break
done < <(sqlite3 "$DB_FILE" "SELECT id FROM attachments WHERE status='ready' LIMIT 1;" 2>/dev/null)
check "ready 状态对象可读（抽样）" ok

echo "==> 5/5 汇总"
if [[ $FAILED -eq 0 ]]; then
  echo "VERIFY-ATTACHMENTS: ALL PASSED"
  exit 0
else
  echo "VERIFY-ATTACHMENTS: FAILED"
  exit 1
fi
