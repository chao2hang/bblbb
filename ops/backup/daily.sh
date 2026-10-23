#!/usr/bin/env bash
# daily.sh — 每日备份编排（M15-BACKUP-05）。
#
# 流程：
#   1. SQLite 安全备份（SQLite online backup API + integrity_check + sha256）；
#   2. manifest（迁移/主题/配置/附件）；
#   3. 本地附件归档至备份目录（S3 version 由 manifest 记录）；
#   4. 自动保留清理暂时暂停，KEEP_DAYS 仅作兼容提示；
#   5. 备份目录 mode 0700；生产需独立保留策略、容量告警与异地副本。
#
# 用法（生产 cron/systemd timer）：
#   ops/backup/daily.sh --db /var/lib/bblbb/database/bblbb.db \
#     --storage /var/lib/bblbb/uploads --backup-dir /var/lib/bblbb/backups
set -euo pipefail

DB_FILE=""
STORAGE_DIR=""
STORAGE_ARG_SET=0
APP_STORAGE_BACKEND="${BBLBB__STORAGE_BACKEND:-}"
BACKUP_STORAGE_BACKEND="${BBLBB_BACKUP_STORAGE_BACKEND:-}"
if [[ -n "$APP_STORAGE_BACKEND" && -n "$BACKUP_STORAGE_BACKEND" && "$APP_STORAGE_BACKEND" != "$BACKUP_STORAGE_BACKEND" ]]; then
  echo "BBLBB_BACKUP_STORAGE_BACKEND 与应用 BBLBB__STORAGE_BACKEND 配置冲突" >&2
  exit 1
fi
CONFIGURED_STORAGE_BACKEND="${BACKUP_STORAGE_BACKEND:-$APP_STORAGE_BACKEND}"
STORAGE_BACKEND="$CONFIGURED_STORAGE_BACKEND"
STORAGE_BACKEND_SET=0
STORAGE_BACKEND_ARG_SET=0
[[ -n "$STORAGE_BACKEND" ]] && STORAGE_BACKEND_SET=1
S3_BUCKET="${BBLBB_BACKUP_S3_BUCKET:-${BBLBB__S3_BUCKET:-}}"
BACKUP_DIR=""
LABEL=""
KEEP_DAYS="${BBLBB_BACKUP_KEEP_DAYS:-14}"
RETENTION_POLICY="${BBLBB_BACKUP_RETENTION_POLICY:-}"
MAX_ATTACHMENT_FILES="${BBLBB_BACKUP_MAX_ATTACHMENT_FILES:-100000}"
MAX_ATTACHMENT_BYTES="${BBLBB_BACKUP_MAX_ATTACHMENT_BYTES:-10737418240}"
MIN_ATTACHMENT_FREE_BYTES="${BBLBB_BACKUP_MIN_ATTACHMENT_FREE_BYTES:-268435456}"

usage() { echo "用法见脚本头部注释" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --db) DB_FILE="$2"; shift 2 ;;
    --storage) STORAGE_DIR="$2"; STORAGE_ARG_SET=1; shift 2 ;;
    --storage-backend) STORAGE_BACKEND="$2"; STORAGE_BACKEND_SET=1; STORAGE_BACKEND_ARG_SET=1; shift 2 ;;
    --s3-bucket) S3_BUCKET="$2"; shift 2 ;;
    --backup-dir) BACKUP_DIR="$2"; shift 2 ;;
    --label) LABEL="$2"; shift 2 ;;
    --keep-days) KEEP_DAYS="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done
[[ -z "$DB_FILE" || -z "$BACKUP_DIR" ]] && usage
if [[ -z "$LABEL" ]]; then LABEL="$(date -u +%Y%m%dT%H%M%SZ)"; fi
[[ "$LABEL" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$ && "$LABEL" != *..* ]] || { echo "备份 label 必须是安全的单层标识符" >&2; exit 1; }
if [[ "$STORAGE_BACKEND_SET" == 0 ]]; then
  if [[ "$STORAGE_ARG_SET" == 1 ]]; then STORAGE_BACKEND=local
  elif [[ -n "$S3_BUCKET" ]]; then STORAGE_BACKEND=s3
  else STORAGE_BACKEND=local
  fi
elif [[ "$STORAGE_BACKEND_ARG_SET" == 1 && -n "$CONFIGURED_STORAGE_BACKEND" && "$STORAGE_BACKEND" != "$CONFIGURED_STORAGE_BACKEND" ]]; then
  echo "--storage-backend 与配置的 BBLBB__STORAGE_BACKEND 冲突" >&2
  exit 1
