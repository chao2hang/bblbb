#!/usr/bin/env bash
# 本地联调一键脚本：
#   1. 构建后端（debug，增量）
#   2. 启动小程序联调后端（127.0.0.1:18181，独立 SQLite）
#   3. 启动 job worker（搜索索引/成就等后台任务）
#
# 用法：bash miniprogram/dev/start-all.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN_DIR="/data/cargo-target/bblbb/debug"

echo ">>> [1/3] 构建后端（debug 增量）"
cd "$ROOT/backend" && cargo build 2>&1 | tail -n 3

echo ">>> [2/3] 启动联调后端（后台，端口 18181）"
nohup bash "$ROOT/miniprogram/dev/start-backend.sh" 18181 \
  > /tmp/bblbb-mp-backend.log 2>&1 &
echo $! > /tmp/bblbb-mp-backend.pid
sleep 2
if ! curl -sf http://127.0.0.1:18181/healthz > /dev/null; then
  echo "后端启动失败，日志："
  tail -n 20 /tmp/bblbb-mp-backend.log
  exit 1
fi
echo "    后端 OK（PID $(cat /tmp/bblbb-mp-backend.pid)）"

echo ">>> [3/3] 启动 job worker（后台）"
DB="/tmp/bblbb-miniprogram.sqlite"
nohup env BBLBB__DATABASE_URL="sqlite:///$DB" BBLBB__LOG_FILTER=warn \
  "$BIN_DIR/bblbb-backend" --worker \
  > /tmp/bblbb-mp-worker.log 2>&1 &
echo $! > /tmp/bblbb-mp-worker.pid
sleep 1

cat <<EOF

联调环境就绪：
  后端   http://127.0.0.1:18181  （独立 SQLite：$DB）
  日志   /tmp/bblbb-mp-backend.log  /tmp/bblbb-mp-worker.log

在微信开发者工具中：
  1. 导入 miniprogram/ 目录；
  2. 详情 → 本地设置 → 勾选「不校验合法域名、web-view（业务域名）、TLS 版本以及 HTTPS 证书」；
  3. miniprogram/config.js 的 API_BASE 保持 http://127.0.0.1:18181
     （若本脚本使用其他端口请同步修改）。

冒烟测试（可选，验证后端契约）：
  BBLBB_MP_DB=/tmp/bblbb-miniprogram.sqlite node miniprogram/dev/smoke-test.js

停止：
  kill \$(cat /tmp/bblbb-mp-backend.pid) \$(cat /tmp/bblbb-mp-worker.pid)
EOF
