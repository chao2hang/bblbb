#!/usr/bin/env bash
#
# BBLBB 一键启动开发环境（Rust 后端 + SvelteKit 前端）
#
# 用法:
#   bash scripts/dev.sh            # 启动前后端（数据库已存在时保留数据）
#   bash scripts/dev.sh --fresh    # 备份旧库并重建 SQLite 后启动
#
# 端口（可用环境变量覆盖）:
#   后端 http://127.0.0.1:8080   (BBLBB__BIND_ADDRESS)
#   前端 http://127.0.0.1:5173   (PORT)
#
# 前置: cargo / npm / curl；Node 版本见 .nvmrc（22）。
#
# 数据库策略（关键）:
#   - 后端默认库为 sqlite://data/bblbb.sqlite（与 backend/src/config.rs 默认一致）
#   - 库文件不存在 → 以 --migrate 启动，自动建库并应用全部迁移（等价 make migrate）
#   - 库文件已存在 → 不迁移、保留数据（迁移 SQL 非幂等，重复应用会失败）
#   - --fresh          → 备份旧库到 data/bblbb.sqlite.bak.<时间戳> 后重建
#
# 说明:
#   - 前端 dev 服务器把 /api /healthz /readyz 代理到后端 8080
#     （见 frontend/vite.config.ts）
#   - 若 8080 / 5173 已有实例在运行，脚本直接复用，不重复启动
#   - Ctrl-C 退出时自动停止本脚本启动的前后端进程

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BACKEND_DIR="$ROOT_DIR/backend"
FRONTEND_DIR="$ROOT_DIR/frontend"

# 后端数据库路径（默认与后端配置一致：相对 backend/ 目录的 ../data/bblbb.sqlite）
DB_FILE="${BBLBB_DB:-$ROOT_DIR/data/bblbb.sqlite}"

BACKEND_BIND="${BBLBB__BIND_ADDRESS:-127.0.0.1:8080}"
BACKEND_URL="http://$BACKEND_BIND"
HEALTH_URL="$BACKEND_URL/healthz"
FRONTEND_HOST="127.0.0.1"
FRONTEND_PORT="${PORT:-5173}"
FRONTEND_URL="http://$FRONTEND_HOST:$FRONTEND_PORT"

BACKEND_PID=""
FRONTEND_PID=""
FRESH=0
[[ "${1:-}" == "--fresh" ]] && FRESH=1

info()  { printf "\033[32m>>>\033[0m %s\n" "$*"; }
warn()  { printf "\033[33m!!!\033[0m %s\n" "$*" >&2; }
error() { printf "\033[31m!!!\033[0m %s\n" "$*" >&2; }

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    error "未找到命令：$1（$2）"
    exit 1
  fi
}

backend_alive()  { curl -fsS --max-time 2 "$HEALTH_URL" >/dev/null 2>&1; }
frontend_alive() { curl -fsS --max-time 2 "$FRONTEND_URL" >/dev/null 2>&1; }

cleanup() {
  local code=$?
  trap - EXIT INT TERM
  if [[ -n "$BACKEND_PID" ]] && kill -0 "$BACKEND_PID" 2>/dev/null; then
    info "停止后端 (PID $BACKEND_PID)"
    kill "$BACKEND_PID" 2>/dev/null || true
    wait "$BACKEND_PID" 2>/dev/null || true
  fi
  if [[ -n "$FRONTEND_PID" ]] && kill -0 "$FRONTEND_PID" 2>/dev/null; then
    info "停止前端 (PID $FRONTEND_PID)"
    kill "$FRONTEND_PID" 2>/dev/null || true
    wait "$FRONTEND_PID" 2>/dev/null || true
  fi
  exit "$code"
}
trap cleanup EXIT INT TERM

# ── 前置检查 ────────────────────────────────────────────────────
require_cmd cargo "请安装 Rust 工具链：https://rustup.rs"
require_cmd npm  "请安装 Node.js 22+（建议 nvm/fnm/mise，版本见 .nvmrc）"
require_cmd curl "curl 为 macOS 系统自带"

# ── 前端依赖 ────────────────────────────────────────────────────
install_frontend_deps() {
  info "安装前端依赖 (npm ci) ..."
  (cd "$FRONTEND_DIR" && npm ci)
}