fi
if [[ "$STORAGE_ARG_SET" == 0 && "$STORAGE_BACKEND" == local ]]; then
  STORAGE_DIR="${BBLBB_BACKUP_STORAGE_DIR:-${BBLBB__STORAGE_DIR:-/var/lib/bblbb/uploads}}"
fi
if [[ -n "$STORAGE_DIR" ]]; then
  [[ -d "$STORAGE_DIR" && ! -L "$STORAGE_DIR" ]] || { echo "拒绝不可用或符号链接存储路径: $STORAGE_DIR" >&2; exit 1; }
fi
case "$STORAGE_BACKEND" in
  local)
    [[ -n "$STORAGE_DIR" ]] || { echo "local storage backend requires a storage directory" >&2; exit 1; }
    ;;
  s3)
    [[ -n "$S3_BUCKET" ]] || { echo "S3 storage backend requires BBLBB_BACKUP_S3_BUCKET or --s3-bucket" >&2; exit 1; }
    echo "S3 data backup is not implemented: refusing to report a successful DB-only backup" >&2
    exit 1
    ;;
  *) echo "不支持的 BBLBB__STORAGE_BACKEND: $STORAGE_BACKEND" >&2; exit 1 ;;
esac
[[ "$BACKUP_DIR" == /* && "$BACKUP_DIR" != "/" && "$BACKUP_DIR" == */backups ]] || { echo "拒绝不安全备份目录（必须是绝对路径且以 /backups 结尾）" >&2; exit 1; }
BACKUP_DIR="$(python3 - "$BACKUP_DIR" <<'PY'
import os, pathlib, stat, sys
path = pathlib.Path(sys.argv[1])
if ".." in path.parts:
    raise SystemExit("backup path may not contain '..'")
current = pathlib.Path(path.anchor)
for component in path.parts[1:]:
    current = current / component
    try:
        mode = current.lstat().st_mode
    except FileNotFoundError:
        continue
    if stat.S_ISLNK(mode):
        raise SystemExit(f"backup path has symlink component: {current}")
print(path)
PY
)" || { echo "拒绝包含符号链接或上级跳转的备份目录" >&2; exit 1; }
[[ "$KEEP_DAYS" =~ ^[0-9]+$ ]] || { echo "--keep-days 必须为非负整数" >&2; exit 1; }
[[ "$RETENTION_POLICY" =~ ^[A-Za-z0-9][A-Za-z0-9._:/+-]{2,255}$ ]] || { echo "自动清理暂停；必须配置 BBLBB_BACKUP_RETENTION_POLICY=<已部署的外部 retention job/策略标识>，并确保磁盘容量告警已启用" >&2; exit 1; }
[[ "$MAX_ATTACHMENT_FILES" =~ ^[1-9][0-9]*$ && "$MAX_ATTACHMENT_BYTES" =~ ^[1-9][0-9]*$ && "$MIN_ATTACHMENT_FREE_BYTES" =~ ^[0-9]+$ ]] || { echo "附件归档限制/剩余空间阈值必须为整数" >&2; exit 1; }
umask 077
mkdir -p -m 0700 "$BACKUP_DIR"
[[ -d "$BACKUP_DIR" && ! -L "$BACKUP_DIR" ]] || { echo "拒绝备份目录符号链接或非目录" >&2; exit 1; }
chmod 0700 "$BACKUP_DIR"
BACKUP_DIR="$(cd "$BACKUP_DIR" && pwd -P)"
[[ "$BACKUP_DIR" == */backups ]] || { echo "备份目录 realpath 必须以 /backups 结尾" >&2; exit 1; }
command -v flock >/dev/null 2>&1 || { echo "需要 flock 支持以防并行 daily backup" >&2; exit 1; }
exec 9>"$BACKUP_DIR/.daily-backup.lock"
flock -n 9 || { echo "已有 daily backup 在运行，拒绝并行执行" >&2; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FAILED=0

[[ -f "$DB_FILE" && ! -L "$DB_FILE" ]] || { echo "数据库文件不可用或为符号链接: $DB_FILE" >&2; exit 1; }
BACKUP_RESERVE_BYTES="${BBLBB_BACKUP_MIN_FREE_BYTES:-$MIN_ATTACHMENT_FREE_BYTES}"
[[ "$BACKUP_RESERVE_BYTES" =~ ^[0-9]+$ ]] || { echo "BBLBB_BACKUP_MIN_FREE_BYTES 必须为非负整数" >&2; exit 1; }
DB_BYTES="$(python3 - "$DB_FILE" <<'PY'
import os, sys
path = sys.argv[1]
size = os.stat(path, follow_symlinks=False).st_size
for suffix in ("-wal",):
    sidecar = path + suffix
    if os.path.exists(sidecar) and not os.path.islink(sidecar):
        size += os.stat(sidecar, follow_symlinks=False).st_size
print(size)
PY
)"
ATTACH_STATS="$(python3 - "$DB_FILE" "$STORAGE_DIR" "$MAX_ATTACHMENT_FILES" "$MAX_ATTACHMENT_BYTES" <<'PY'
import os, pathlib, sqlite3, sys

