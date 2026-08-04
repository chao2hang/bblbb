#!/usr/bin/env bash
#
# watch-todo.sh — 监听 todo/ 目录变更，每 30 分钟检查一次
#
# 用法：
#   ./scripts/watch-todo.sh              # 前台运行
#   ./scripts/watch-todo.sh --daemon     # 后台守护运行
#
# 检测到变更时输出差异摘要，并写入 scripts/todo-watch.log

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TODO_DIR="$PROJECT_ROOT/todo"
STATE_FILE="$PROJECT_ROOT/scripts/.todo-snapshot.md5"
LOG_FILE="$PROJECT_ROOT/scripts/todo-watch.log"
INTERVAL_SECONDS=1800  # 30 分钟

log() {
  local timestamp
  timestamp="$(date '+%Y-%m-%d %H:%M:%S')"
  echo "[$timestamp] $*" | tee -a "$LOG_FILE"
}

take_snapshot() {
  if [[ ! -d "$TODO_DIR" ]]; then
    log "ERROR: todo/ directory not found at $TODO_DIR"
    return 1
  fi
  find "$TODO_DIR" -type f -name '*.md' -o -type f -name '*.json' | sort | \
    while read -r f; do
      md5sum "$f"
    done > "$STATE_FILE.tmp"
  mv "$STATE_FILE.tmp" "$STATE_FILE"
}

check_changes() {
  local current_snapshot
  current_snapshot="$(find "$TODO_DIR" -type f -name '*.md' -o -type f -name '*.json' | sort | \
    while read -r f; do
      md5sum "$f"
    done)"

  if [[ ! -f "$STATE_FILE" ]]; then
    echo "$current_snapshot" > "$STATE_FILE"
    log "INIT: Initial snapshot created."
    return 0
  fi

  local old_snapshot
  old_snapshot="$(cat "$STATE_FILE")"

  if [[ "$current_snapshot" == "$old_snapshot" ]]; then
    return 0
  fi

  # 计算差异
  local added removed modified
  added="$(diff <(echo "$old_snapshot") <(echo "$current_snapshot") | grep '^>' || true)"
  removed="$(diff <(echo "$old_snapshot") <(echo "$current_snapshot") | grep '^<' || true)"

  log "CHANGE DETECTED: todo/ directory has been modified"

  if [[ -n "$removed" ]]; then
    log "  Removed/Modified files:"
    echo "$removed" | sed 's/^</    /' | tee -a "$LOG_FILE"
  fi

  if [[ -n "$added" ]]; then
    log "  Added/Modified files:"
    echo "$added" | sed 's/^>/    /' | tee -a "$LOG_FILE"
  fi

  # 更新快照
  echo "$current_snapshot" > "$STATE_FILE"

  # 输出当前 todo 文件列表和行数
  log "  Current todo files:"
  for f in "$TODO_DIR"/*.md "$TODO_DIR"/*.json; do
    [[ -f "$f" ]] || continue
    local lines
    lines="$(wc -l < "$f")"
    log "    $(basename "$f") — ${lines} lines"
  done

  return 1  # 返回 1 表示有变更
}

main() {
  log "=== BBLBB todo/ watcher started ==="
  log "Watching: $TODO_DIR"
  log "Interval: ${INTERVAL_SECONDS}s (30 min)"

  # 初始快照
  take_snapshot
  log "Initial snapshot created."

  while true; do
    sleep "$INTERVAL_SECONDS"
    log "--- Periodic check ---"
    if ! check_changes; then
      log "Changes detected — re-plan may be needed."
    else
      log "No changes detected."
    fi
  done
}

# 支持 --daemon 后台模式
if [[ "${1:-}" == "--daemon" ]]; then
  nohup "$0" > /dev/null 2>&1 &
  echo "Watcher started in background (PID: $!)"
  echo "Log: $LOG_FILE"
else
  main
fi
