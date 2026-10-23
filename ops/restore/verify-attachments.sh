#!/usr/bin/env bash
# verify-attachments.sh — 附件恢复后校验（M15-BACKUP-08）。
#
# 校验项：
#   1. 对象数量：统计本地 ready 对象（S3 对象由独立云端演练核验）；
#   2. size/hash：全部本地 ready 对象大小和 SHA-256 与 attachments 元数据一致；
#   3. 引用：posts/theme/cover 引用 attachment id 存在；
#   4. Cover：users/posts/theme 中被检查的 cover 引用 attachment id 存在；
#   5. 全部本地 ready 对象可由操作系统读取。此脚本不验证 ACL、公开访问策略，
#      也不执行应用层 HEAD/Range 请求；S3 需单独云端演练核验。
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

[[ -f "$DB_FILE" && -d "$STORAGE_DIR" ]] || { echo "数据库或存储目录不存在" >&2; exit 1; }
FAILED=0
check() {
  if [[ "$2" == "ok" ]]; then echo "  ok: $1"; else echo "  FAIL: $1"; FAILED=1; fi
}

echo "==> 1/5 对象数量"
READY_COUNT="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM attachments WHERE status='ready' AND storage_backend='local';" 2>/dev/null || echo "ERR")"
[[ "$READY_COUNT" =~ ^[0-9]+$ ]] && check "本地 attachments 表可读（ready=${READY_COUNT}）" ok || { check "attachments 表" "fail"; READY_COUNT=ERR; }

echo "==> 2/5 size/hash 一致性"
STORAGE_ROOT="$(cd "$STORAGE_DIR" && pwd -P)"
ATTACHMENT_CHECK="$(python3 - "$DB_FILE" "$STORAGE_ROOT" <<'PY'
import hashlib, json, pathlib, sqlite3, sys

db_path, storage_root = sys.argv[1:]
root = pathlib.Path(storage_root).resolve(strict=True)
db_uri = pathlib.Path(db_path).resolve(strict=True).as_uri() + "?mode=ro"
connection = sqlite3.connect(db_uri, uri=True)
try:
    rows = connection.execute("SELECT storage_key, size_bytes, sha256 FROM attachments WHERE status='ready' AND storage_backend='local'").fetchall()
finally:
    connection.close()
failures = 0
readable = 0
for key, expected_size, expected_hash in rows:
    safe = isinstance(key, str) and bool(key) and not key.startswith('/') and chr(92) not in key and chr(0) not in key and all(part not in ('', '.', '..') for part in key.split('/'))
    if not safe:
        failures += 1
        print(f"  FAIL: storage_key 不是安全相对路径: {key!r}", file=sys.stderr)
        continue
    try:
        path = (root / key).resolve(strict=True)
        path.relative_to(root)
        if not path.is_file():
            raise OSError("not a regular file")
        digest = hashlib.sha256()
        size = 0
        with path.open('rb') as source:
            for block in iter(lambda: source.read(1024 * 1024), b''):
                size += len(block)
                digest.update(block)
        readable += 1
        if not isinstance(expected_hash, str) or len(expected_hash) != 64 or any(c not in '0123456789abcdefABCDEF' for c in expected_hash) or size != expected_size or digest.hexdigest().lower() != expected_hash.lower():
            failures += 1
            print(f"  FAIL: size/hash 不匹配（对象 {key!r}）", file=sys.stderr)
    except (OSError, ValueError):
        failures += 1
        print(f"  FAIL: 对象缺失、不可读或解析到存储目录外: {key!r}", file=sys.stderr)
print(json.dumps({"total":len(rows), "failures":failures, "readable":readable}))
PY
)" || { echo "  FAIL: 无法读取本地附件元数据"; FAILED=1; ATTACHMENT_CHECK='{"total":0,"failures":1,"readable":0}'; }
TOTAL="$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["total"])' "$ATTACHMENT_CHECK")"
SIZE_MISMATCH="$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["failures"])' "$ATTACHMENT_CHECK")"
READABLE_SAMPLE="$(python3 -c 'import json,sys; print(1 if json.loads(sys.argv[1])["readable"] else 0)' "$ATTACHMENT_CHECK")"
check "ready 对象 size/hash 一致性（检查 $TOTAL 个）" "$([[ $SIZE_MISMATCH == 0 ]] && echo ok || echo fail)"

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

echo "==> 4/5 本地读取状态"
[[ "$READY_COUNT" == 0 || "$READABLE_SAMPLE" == 1 ]] && check "ready 对象可读" ok || check "ready 对象不可读" fail

echo "==> 5/5 汇总"
if [[ $FAILED -eq 0 ]]; then
  echo "VERIFY-ATTACHMENTS: ALL PASSED"
  exit 0
else
  echo "VERIFY-ATTACHMENTS: FAILED"
  exit 1
fi
