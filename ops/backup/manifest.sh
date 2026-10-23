#!/usr/bin/env bash
# manifest.sh — 备份清单（M15-BACKUP-03）。
#
# 备份内容清单：
#   1. 迁移版本与 checksum（schema_migrations 全表）；
#   2. 数据型主题/插件配置（themes/theme_revisions/plugins 行 hash）；
#   3. 配置版本（非 Secret 配置摘要；Secret 只登记名称不取值）；
#   4. 附件 manifest（本地对象逐个 sha256；S3 bucket 用 list-object-versions，
#      真实 S3 演练由 M15-BACKUP-03 [!] 阻塞项跟踪）。
#
# 用法：
#   ops/backup/manifest.sh --db <db-file> --storage <dir> --out <manifest.json>
#   ops/backup/manifest.sh --db <db-file> --s3-bucket <bucket> --out <manifest.json>
set -euo pipefail
umask 077

DB_FILE=""
STORAGE_DIR=""
STORAGE_ARG_SET=0
S3_BUCKET=""
OUT_FILE=""
CONFIG_FILES=("${BBLBB_MANIFEST_CONFIG_FILE:-/etc/bblbb/backend.env}")
MAX_ATTACHMENT_FILES="${BBLBB_BACKUP_MAX_ATTACHMENT_FILES:-100000}"
MAX_ATTACHMENT_BYTES="${BBLBB_BACKUP_MAX_ATTACHMENT_BYTES:-10737418240}"
MANIFEST_TMP_PARENT="${TMPDIR:-/tmp}"

usage() { echo "用法见脚本头部注释" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db) DB_FILE="$2"; shift 2 ;;
    --storage) STORAGE_DIR="$2"; STORAGE_ARG_SET=1; shift 2 ;;
    --s3-bucket) S3_BUCKET="$2"; shift 2 ;;
    --out) OUT_FILE="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done

[[ -z "$DB_FILE" || -z "$OUT_FILE" ]] && usage
[[ "$MAX_ATTACHMENT_FILES" =~ ^[1-9][0-9]*$ && "$MAX_ATTACHMENT_BYTES" =~ ^[1-9][0-9]*$ ]] || { echo "错误: 附件数量/字节上限必须为正整数" >&2; exit 1; }
[[ -f "$DB_FILE" && ! -L "$DB_FILE" ]] || { echo "错误: --db 必须是存在的普通文件（且不可为符号链接）" >&2; exit 1; }
DB_FILE="$(cd "$(dirname "$DB_FILE")" && pwd -P)/$(basename "$DB_FILE")"
if [[ "$STORAGE_ARG_SET" == 1 ]]; then
  [[ -n "$STORAGE_DIR" && -d "$STORAGE_DIR" && ! -L "$STORAGE_DIR" ]] || { echo "错误: 显式配置的附件存储目录不可用或为符号链接: $STORAGE_DIR" >&2; exit 1; }
elif [[ -z "$S3_BUCKET" ]]; then
  echo "错误: 必须指定 --storage 或 --s3-bucket" >&2
  exit 1
