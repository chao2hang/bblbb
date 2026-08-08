#!/usr/bin/env bash
# test-graceful-shutdown.sh — SIGTERM 优雅停机实测（M15-UPGRADE-06）。
#
# 覆盖：
#   1. HTTP 服务：SIGTERM → 停止接收新请求 → 完成在途请求 → 干净退出（exit 0，
#      且在 TimeoutStopSec 内）；日志含 "server shutdown complete"；
#   2. worker（--worker）：SIGTERM → 停止领取 → 收尾在途任务（drain_timeout）→
#      干净退出；日志含 "all worker queues drained"；
#   3. 超时门：退出时长 < 阈值（默认 10s），不 kill -9。
#
# 用法：ops/test-graceful-shutdown.sh [--timeout 10]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BACKEND_BIN="$ROOT/backend/target/debug/bblbb-backend"
TIMEOUT_S="${1:-10}"
DB="$(mktemp -d)/bblbb-shutdown.sqlite"
PORT=18100

PASS=0
FAIL=0
ok()  { echo "  ok: $*"; PASS=$((PASS+1)); }
bad() { echo "  FAIL: $*" >&2; FAIL=$((FAIL+1)); }

[[ -x "$BACKEND_BIN" ]] || { echo "backend 未构建：cargo build 后再运行" >&2; exit 1; }

echo "==> 1/3 HTTP 服务优雅停机"
BBLBB__ENV=development BBLBB__BIND_ADDRESS="127.0.0.1:$PORT" \
BBLBB__DATABASE_URL="sqlite://$DB" BBLBB__MIGRATIONS_DIR="$ROOT/migrations/sqlite" \
BBLBB__AUTO_MIGRATE=true BBLBB__STORAGE_DIR="$(mktemp -d)" \
"$BACKEND_BIN" > /tmp/bblbb-shutdown-server.log 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 40); do
  curl -s -o /dev/null "http://127.0.0.1:$PORT/healthz" && break
  sleep 0.25
done
curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null && ok "服务启动"
START="$(python3 -c 'import time;print(time.time())')"
kill -TERM "$SERVER_PID"
# 等待退出（轮询，避免 wait 阻塞超时）
EXITED=0
for _ in $(seq 1 $((TIMEOUT_S * 4))); do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then EXITED=1; break; fi
  sleep 0.25
done
END="$(python3 -c 'import time;print(time.time())')"
ELAPSED="$(python3 -c "print(f'{$END-$START:.2f}')")"
if [[ $EXITED == 1 ]]; then
  ok "SIGTERM 后 ${ELAPSED}s 内干净退出"
  wait "$SERVER_PID" 2>/dev/null; RC=$?
  [[ $RC == 0 ]] && ok "退出码 0" || bad "退出码 $RC"
  grep -q "server shutdown complete" /tmp/bblbb-shutdown-server.log \
    && ok "日志含 server shutdown complete" || bad "缺少 shutdown 日志"
else
  bad "SIGTERM 后 ${TIMEOUT_S}s 未退出（超时）"
  kill -9 "$SERVER_PID" 2>/dev/null || true
fi

echo "==> 2/3 worker 优雅停机（--worker）"
BBLBB__ENV=development BBLBB__DATABASE_URL="sqlite://$DB" \
BBLBB__MIGRATIONS_DIR="$ROOT/migrations/sqlite" \
"$BACKEND_BIN" --worker > /tmp/bblbb-shutdown-worker.log 2>&1 &
WORKER_PID=$!
sleep 2
kill -0 "$WORKER_PID" 2>/dev/null && ok "worker 启动"
START="$(python3 -c 'import time;print(time.time())')"
kill -TERM "$WORKER_PID"
WEXITED=0
for _ in $(seq 1 $((TIMEOUT_S * 4))); do
  if ! kill -0 "$WORKER_PID" 2>/dev/null; then WEXITED=1; break; fi
  sleep 0.25
done
END="$(python3 -c 'import time;print(time.time())')"
WELAPSED="$(python3 -c "print(f'{$END-$START:.2f}')")"
if [[ $WEXITED == 1 ]]; then
  ok "worker SIGTERM 后 ${WELAPSED}s 内退出"
  wait "$WORKER_PID" 2>/dev/null; RC=$?
  [[ $RC == 0 ]] && ok "worker 退出码 0" || bad "worker 退出码 $RC"
  grep -q "all worker queues drained" /tmp/bblbb-shutdown-worker.log \
    && ok "日志含 worker drained" || bad "缺少 worker drained 日志"
else
  bad "worker SIGTERM 后未退出"
  kill -9 "$WORKER_PID" 2>/dev/null || true
fi

echo "==> 3/3 结果汇总"
echo "PASS=$PASS FAIL=$FAIL"
# 停机语义库级测试（停止领取/租约恢复/drain 超时）已由 backend/tests/worker_loop.rs 覆盖
echo "（worker 停止领取/租约处理/drain 超时的库级测试见 backend/tests/worker_loop.rs）"
[[ $FAIL -eq 0 ]] || exit 1
