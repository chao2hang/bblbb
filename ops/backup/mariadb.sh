#!/usr/bin/env bash
# mariadb.sh — MariaDB 10.11 一致备份（M15-BACKUP-02）。
#
# MariaDB 与 MySQL 分开执行（工具/语法差异），恢复分别测试。
# 真实 MariaDB 服务器演练依赖外部基础设施（M15-BACKUP-02 [!] 阻塞项）；
# 本脚本就绪供生产使用：mariadb-dump + gzip + AES-256 加密 + sha256 + 冒烟。
#
# 用法：
#   ops/backup/mariadb.sh --host <h> --user <u> --db <db> --backup-dir <dir> \
#     [--encryption-key <keyfile>] [--keep-days 14]
set -euo pipefail

HOST=""
USER=""
DB=""
BACKUP_DIR=""
KEY_FILE=""
KEEP_DAYS=14

usage() { echo "用法见脚本头部注释" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --host) HOST="$2"; shift 2 ;;
    --user) USER="$2"; shift 2 ;;
    --db) DB="$2"; shift 2 ;;
    --backup-dir) BACKUP_DIR="$2"; shift 2 ;;
    --encryption-key) KEY_FILE="$2"; shift 2 ;;
    --keep-days) KEEP_DAYS="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done
[[ -z "$HOST" || -z "$USER" || -z "$DB" || -z "$BACKUP_DIR" ]] && usage

LABEL="$(date -u +%Y%m%dT%H%M%SZ)"
DEST_DIR="$BACKUP_DIR/mariadb/$DB/$LABEL"
mkdir -p "$DEST_DIR"
DUMP="$DEST_DIR/$DB.sql.gz"

echo "==> mariadb-dump（--single-transaction 一致性快照）"
if [[ -z "${MYSQL_PWD:-}" ]]; then
  echo -n "MariaDB password: " >&2
  read -rs MYSQL_PWD
  echo >&2
  export MYSQL_PWD
fi
mariadb-dump --single-transaction --host "$HOST" --user "$USER" "$DB" | gzip > "$DUMP"

echo "==> 加密（AES-256-CBC，密钥文件 0600）"
if [[ -n "$KEY_FILE" ]]; then
  openssl enc -aes-256-cbc -salt -pbkdf2 \
    -pass file:"$KEY_FILE" -in "$DUMP" -out "$DUMP.enc"
  rm -f "$DUMP"
  shasum -a 256 "$DUMP.enc" > "$DUMP.enc.sha256"
  chmod 0400 "$DUMP.enc" "$DUMP.enc.sha256"
  echo "  加密完成: $DUMP.enc"
else
  echo "  [warn] 未指定 --encryption-key，备份为明文 gzip（生产必须加密）"
  shasum -a 256 "$DUMP" > "$DUMP.sha256"
fi

echo "==> 完整性（备份后解压冒烟）"
if [[ -n "$KEY_FILE" ]]; then
  openssl enc -d -aes-256-cbc -pbkdf2 -pass file:"$KEY_FILE" -in "$DUMP.enc" | \
    gunzip -t && echo "  [ok] 解密+解压冒烟通过"
else
  gunzip -t "$DUMP" && echo "  [ok] 解压冒烟通过"
fi

find "$BACKUP_DIR/mariadb/$DB" -type f -mtime "+$KEEP_DAYS" -delete 2>/dev/null || true
echo "==> 完成: $DEST_DIR"