def scan(root, max_files, max_bytes):
    files = total = 0
    pending = [root]
    while pending:
        directory = pending.pop()
        with os.scandir(directory) as entries:
            for entry in entries:
                if entry.is_dir(follow_symlinks=False): pending.append(pathlib.Path(entry.path))
                elif entry.is_file(follow_symlinks=False):
                    files += 1; total += entry.stat(follow_symlinks=False).st_size
                    if files > max_files or total > max_bytes:
                        raise SystemExit("attachment count/size exceeds configured limit")
    return files, total
root = pathlib.Path(sys.argv[2]).resolve(strict=True)
max_files, max_bytes = map(int, sys.argv[3:])
db_uri = pathlib.Path(sys.argv[1]).resolve(strict=True).as_uri() + "?mode=ro"
connection = sqlite3.connect(db_uri, uri=True)
try:
    rows = connection.execute("SELECT storage_key FROM attachments WHERE status='ready' AND storage_backend='local'").fetchall()
    s3_count = connection.execute("SELECT COUNT(*) FROM attachments WHERE status='ready' AND storage_backend='s3'").fetchone()[0]
finally:
    connection.close()
if s3_count:
    raise SystemExit(f"database snapshot candidate has {s3_count} ready S3-backed attachments; object bytes are not backed up")
if len(rows) > max_files:
    raise SystemExit("ready local attachment references exceed configured file limit")
referenced_bytes = 0
for (key,) in rows:
    if not isinstance(key, str) or not key or key.startswith('/') or chr(92) in key or any(part in ('', '.', '..') for part in key.split('/')):
        raise SystemExit(f"unsafe local storage key in database: {key!r}")
    path = (root / key).resolve(strict=True)
    try: path.relative_to(root)
    except ValueError: raise SystemExit(f"local storage key escapes storage root: {key!r}")
    if not path.is_file(): raise SystemExit(f"local storage key is not a regular file: {key!r}")
    referenced_bytes += path.stat().st_size
    if referenced_bytes > max_bytes:
        raise SystemExit("referenced attachment bytes exceed configured size limit")
files, total = scan(root, max_files, max_bytes)
print(files, total, referenced_bytes)
PY
)" || { echo "附件/DB 引用预检失败：超过限制或扫描失败" >&2; exit 1; }
read -r ATTACH_FILES ATTACH_BYTES REFERENCED_ATTACHMENT_BYTES <<< "$ATTACH_STATS"
[[ "$DB_BYTES" =~ ^[0-9]+$ ]] || { echo "无法估算 SQLite/WAL 大小" >&2; exit 1; }
FREE_BYTES="$(df -Pk "$BACKUP_DIR" | awk 'NR==2 {printf "%.0f", $4 * 1024}')"
[[ "$FREE_BYTES" =~ ^[0-9]+$ ]] || { echo "无法读取备份卷剩余空间" >&2; exit 1; }
REQUIRED_BYTES=$((DB_BYTES + ATTACH_BYTES + BACKUP_RESERVE_BYTES))
if (( FREE_BYTES < REQUIRED_BYTES )); then
  echo "备份卷空间不足：需要至少 ${REQUIRED_BYTES} bytes（含 DB/WAL、附件与 reserve），当前 ${FREE_BYTES}" >&2
  exit 1
fi

echo "==> daily backup label=$LABEL (preflight db=$DB_BYTES attachments=$ATTACH_BYTES free=$FREE_BYTES)"
if "$SCRIPT_DIR/sqlite.sh" "$DB_FILE" "$BACKUP_DIR" --label "$LABEL"; then
  echo "  [ok] sqlite 备份"
