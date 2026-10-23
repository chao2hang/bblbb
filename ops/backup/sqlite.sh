#!/usr/bin/env bash
# sqlite.sh — SQLite online backup API 一致性备份（M15-BACKUP-01）。
#
# 通过 Python sqlite3 的 Connection.backup() 获取并发写入下的一致快照；绝不
# checkpoint 后裸 cp 活跃数据库。备份文件本身只读（0400），减少意外篡改风险。
#
# 用法：
#   ops/backup/sqlite.sh <db-file> <backup-dir> [--label <name>]
#
# 产出：
#   <backup-dir>/sqlite/<label>/bblbb.sqlite
#   <backup-dir>/sqlite/<label>/bblbb.sqlite.sha256
#   <backup-dir>/sqlite/<label>/backup.json    # 元数据（来源/时间/checksum）
#
# 前置：Python 3 标准库 sqlite3。
set -euo pipefail
umask 077

DB_FILE=""
BACKUP_DIR=""
LABEL="default"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --label) LABEL="$2"; shift 2 ;;
    *) if [[ -z "$DB_FILE" ]]; then DB_FILE="$1"; elif [[ -z "$BACKUP_DIR" ]]; then BACKUP_DIR="$1"; else
         echo "错误: 多余参数 $1" >&2; exit 1
       fi; shift ;;
  esac
done

if [[ -z "$DB_FILE" || -z "$BACKUP_DIR" ]]; then
  echo "用法: ops/backup/sqlite.sh <db-file> <backup-dir> [--label <name>]" >&2
  exit 1
fi
if [[ ! "$LABEL" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$ || "$LABEL" == *..* ]]; then
  echo "错误: label 必须是安全的单层标识符" >&2
  exit 1
fi
if [[ ! -f "$DB_FILE" || -L "$DB_FILE" ]]; then
  echo "错误: 数据库文件不存在或为符号链接: $DB_FILE" >&2
  exit 1
fi
if [[ "$BACKUP_DIR" != /* ]]; then
  echo "错误: backup-dir 必须为绝对路径" >&2
  exit 1
fi
if [[ -e "$BACKUP_DIR" || -L "$BACKUP_DIR" ]]; then
  [[ -d "$BACKUP_DIR" && ! -L "$BACKUP_DIR" ]] || { echo "错误: 备份根目录必须是非符号链接目录" >&2; exit 1; }
else
  mkdir -m 0700 -- "$BACKUP_DIR" || { echo "错误: 无法创建备份根目录" >&2; exit 1; }
fi
BACKUP_DIR="$(cd "$BACKUP_DIR" && pwd -P)"
if [[ -e "$BACKUP_DIR/sqlite" || -L "$BACKUP_DIR/sqlite" ]]; then
  [[ -d "$BACKUP_DIR/sqlite" && ! -L "$BACKUP_DIR/sqlite" ]] || { echo "错误: SQLite 备份目录必须是普通目录" >&2; exit 1; }
else
  mkdir -m 0700 -- "$BACKUP_DIR/sqlite" || { echo "错误: 无法创建 SQLite 备份目录" >&2; exit 1; }
fi
DEST_DIR="$BACKUP_DIR/sqlite/$LABEL"
[[ ! -e "$DEST_DIR" && ! -L "$DEST_DIR" ]] || { echo "错误: 备份目标已存在，拒绝覆盖: $DEST_DIR" >&2; exit 1; }
STAGE_DIR="$(mktemp -d "$BACKUP_DIR/sqlite/.${LABEL}.XXXXXX")" || { echo "错误: 无法创建私有备份暂存目录" >&2; exit 1; }
chmod 0700 "$STAGE_DIR"
STAGE_REAL="$(cd "$STAGE_DIR" && pwd -P)"
[[ "$STAGE_REAL" == "$BACKUP_DIR/sqlite/"* && ! -L "$STAGE_DIR" ]] || { echo "错误: 备份暂存目录路径校验失败" >&2; exit 1; }
TMP_COPY="$STAGE_DIR/bblbb.sqlite"
CHECKSUM_TMP="$STAGE_DIR/bblbb.sqlite.sha256"
META_TMP="$STAGE_DIR/backup.json"
PUBLISHED=0
cleanup_backup_stage() {
  if [[ "$PUBLISHED" == 0 && -n "$STAGE_DIR" && "$STAGE_DIR" == "$BACKUP_DIR/sqlite/.${LABEL}."* && -d "$STAGE_DIR" && ! -L "$STAGE_DIR" ]]; then
    rm -f -- "$TMP_COPY" "$CHECKSUM_TMP" "$META_TMP"
    rmdir -- "$STAGE_DIR" 2>/dev/null || true
  fi
}
trap cleanup_backup_stage EXIT

echo "==> 备份: $DB_FILE → $DEST_DIR"

# SQLite online backup API holds a consistent read snapshot while writers continue.
echo "==> 1/3 SQLite online backup + integrity_check"
python3 - "$DB_FILE" "$TMP_COPY" <<'PY'
import pathlib, sqlite3, sys
source_path, target_path = sys.argv[1:]
source_uri = pathlib.Path(source_path).resolve().as_uri() + "?mode=ro"
source = sqlite3.connect(source_uri, uri=True, timeout=30)
target = sqlite3.connect(target_path, timeout=30)
try:
    source.backup(target, pages=256, sleep=0.05)
    result = target.execute("PRAGMA integrity_check").fetchone()
    if not result or result[0] != "ok":
        raise SystemExit("backup integrity_check failed")
finally:
    target.close()
    source.close()
PY
chmod 0400 "$TMP_COPY"

# 2) Assemble all artifacts privately, then atomically publish the complete set.
echo "==> 2/3 计算 checksum 并组装元数据"
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$TMP_COPY" | awk '{print $1}' > "$CHECKSUM_TMP"
else
  shasum -a 256 "$TMP_COPY" | awk '{print $1}' > "$CHECKSUM_TMP"
fi
chmod 0400 "$TMP_COPY" "$CHECKSUM_TMP"
CHECKSUM="$(<"$CHECKSUM_TMP")"
NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
python3 - "$META_TMP" "$LABEL" "$NOW" "$DB_FILE" "$CHECKSUM" <<'PY'
import json, sys
path, label, timestamp, source, digest = sys.argv[1:]
with open(path, "x", encoding="utf-8") as output:
    json.dump({"tool":"ops/backup/sqlite.sh", "label":label, "backed_up_at":timestamp,
               "source":source, "sha256":digest,
               "snapshot_method":"sqlite3.Connection.backup", "integrity_check":"ok"}, output)
    output.write("\n")
PY
chmod 0400 "$META_TMP"

# Same-filesystem rename publishes database, checksum, and metadata as one directory.
mv -Tn -- "$STAGE_DIR" "$DEST_DIR" || { echo "错误: 无法原子发布完整备份集" >&2; exit 1; }
[[ ! -e "$STAGE_DIR" ]] || { echo "错误: 备份目标已存在或无法无覆盖发布" >&2; exit 1; }
[[ ! -e "$STAGE_DIR" && -d "$DEST_DIR" && ! -L "$DEST_DIR" ]] || { echo "错误: 备份集原子发布未完成" >&2; exit 1; }
PUBLISHED=1

echo "==> 完成: $DEST_DIR/bblbb.sqlite"
echo "    sha256: $CHECKSUM"
