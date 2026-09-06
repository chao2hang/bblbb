#!/bin/sh
# BBLBB 原型端口看门狗：setsid 脱离会话常驻，serve.mjs 被 SIGTERM/异常退出后 1 秒自动拉起。
# 启动:  setsid nohup sh .serve-watchdog.sh >/dev/null 2>&1 </dev/null &
# 停止:  pkill -f 'serve-watchdog\.sh'; fuser -k 8765/tcp 2>/dev/null
#        （注意：不要用 pkill -f 'serve.mjs' 这类宽泛模式——会误杀命令行里
#          恰好包含该字符串的包装进程，如执行 pkill 的 shell 本身。）
# 日志:  /tmp/bblbb-prototype-serve.log
cd "$(dirname "$0")" || exit 1
export PROTOTYPE_HOST=0.0.0.0
export PROTOTYPE_PORT=8765
while :; do
  node serve.mjs >> /tmp/bblbb-prototype-serve.log 2>&1
  echo "[watchdog] $(date '+%F %T') serve.mjs exited, restarting in 1s" >> /tmp/bblbb-prototype-serve.log
  sleep 1
done
