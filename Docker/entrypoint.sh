#!/bin/sh
set -eu
cd /app
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
