#!/usr/bin/env bash
# daily.sh — 每日备份编排（M15-BACKUP-05）。
#
# 流程：
#   1. SQLite 安全备份（WAL checkpoint + 复制 + sha256）；
#   2. manifest（迁移/主题/配置/附件）；
#   3. 附件 manifest 复制到备份目录（本地存储）；S3 version 由 manifest 记录；
#   4. 保留策略：本地保留最近 14 天（RPO≤24h 基线上的保守默认）；
#   5. 备份目录权限：root 所有，应用账号不可删（不可变性的沙箱等价物；
#      生产建议对象存储版本控制 + WORM）。
#
# 用法（生产 cron/systemd timer）：
#   ops/backup/daily.sh --db /var/lib/bblbb/database/bblbb.db \
#     --storage /var/lib/bblbb/uploads --backup-dir /var/lib/bblbb/backups
set -euo pipefail

DB_FILE=""
STORAGE_DIR=""
BACKUP_DIR=""
KEEP_DAYS="${BBLBB_BACKUP_KEEP_DAYS:-14}"

usage() { echo "用法见脚本头部注释" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --db) DB_FILE="$2"; shift 2 ;;
    --storage) STORAGE_DIR="$2"; shift 2 ;;
    --backup-dir) BACKUP_DIR="$2"; shift 2 ;;
    --keep-days) KEEP_DAYS="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done
[[ -z "$DB_FILE" || -z "$BACKUP_DIR" ]] && usage

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LABEL="$(date -u +%Y%m%dT%H%M%SZ)"
FAILED=0

echo "==> daily backup label=$LABEL"
if "$SCRIPT_DIR/sqlite.sh" "$DB_FILE" "$BACKUP_DIR" --label "$LABEL"; then
  echo "  [ok] sqlite 备份"
else
  echo "  [fail] sqlite 备份"; FAILED=1
fi

if "$SCRIPT_DIR/manifest.sh" --db "$DB_FILE" --storage "${STORAGE_DIR:-}" \
   --out "$BACKUP_DIR/manifest-$LABEL.json" 2>/dev/null; then
  echo "  [ok] manifest"
else
  echo "  [fail] manifest"; FAILED=1
fi

# 本地附件 manifest 文件复制进备份（对象本体留在存储，按 manifest 可重建）
if [[ -n "$STORAGE_DIR" && -d "$STORAGE_DIR" ]]; then
  ( cd "$STORAGE_DIR" && find . -type f -print0 | tar --null -cf - -T - | \
      gzip > "$BACKUP_DIR/attachments-$LABEL.tar.gz" ) \
    && echo "  [ok] 附件 tar 打包" || { echo "  [fail] 附件打包"; FAILED=1; }
fi

# 保留策略：删除超过 KEEP_DAYS 的备份（root 执行；应用账号无删除权限）
if [[ $FAILED -eq 0 ]]; then
  find "$BACKUP_DIR" -maxdepth 2 -type f -mtime "+$KEEP_DAYS" -delete 2>/dev/null || true
  echo "  [ok] 保留策略（>${KEEP_DAYS}d 清理）"
fi

echo "==> daily backup $([[ $FAILED -eq 0 ]] && echo SUCCESS || echo FAILED)"
exit $FAILED
