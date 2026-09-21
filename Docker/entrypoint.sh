#!/bin/sh
set -eu
cd /app

UPLOAD_DIR="${BBLBB__STORAGE_DIR:-/app/uploads}"

# 如果以 root 身份运行，自动修复 uploads 目录属主与权限，并切换为 bblbb 运行
if [ "$(id -u)" = "0" ]; then
  mkdir -p "$UPLOAD_DIR"
  chown -R bblbb:bblbb "$UPLOAD_DIR" 2>/dev/null || true
  chmod -R 775 "$UPLOAD_DIR" 2>/dev/null || true
  exec su -s /bin/sh bblbb -c "exec /app/entrypoint.sh \"$@\""
fi

# 非 root（uid 10001 bblbb）运行时的自检与友好诊断提示
if [ "${1:-frontend}" = "backend" ] || [ "${1:-frontend}" = "worker" ]; then
  mkdir -p "$UPLOAD_DIR" 2>/dev/null || true
  PROBE_FILE="$UPLOAD_DIR/.write_probe_$$"
  if ! touch "$PROBE_FILE" 2>/dev/null; then
    echo "==================================================================" >&2
    echo "❌ [ERROR] 存储目录不可写: $UPLOAD_DIR" >&2
    echo "当前容器运行用户: $(id -u):$(id -g) ($(whoami 2>/dev/null || echo bblbb))" >&2
    echo "目录实际权限与属主: $(ls -ld "$UPLOAD_DIR" 2>/dev/null || echo '无法查看')" >&2
    echo "若使用了宿主机挂载卷（例如 ./uploads），请在宿主机执行属主与权限校正：" >&2
    echo "  sudo chown -R 10001:999 uploads && sudo chmod -R 775 uploads" >&2
    echo "==================================================================" >&2
  else
    rm -f "$PROBE_FILE" 2>/dev/null || true
  fi
fi

case "${1:-frontend}" in
  migrate)
    exec /app/bin/bblbb-migrate apply --db-url "${BBLBB__DATABASE_URL}" --migrations-dir "${BBLBB__MIGRATIONS_DIR:-/app/migrations/mariadb}"
    ;;
  backend)
    exec /app/bin/bblbb-backend
    ;;
  worker)
    exec /app/bin/bblbb-backend --worker
    ;;
  frontend)
    cd /app/frontend
    exec node build/index.js
    ;;
  *) exec "$@" ;;
esac
