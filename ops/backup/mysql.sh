#!/usr/bin/env bash
# mysql.sh — MySQL 8 一致备份（M15-BACKUP-02）。
#
# 真实 MySQL 服务器的演练依赖外部基础设施（M15-BACKUP-02 [!] 阻塞项）；
# 本脚本就绪供生产使用：
#   - 一致性 dump（--single-transaction --set-gtid-purged）→ gzip → AES-256
#     加密（openssl enc）→ sha256；
#   - 完整性与保留策略：恢复演练（每周）、保留 N 天。
#
# 用法：
#   ops/backup/mysql.sh --host <h> --user <u> --db <db> --backup-dir <dir> \
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
DEST_DIR="$BACKUP_DIR/mysql/$DB/$LABEL"
mkdir -p "$DEST_DIR"
DUMP="$DEST_DIR/$DB.sql.gz"
ENC="$DUMP.enc"

echo "==> mysqldump（--single-transaction 一致性快照）"
# 密码从环境 MYSQL_PWD 或交互提示；禁止命令行明文
if [[ -z "${MYSQL_PWD:-}" ]]; then
  echo -n "MySQL password: " >&2
  read -rs MYSQL_PWD
  echo >&2
  export MYSQL_PWD
fi
mysqldump --single-transaction --set-gtid-purged=OFF \
  --host "$HOST" --user "$USER" "$DB" | gzip > "$DUMP"

echo "==> 加密（AES-256-CBC，密钥文件 0600）"
if [[ -n "$KEY_FILE" ]]; then
  openssl enc -aes-256-cbc -salt -pbkdf2 \
    -pass file:"$KEY_FILE" -in "$DUMP" -out "$ENC"
  rm -f "$DUMP"
  shasum -a 256 "$ENC" > "$ENC.sha256"
  chmod 0400 "$ENC" "$ENC.sha256"
  echo "  加密完成: $ENC"
else
  echo "  [warn] 未指定 --encryption-key，备份为明文 gzip（生产必须加密）"
  shasum -a 256 "$DUMP" > "$DUMP.sha256"
fi

echo "==> 完整性（备份后 gunzip 冒烟）"
if [[ -n "$KEY_FILE" ]]; then
  openssl enc -d -aes-256-cbc -pbkdf2 -pass file:"$KEY_FILE" -in "$ENC" | \
    gunzip -t && echo "  [ok] 解密+解压冒烟通过"
else
  gunzip -t "$DUMP" && echo "  [ok] 解压冒烟通过"
fi

find "$BACKUP_DIR/mysql/$DB" -type f -mtime "+$KEEP_DAYS" -delete 2>/dev/null || true
echo "==> 完成: $DEST_DIR"