if [[ ! -d "$FRONTEND_DIR/node_modules" ]]; then
  install_frontend_deps
elif [[ "$FRONTEND_DIR/package-lock.json" -nt "$FRONTEND_DIR/node_modules/.package-lock.json" ]]; then
  warn "package-lock.json 比已安装依赖新，重新安装 ..."
  install_frontend_deps
fi

# ── 数据库：迁移策略 ────────────────────────────────────────────
MIGRATE_FLAG=""
if [[ "$FRESH" == "1" ]]; then
  if backend_alive; then
    error "--fresh 会重建数据库，但检测到后端正在 $BACKEND_URL 运行（旧进程仍持有旧库句柄，重建不会生效）。"
    error "请先停止该后端进程，再重新运行：bash scripts/dev.sh --fresh"
    exit 1
  fi
  if [[ -f "$DB_FILE" ]]; then
    local_backup="$DB_FILE.bak.$(date +%Y%m%d%H%M%S)"
    cp "$DB_FILE" "$local_backup"
    info "已备份旧数据库 → $local_backup"
  fi
  rm -f "$DB_FILE" "$DB_FILE-shm" "$DB_FILE-wal"
  MIGRATE_FLAG="--migrate"
elif [[ ! -f "$DB_FILE" ]]; then
  info "未发现数据库 ${DB_FILE}，将自动创建并应用全部迁移"
  MIGRATE_FLAG="--migrate"
else
  warn "数据库 $DB_FILE 已存在，跳过迁移（保留现有数据）。"
  warn "如需按最新迁移重建：bash scripts/dev.sh --fresh（会自动备份旧库）"
fi

# ── 启动后端 ────────────────────────────────────────────────────
if backend_alive; then
  warn "检测到后端已在 $BACKEND_URL 运行，直接复用（不重复启动）。"
else
  info "启动后端：${BACKEND_URL}（迁移: ${MIGRATE_FLAG:-关闭}）"
  (
    cd "$BACKEND_DIR"
    export BBLBB__ENV=development
    if [[ -n "$MIGRATE_FLAG" ]]; then
      export BBLBB__AUTO_MIGRATE=true
      exec cargo run --bin bblbb-backend -- --migrate
    else
      exec cargo run --bin bblbb-backend
    fi
  ) &
  BACKEND_PID=$!

  # 等待后端就绪（首次运行需编译 Rust 依赖，耗时较长）
  waited=0
  while (( waited < 180 )); do
    if backend_alive; then
      info "后端就绪：$HEALTH_URL"
      break
    fi
    if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
      error "后端进程已退出，请检查上方日志。"
      exit 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  if (( waited >= 180 )); then
    error "后端 180 秒内未就绪，请检查上方日志。"
    exit 1
  fi
fi

# ── 启动前端 ────────────────────────────────────────────────────
if frontend_alive; then
  warn "检测到前端已在 $FRONTEND_URL 运行，直接复用（不重复启动）。"
else
  info "启动前端：${FRONTEND_URL}（--strictPort，端口被占用会报错）"
  (
    cd "$FRONTEND_DIR"
    exec npm run dev -- --host "$FRONTEND_HOST" --port "$FRONTEND_PORT" --strictPort
  ) &
  FRONTEND_PID=$!

  # 等待前端就绪
  waited=0
  while (( waited < 60 )); do
    if frontend_alive; then
      break
    fi
    if ! kill -0 "$FRONTEND_PID" 2>/dev/null; then
      error "前端进程已退出，请检查上方日志。若端口 $FRONTEND_PORT 被占用，可用 PORT=xxxx bash scripts/dev.sh 更换。"
      exit 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  if (( waited >= 60 )); then
    error "前端 60 秒内未就绪，请检查上方日志。"
    exit 1
  fi
fi

printf "\n\033[1m  BBLBB 开发环境已就绪\033[0m\n"
printf "    前端  %s\n" "$FRONTEND_URL"
printf "    后端  %s   (健康检查 %s)\n" "$BACKEND_URL" "$HEALTH_URL"
printf "    按 Ctrl-C 同时停止前后端\n\n"

# 前台等待前端进程；退出后由 EXIT trap 清理后端
wait "$FRONTEND_PID"
