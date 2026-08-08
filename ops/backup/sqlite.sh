#!/usr/bin/env bash
# sqlite.sh — SQLite WAL checkpoint + 安全备份（M15-BACKUP-01）。
#
# 绝不直接复制活跃数据库文件：先 `PRAGMA wal_checkpoint(TRUNCATE)` 把 WAL
# 合并回主库并截断 WAL，校验 checkpoint 后 WAL 无残留帧，再复制主库 + 计算
# sha256。备份文件本身只读（0400），保证备份不被篡改。
#
# 用法：
#   ops/backup/sqlite.sh <db-file> <backup-dir> [--label <name>]
#
# 产出：
#   <backup-dir>/sqlite/<label>/bblbb.sqlite
#   <backup-dir>/sqlite/<label>/bblbb.sqlite.sha256
#   <backup-dir>/sqlite/<label>/backup.json    # 元数据（来源/时间/checksum）
#
# 前置：sqlite3 CLI（SQLite >= 3.22，支持 wal_checkpoint(TRUNCATE)）。
set -euo pipefail

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
if [[ ! -f "$DB_FILE" ]]; then
  echo "错误: 数据库文件不存在: $DB_FILE" >&2
  exit 1
fi

DEST_DIR="$BACKUP_DIR/sqlite/$LABEL"
mkdir -p "$DEST_DIR"

echo "==> 备份: $DB_FILE → $DEST_DIR"

# 1) WAL checkpoint + TRUNCATE（合并 + 截断 WAL）
echo "==> 1/5 WAL checkpoint(TRUNCATE)"
sqlite3 "$DB_FILE" "PRAGMA wal_checkpoint(TRUNCATE);"

# 2) 校验 checkpoint 后 WAL 无残留帧（busy 时拒绝备份，绝不复制不一致状态）
echo "==> 2/5 校验 WAL 状态"
WAL_STATE="$(sqlite3 "$DB_FILE" "PRAGMA wal_checkpoint(PASSIVE);")"
# 输出格式（管道分隔）: busy|log|checkpointed；log=-1 表示无 WAL 文件
IFS='|' read -r BUSY_FRAMES LOG_FRAMES CHECKPOINTED <<< "$WAL_STATE"
if [[ "$BUSY_FRAMES" != "0" ]] || { [[ "$LOG_FRAMES" != "0" && "$LOG_FRAMES" != "-1" ]]; }; then
  echo "错误: WAL 仍包含 ${LOG_FRAMES} 帧（busy=${BUSY_FRAMES}），数据库繁忙，放弃备份" >&2
  exit 1
fi

# 3) 完整性检查（备份前）
echo "==> 3/5 integrity_check"
sqlite3 "$DB_FILE" "PRAGMA integrity_check;" | grep -q "^ok$"

# 4) 复制主库（WAL 已合并截断，此时复制即一致快照）
echo "==> 4/5 复制主库 + 计算 sha256"
TMP_COPY="$DEST_DIR/.bblbb.sqlite.tmp"
cp "$DB_FILE" "$TMP_COPY"
shasum -a 256 "$TMP_COPY" | awk '{print $1}' > "$DEST_DIR/bblbb.sqlite.sha256"
mv "$TMP_COPY" "$DEST_DIR/bblbb.sqlite"
chmod 0400 "$DEST_DIR/bblbb.sqlite" "$DEST_DIR/bblbb.sqlite.sha256"

# 5) 元数据
echo "==> 5/5 元数据"
NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
CHECKSUM="$(cat "$DEST_DIR/bblbb.sqlite.sha256")"
cat > "$DEST_DIR/backup.json" <<EOF
{
  "tool": "ops/backup/sqlite.sh",
  "label": "$LABEL",
  "backed_up_at": "$NOW",
  "source": "$DB_FILE",
  "sha256": "$CHECKSUM",
  "wal_checkpoint": "truncate",
  "pre_backup_integrity": "ok"
}
EOF
chmod 0400 "$DEST_DIR/backup.json"

echo "==> 完成: $DEST_DIR/bblbb.sqlite"
echo "    sha256: $CHECKSUM"