else
  echo "  [fail] sqlite 备份"; FAILED=1
fi

BACKUP_DB="$BACKUP_DIR/sqlite/$LABEL/bblbb.sqlite"
if [[ $FAILED -eq 0 && -f "$BACKUP_DB" && ! -L "$BACKUP_DB" ]]; then
  MANIFEST_ARGS=(--db "$BACKUP_DB" --storage "${STORAGE_DIR:-}" --out "$BACKUP_DIR/manifest-$LABEL.json")
  if [[ -n "$S3_BUCKET" ]]; then MANIFEST_ARGS+=(--s3-bucket "$S3_BUCKET"); fi
  if BBLBB_BACKUP_MAX_ATTACHMENT_FILES="$MAX_ATTACHMENT_FILES" BBLBB_BACKUP_MAX_ATTACHMENT_BYTES="$MAX_ATTACHMENT_BYTES" "$SCRIPT_DIR/manifest.sh" "${MANIFEST_ARGS[@]}"; then
    echo "  [ok] manifest（database snapshot）"
  else
    echo "  [fail] manifest"; FAILED=1
  fi
else
  echo "  [fail] SQLite snapshot 不可用，跳过 manifest"; FAILED=1
fi
if [[ $FAILED -eq 0 && -f "$BACKUP_DIR/manifest-$LABEL.json" ]]; then
  SNAPSHOT_S3_READY_COUNT="$(python3 - "$BACKUP_DIR/manifest-$LABEL.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as source:
    manifest = json.load(source)
print(int(manifest["attachments"]["db_snapshot_ready_s3_count"]))
PY
)" || FAILED=1
  if [[ ! "$SNAPSHOT_S3_READY_COUNT" =~ ^[0-9]+$ ]] || (( SNAPSHOT_S3_READY_COUNT > 0 )); then
    echo "  [fail] SQLite snapshot 含 ready S3 attachment；对象字节备份未实现，拒绝报告成功"; FAILED=1
  fi
fi

# 本地附件对象打包归档（不跟随符号链接；超过上限则拒绝归档）
if [[ -n "$STORAGE_DIR" && -d "$STORAGE_DIR" ]]; then
  ATTACH_FINAL="$BACKUP_DIR/attachments-$LABEL.tar.gz"
  if [[ -e "$ATTACH_FINAL" || -L "$ATTACH_FINAL" ]]; then echo "  [fail] 附件归档目标已存在"; FAILED=1; fi
  STORAGE_REAL="$(cd "$STORAGE_DIR" && pwd -P)"
  export MAX_ATTACHMENT_FILES MAX_ATTACHMENT_BYTES
  if [[ $FAILED -eq 0 && ! -e "$ATTACH_FINAL" && ! -L "$ATTACH_FINAL" ]]; then
    ATTACH_TMP="$(mktemp "$BACKUP_DIR/.attachments-$LABEL.XXXXXX")"
    ATTACH_TMP_REAL="$(realpath -e "$ATTACH_TMP")"
    cleanup_attachment_temp() {
      if [[ -n "${ATTACH_TMP:-}" && -f "$ATTACH_TMP" && ! -L "$ATTACH_TMP" && "$ATTACH_TMP_REAL" == "$BACKUP_DIR"/.attachments-* ]]; then rm -f -- "$ATTACH_TMP"; fi
    }
    trap cleanup_attachment_temp EXIT
    ATTACH_MAX_OUTPUT_BYTES="$(python3 - "$ATTACH_FILES" "$ATTACH_BYTES" <<'PY'
import sys
files, raw_bytes = map(int, sys.argv[1:])
print(raw_bytes + files * 8192 + 16 * 1024 * 1024)
PY
)"
    FREE_BYTES="$(df -Pk "$BACKUP_DIR" | awk 'NR==2 {printf "%.0f", $4 * 1024}')"
    [[ "$FREE_BYTES" =~ ^[0-9]+$ ]] || { echo "  [fail] 无法读取备份卷剩余空间"; FAILED=1; FREE_BYTES=0; }
    if (( FREE_BYTES < ATTACH_MAX_OUTPUT_BYTES + BACKUP_RESERVE_BYTES )); then
      echo "  [fail] 归档卷当前剩余空间不足，拒绝创建附件归档"; FAILED=1
    fi
    if [[ $FAILED -eq 0 ]]; then
      python3 "$SCRIPT_DIR/archive-attachments.py" --manifest "$BACKUP_DIR/manifest-$LABEL.json" \
        --storage "$STORAGE_REAL" --output "$ATTACH_TMP" \
        --max-files "$MAX_ATTACHMENT_FILES" --max-bytes "$MAX_ATTACHMENT_BYTES" \
        --max-output-bytes "$ATTACH_MAX_OUTPUT_BYTES" || FAILED=1
    fi
    if [[ $FAILED -eq 0 ]]; then
      if python3 - "$BACKUP_DIR/manifest-$LABEL.json" "$ATTACH_TMP" <<'PY'
