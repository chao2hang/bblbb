#!/usr/bin/env bash
# BBLBB 小程序联调后端启动脚本
#
# 用独立端口与独立 SQLite 库启动一个后端实例，避免影响仓库既有的
# 开发/测试服务（如 8080 主开发实例）。
#
# 关键配置说明：
# - BBLBB__BIND_ADDRESS：默认 127.0.0.1:18181（避开已占用的 8080/8081/8082）；
# - BBLBB__ALLOWED_ORIGINS：必须包含 https://servicewechat.com ——
#   微信 wx.request 会自动携带
#   `Referer: https://servicewechat.com/{appid}/{page}/{version}`，
#   后端 CSRF 来源校验（M02-SESSION-09）据此放行小程序写请求；
# - BBLBB__DATABASE_URL：独立 SQLite 库（首次自动应用迁移）。
#
# 用法：
#   bash miniprogram/dev/start-backend.sh [port]
#
set -euo pipefail

PORT="${1:-18181}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${BBLBB_BACKEND_BIN:-$ROOT/../target/../bblbb-backend}"

# 共享 cargo target 目录（backend/.cargo/config.toml）
BIN_DIR="/data/cargo-target/bblbb/debug"
if [ -x "$BIN_DIR/bblbb-backend" ]; then
  BIN="$BIN_DIR/bblbb-backend"
elif [ -x "$ROOT/backend/target/debug/bblbb-backend" ]; then
  BIN="$ROOT/backend/target/debug/bblbb-backend"
else
  echo "未找到 bblbb-backend 二进制。"
  echo "请先构建：cd $ROOT/backend && cargo build"
  exit 1
fi

DB="/tmp/bblbb-miniprogram.sqlite"

# 首次：应用 SQLite 迁移（幂等：已存在则复用）
if [ ! -f "$DB" ]; then
  echo ">>> 初始化数据库 $DB"
  for f in "$ROOT"/migrations/sqlite/*.sql; do
    sqlite3 -bail "$DB" < "$f" || { echo "迁移失败: $f"; exit 1; }
  done
fi

cd "$ROOT/backend"
echo ">>> 启动后端: http://127.0.0.1:$PORT"
echo ">>> 数据库: $DB"
echo ">>> ALLOWED_ORIGINS: https://servicewechat.com（小程序 Referer 来源）"
exec env \
  BBLBB__DATABASE_URL="sqlite:///$DB" \
  BBLBB__BIND_ADDRESS="127.0.0.1:$PORT" \
  BBLBB__LOG_FILTER="${BBLBB__LOG_FILTER:-warn}" \
  BBLBB__ALLOWED_ORIGINS="https://servicewechat.com" \
  "$BIN"