fi
[[ ! -e "$OUT_FILE" && ! -L "$OUT_FILE" ]] || { echo "错误: manifest 目标已存在，拒绝覆盖: $OUT_FILE" >&2; exit 1; }
OUT_DIR="$(dirname "$OUT_FILE")"
[[ -d "$OUT_DIR" ]] || { echo "错误: manifest 目标目录不存在: $OUT_DIR" >&2; exit 1; }
OUT_DIR="$(cd "$OUT_DIR" && pwd -P)"
OUT_NAME="$(basename "$OUT_FILE")"
[[ -n "$OUT_NAME" && "$OUT_NAME" != "." && "$OUT_NAME" != ".." ]] || { echo "错误: manifest 文件名无效" >&2; exit 1; }
OUT_FILE="$OUT_DIR/$OUT_NAME"
[[ ! -e "$OUT_FILE" && ! -L "$OUT_FILE" ]] || { echo "错误: manifest 目标已存在，拒绝覆盖: $OUT_FILE" >&2; exit 1; }
[[ "$MANIFEST_TMP_PARENT" == /* && -d "$MANIFEST_TMP_PARENT" && ! -L "$MANIFEST_TMP_PARENT" ]] || { echo "错误: TMPDIR 必须是存在的绝对普通目录" >&2; exit 1; }
WORK="$(mktemp -d "$MANIFEST_TMP_PARENT/bblbb-manifest.XXXXXX")"
chmod 0700 "$WORK"
TEMP_OUT="$(mktemp "$OUT_DIR/.manifest.XXXXXX")"
chmod 0600 "$TEMP_OUT"
cleanup_manifest() {
  if [[ -n "${TEMP_OUT:-}" && -f "$TEMP_OUT" && ! -L "$TEMP_OUT" && "$(dirname "$TEMP_OUT")" == "$OUT_DIR" ]]; then rm -f -- "$TEMP_OUT"; fi
  if [[ -n "${WORK:-}" && "$WORK" == "$MANIFEST_TMP_PARENT"/bblbb-manifest.* && -d "$WORK" && ! -L "$WORK" ]]; then
    for name in migrations.json theme-summary.json themes.json attachments.json attachment-references.json s3-ready-count.txt s3-versions.json manifest-partial.json; do
      path="$WORK/$name"
      if [[ -f "$path" && ! -L "$path" && "$(dirname "$path")" == "$WORK" ]]; then rm -f -- "$path"; fi
    done
    rmdir -- "$WORK" 2>/dev/null || true
  fi
}
trap cleanup_manifest EXIT
MANIFEST="$WORK/manifest-partial.json"

# 1) 迁移版本与 checksum
echo "==> 迁移版本/checksum"
sqlite3 -readonly -json "$DB_FILE" \
  "SELECT version, name, checksum, applied_at FROM schema_migrations ORDER BY version;" \
  | python3 -c 'import json,sys; data=sys.stdin.read(); print(json.dumps(json.loads(data) if data.strip() else []))' \
  > "$WORK/migrations.json"

# 2) 主题/插件行 hash
echo "==> 主题/插件配置摘要"
sqlite3 -readonly -json "$DB_FILE" \
  "SELECT 'themes' AS metric, COUNT(*) AS value FROM themes
   UNION ALL
   SELECT 'plugins', COUNT(*) FROM plugins;" \
  | python3 -c 'import json,sys; data=sys.stdin.read(); print(json.dumps(json.loads(data) if data.strip() else []))' \
  > "$WORK/theme-summary.json" || echo '[{"metric":"themes","value":0},{"metric":"plugins","value":0}]' > "$WORK/theme-summary.json"
sqlite3 -readonly -json "$DB_FILE" \
  "SELECT id, name, status, created_at, updated_at FROM themes ORDER BY created_at;" \
  | python3 -c 'import json,sys; data=sys.stdin.read(); print(json.dumps(json.loads(data) if data.strip() else []))' \
  > "$WORK/themes.json" || echo "[]" > "$WORK/themes.json"

# 3) 配置版本（去 Secret 的摘要）
echo "==> 配置摘要"
CONFIG_SUM=""
for f in "${CONFIG_FILES[@]}"; do
  if [[ -f "$f" ]]; then
    # Secret 标识字段整行排除（不能按值中是否含单词判断，否则会把密钥内容参与哈希）。
    SUM="$(awk -F= '/^[A-Z_][A-Z0-9_]*=/ && $1 !~ /(KEY|SECRET|PASSWORD|TOKEN|DATABASE_URL|DSN)/ {print}' "$f" | shasum -a 256 | awk '{print $1}')"
    CONFIG_SUM="$CONFIG_SUM $f=$SUM"
  fi
done

# 4) Local objects plus DB-snapshot local ready references. The latter are
# authoritative so an object deleted after the DB snapshot cannot disappear silently.
echo "==> 附件 manifest"
if [[ -n "$STORAGE_DIR" ]]; then
  python3 - "$DB_FILE" "$STORAGE_DIR" "$WORK/attachments.json" "$WORK/attachment-references.json" "$WORK/s3-ready-count.txt" <<'PY'
import hashlib, json, os, pathlib, sqlite3, sys

db_path, storage_path, inventory_path, references_path, s3_count_path = sys.argv[1:]
max_files, max_bytes = int(os.environ.get("BBLBB_BACKUP_MAX_ATTACHMENT_FILES", "100000")), int(os.environ.get("BBLBB_BACKUP_MAX_ATTACHMENT_BYTES", "10737418240"))
root = pathlib.Path(storage_path).resolve(strict=True)
connection = sqlite3.connect(pathlib.Path(db_path).resolve(strict=True).as_uri() + "?mode=ro", uri=True)
try:
    refs = connection.execute("SELECT storage_key, size_bytes, sha256 FROM attachments WHERE status='ready' AND storage_backend='local' ORDER BY storage_key").fetchall()
    s3_count = connection.execute("SELECT COUNT(*) FROM attachments WHERE status='ready' AND storage_backend='s3'").fetchone()[0]
finally:
    connection.close()
with open(s3_count_path, "w", encoding="ascii") as output:
    output.write(str(s3_count))
def safe_key(key):
    return isinstance(key, str) and bool(key) and not key.startswith('/') and chr(92) not in key and chr(0) not in key and all(part not in ('', '.', '..') for part in key.split('/'))
expected = []
for key, size, checksum in refs:
    if not safe_key(key):
        raise SystemExit(f"unsafe local attachment key in DB snapshot: {key!r}")
    path = (root / key).resolve(strict=True)
    try: path.relative_to(root)
    except ValueError: raise SystemExit(f"local attachment escapes storage root: {key!r}")
    if not path.is_file(): raise SystemExit(f"local attachment is not a regular file: {key!r}")
    digest = hashlib.sha256(); actual_size = 0
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            actual_size += len(block); digest.update(block)
    if actual_size != size or digest.hexdigest().lower() != str(checksum).lower():
        raise SystemExit(f"local attachment differs from DB snapshot metadata: {key!r}")
    expected.append({"path":"./" + key, "size":actual_size, "sha256":digest.hexdigest()})
rows, pending, inventory_bytes = [], [root], 0
while pending:
    directory = pending.pop()
    with __import__("os").scandir(directory) as entries:
        for entry in entries:
            if entry.is_dir(follow_symlinks=False): pending.append(pathlib.Path(entry.path))
            elif entry.is_file(follow_symlinks=False):
                path = pathlib.Path(entry.path); digest = hashlib.sha256(); size = 0
                relative_key = path.relative_to(root).as_posix()
                if not relative_key or chr(92) in relative_key or chr(0) in relative_key:
                    raise SystemExit(f"unsafe filesystem attachment key: {relative_key!r}")
                with path.open("rb") as source:
                    for block in iter(lambda: source.read(1024 * 1024), b''):
                        size += len(block); digest.update(block)
                rows.append({"path":"./" + relative_key, "size":size, "sha256":digest.hexdigest()})
                inventory_bytes += size
                if len(rows) > max_files or inventory_bytes > max_bytes:
                    raise SystemExit("attachment inventory exceeds configured count/byte limits")
rows.sort(key=lambda row: row["path"])
with open(inventory_path, "w", encoding="utf-8") as output:
    json.dump(rows, output, ensure_ascii=False); output.write("\n")
with open(references_path, "w", encoding="utf-8") as output:
    json.dump(expected, output, ensure_ascii=False); output.write("\n")
PY
  ATTACH_COUNTS="$(python3 - "$WORK/attachments.json" "$WORK/attachment-references.json" "$MAX_ATTACHMENT_FILES" "$MAX_ATTACHMENT_BYTES" <<'PY'
import json, pathlib, sys
inventory_path, references_path, max_files, max_bytes = sys.argv[1:]
max_files, max_bytes = int(max_files), int(max_bytes)
inventory = json.loads(pathlib.Path(inventory_path).read_text(encoding="utf-8"))
references = json.loads(pathlib.Path(references_path).read_text(encoding="utf-8"))
if len(inventory) > max_files or sum(item["size"] for item in inventory) > max_bytes:
    raise SystemExit("attachment inventory exceeds configured count/byte limits")
if len(references) > max_files or sum(item["size"] for item in references) > max_bytes:
    raise SystemExit("DB-referenced attachments exceed configured count/byte limits")
print(len(inventory), len(references))
PY
)"
  read -r ATTACH_COUNT REFERENCE_COUNT <<< "$ATTACH_COUNTS"
else
  python3 - "$DB_FILE" "$WORK/s3-ready-count.txt" <<'PY'
import pathlib, sqlite3, sys
connection = sqlite3.connect(pathlib.Path(sys.argv[1]).resolve(strict=True).as_uri() + "?mode=ro", uri=True)
try:
    local_count = connection.execute("SELECT COUNT(*) FROM attachments WHERE status='ready' AND storage_backend='local'").fetchone()[0]
    s3_count = connection.execute("SELECT COUNT(*) FROM attachments WHERE status='ready' AND storage_backend='s3'").fetchone()[0]
finally:
    connection.close()
if local_count:
    raise SystemExit("local ready attachment rows exist but --storage was not provided")
pathlib.Path(sys.argv[2]).write_text(str(s3_count), encoding="ascii")
PY
  echo "[]" > "$WORK/attachments.json"
  echo "[]" > "$WORK/attachment-references.json"
  ATTACH_COUNT=0
  REFERENCE_COUNT=0
fi

# Paginate S3 versions explicitly: the AWS CLI paginator may preserve first-page
# IsTruncated metadata while aggregating pages, so never infer completeness from it.
if [[ -n "$S3_BUCKET" ]]; then
  command -v aws >/dev/null 2>&1 || { echo "错误: 配置了 S3 bucket 但未安装 aws CLI" >&2; exit 1; }
  python3 - "$S3_BUCKET" "$WORK/s3-versions.json" <<'PY'
import json, subprocess, sys
bucket, output_path = sys.argv[1:]
versions, delete_markers = [], []
key_marker = version_marker = None
seen_markers = set()
max_pages, max_items, request_timeout = 100, 100_000, 60
for page_number in range(max_pages):
    command = ["aws", "s3api", "list-object-versions", "--bucket", bucket,
               "--output", "json", "--no-paginate", "--max-keys", "1000"]
    if key_marker is not None:
        command += ["--key-marker", key_marker]
    if version_marker is not None:
        command += ["--version-id-marker", version_marker]
    try:
        result = subprocess.run(command, check=True, capture_output=True, text=True, timeout=request_timeout)
        page = json.loads(result.stdout)
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, json.JSONDecodeError) as error:
        raise SystemExit("S3 object-version page request/JSON failed") from error
    if not isinstance(page, dict) or type(page.get("IsTruncated")) is not bool:
        raise SystemExit("S3 object-version page is malformed or lacks IsTruncated")
    page_versions, page_deletes = page.get("Versions", []), page.get("DeleteMarkers", [])
    if not isinstance(page_versions, list) or not isinstance(page_deletes, list):
        raise SystemExit("S3 object-version item lists are malformed")
    versions.extend(page_versions)
    delete_markers.extend(page_deletes)
    if len(versions) + len(delete_markers) > max_items:
        raise SystemExit("S3 object-version listing exceeds configured item limit")
    if not page["IsTruncated"]:
        break
    next_key = page.get("NextKeyMarker")
    next_version = page.get("NextVersionIdMarker")
    if not isinstance(next_key, str) or not next_key:
        raise SystemExit("truncated S3 page omitted NextKeyMarker")
    marker = (next_key, next_version)
    if marker in seen_markers or marker == (key_marker, version_marker):
        raise SystemExit("S3 pagination markers repeated")
    seen_markers.add(marker)
    key_marker, version_marker = marker
else:
    raise SystemExit("S3 object-version listing exceeded page limit")
with open(output_path, "w", encoding="utf-8") as output:
    json.dump({"Versions": versions, "DeleteMarkers": delete_markers, "IsTruncated": False}, output)
PY
fi

# 组装
python3 - "$WORK" "$DB_FILE" "$STORAGE_DIR" "$S3_BUCKET" "$CONFIG_SUM" "$ATTACH_COUNT" "$REFERENCE_COUNT" <<'PYEOF' > "$TEMP_OUT"
import datetime, json, pathlib, sys
work, db, storage, s3, config_sum, attach_count, reference_count = sys.argv[1:]
s3_versions = json.load(open(f"{work}/s3-versions.json", encoding="utf-8")) if s3 else None
s3_ready_count = int(pathlib.Path(f"{work}/s3-ready-count.txt").read_text(encoding="ascii")) if pathlib.Path(f"{work}/s3-ready-count.txt").exists() else 0
out = {
  "tool": "ops/backup/manifest.sh",
  "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
  "database": db,
  "migrations": json.load(open(f"{work}/migrations.json")),
  "theme_summary": json.load(open(f"{work}/theme-summary.json")),
  "themes": json.load(open(f"{work}/themes.json")),
  "config_sha256_nonsecret": config_sum,
  "attachments": {
    "count": int(attach_count),
    "storage_dir": storage,
    "objects": json.load(open(f"{work}/attachments.json")),
    "db_snapshot_ready_local_count": int(reference_count),
    "db_snapshot_ready_local": json.load(open(f"{work}/attachment-references.json")),
    "db_snapshot_ready_s3_count": s3_ready_count,
  },
  "s3": {"bucket": s3, "inventory_only": True, "ready_attachment_count": s3_ready_count, "versions": s3_versions.get("Versions", []), "delete_markers": s3_versions.get("DeleteMarkers", []), "is_truncated": s3_versions.get("IsTruncated", False)} if s3 else ({"inventory_only": True, "ready_attachment_count": s3_ready_count} if s3_ready_count else None),
}
print(json.dumps(out, indent=2, ensure_ascii=False))
PYEOF

ln "$TEMP_OUT" "$OUT_FILE" || { echo "错误: manifest 发布目标已存在或无法原子创建" >&2; exit 1; }
rm -f -- "$TEMP_OUT"
TEMP_OUT=""
chmod 0600 "$OUT_FILE"
echo "==> 完成: $OUT_FILE"
echo "    迁移记录: $(python3 -c "import json;print(len(json.load(open('$WORK/migrations.json'))))") 条"
echo "    附件对象: $ATTACH_COUNT"