import hashlib, json, pathlib, sys, tarfile
manifest_path, archive_path = sys.argv[1:]
manifest = json.load(open(manifest_path, encoding="utf-8"))
def normalized_items(items, label):
    result = {}
    for item in items:
        path = item.get("path")
        if not isinstance(path, str) or not path.startswith("./"):
            raise SystemExit(f"unsafe {label} path in manifest")
        key = path[2:]
        parts = pathlib.PurePosixPath(key).parts
        if not key or pathlib.PurePosixPath(key).is_absolute() or ".." in parts or chr(92) in key or key in result:
            raise SystemExit(f"unsafe or duplicate {label} path in manifest")
        result[key] = item
    return result
inventory = normalized_items(manifest["attachments"]["objects"], "inventory")
expected = normalized_items(manifest["attachments"]["db_snapshot_ready_local"], "database reference")
actual = {}
with tarfile.open(archive_path, "r:gz") as archive:
    for member in archive:
        if member.isdir():
            continue
        if not member.isfile() or not member.name.startswith("./"):
            raise SystemExit("unexpected tar member type or path")
        key = member.name[2:]
        parts = pathlib.PurePosixPath(key).parts
        if not key or pathlib.PurePosixPath(key).is_absolute() or ".." in parts or "\\" in key or key in actual:
            raise SystemExit("archive contains unsafe or duplicate member path")
        item = inventory.get(key)
        if item is None:
            raise SystemExit("archive contains file absent from manifest inventory")
        stream = archive.extractfile(member)
        if stream is None:
            raise SystemExit("archive member unreadable")
        digest = hashlib.sha256()
        size = 0
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            size += len(block)
            digest.update(block)
        if size != item["size"] or digest.hexdigest() != item["sha256"]:
            raise SystemExit("archive content differs from filesystem inventory manifest")
        actual[key] = item
if set(actual) != set(inventory):
    raise SystemExit("archive file set differs from filesystem inventory manifest")
for key, item in expected.items():
    archived = actual.get(key)
    if archived is None or archived["size"] != item["size"] or archived["sha256"] != item["sha256"]:
        raise SystemExit("archive is missing or differs from DB-snapshot attachment references")
PY
      then
        chmod 0400 "$ATTACH_TMP"
        if ! ln -- "$ATTACH_TMP" "$ATTACH_FINAL"; then
          echo "  [fail] 附件归档无法无覆盖发布"; FAILED=1
        else
          rm -f -- "$ATTACH_TMP"
          ATTACH_TMP=""
          echo "  [ok] 附件 tar 打包（files=$ATTACH_FILES bytes=$ATTACH_BYTES）"
        fi
      else
        echo "  [fail] 附件归档与快照 manifest 不一致"; FAILED=1
      fi
    else
      echo "  [fail] 附件打包"; FAILED=1
    fi
  fi
fi

# Do not automatically prune backups yet: historical root-level filenames were
# not exclusively tool-owned, and partial sets require an operator-reviewed
# retention plan. KEEP_DAYS remains accepted for compatibility but is advisory.
# The manifest reads the published DB snapshot; storage can still mutate during
# tar creation, so strict DB+object cross-time consistency requires a maintenance
# window or a storage snapshot facility and is not claimed by this job.
if [[ $FAILED -eq 0 ]]; then
  echo "  [info] 自动保留清理暂停；KEEP_DAYS=${KEEP_DAYS}d 仅作提示，待完整套件 dry-run 策略落地"
fi

if [[ $FAILED -ne 0 ]]; then
  echo "  [warn] 本次备份未完整成功；保留已生成的隔离 label=$LABEL 产物供操作员核查，不自动删除历史/部分集合"
fi
echo "==> daily backup $([[ $FAILED -eq 0 ]] && echo SUCCESS || echo FAILED)"
exit $FAILED
