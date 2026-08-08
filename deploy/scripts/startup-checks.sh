#!/usr/bin/env bash
# startup-checks.sh — 生产启动检查（M15-PACKAGE-06）。
#
# 在切换到 new current 之前执行（release.sh 调用）；任何一项失败即中止发布。
# 检查项：origin / Cookie 配置 / 数据库可达 / 目录权限 / 迁移状态 /
# OIDC 密钥 / 外部配置。
#
# 用法：deploy/scripts/startup-checks.sh [--env-file /etc/bblbb/backend.env]
set -euo pipefail

ENV_FILE="${1:-/etc/bblbb/backend.env}"
DB_URL=""
MIGRATIONS_DIR=""
STORAGE_DIR=""
PUBLIC_ORIGIN=""
FAILED=0

note()  { echo "[startup-checks] $*"; }
fail()  { echo "[startup-checks] FAIL: $*" >&2; FAILED=1; }

# 读取 backend.env（BBLBB__ 前缀环境变量）
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  set -a; source "$ENV_FILE"; set +a
fi

DB_URL="${BBLBB__DATABASE_URL:-$DB_URL}"
MIGRATIONS_DIR="${BBLBB__MIGRATIONS_DIR:-$MIGRATIONS_DIR}"
STORAGE_DIR="${BBLBB__STORAGE_DIR:-$STORAGE_DIR}"
PUBLIC_ORIGIN="${BBLBB__PUBLIC_ORIGIN:-$PUBLIC_ORIGIN}"

note "==> 1/7 origin 检查"
if [[ -z "$PUBLIC_ORIGIN" ]]; then
  fail "PUBLIC_ORIGIN 未配置（OIDC discovery / Cookie 依赖固定 HTTPS origin）"
elif [[ "$PUBLIC_ORIGIN" != https://* ]]; then
  fail "PUBLIC_ORIGIN 必须为 HTTPS: $PUBLIC_ORIGIN"
else
  note "origin: $PUBLIC_ORIGIN"
fi

note "==> 2/7 Cookie 配置检查"
if [[ -z "${BBLBB__SESSION_COOKIE_NAME:-}" ]] && [[ -z "${SESSION_COOKIE_NAME:-}" ]]; then
  note "SESSION_COOKIE_NAME 未显式配置（将使用默认 __Host-bblbb_session）"
else
  note "SESSION_COOKIE_NAME 已配置"
fi

note "==> 3/7 数据库可达"
if [[ -z "$DB_URL" ]]; then
  fail "BBLBB__DATABASE_URL 未配置"
else
  case "$DB_URL" in
    sqlite:*)
      DB_PATH="${DB_URL#sqlite:}"
      DB_PATH="${DB_PATH#//}"
      # sqlite:///var/lib/... 或 sqlite:../data/... 归一化
      if [[ "$DB_PATH" == /* ]]; then :; else DB_PATH="../$DB_PATH"; fi
      DB_DIR="$(dirname "$DB_PATH")"
      if [[ ! -d "$DB_DIR" ]]; then
        fail "数据库目录不存在: $DB_DIR"
      else
        note "数据库目录存在: $DB_DIR"
      fi
      if [[ -f "$DB_PATH" ]]; then
        sqlite3 "$DB_PATH" "PRAGMA quick_check;" >/dev/null 2>&1 \
          && note "SQLite quick_check OK" || fail "SQLite quick_check 失败"
      fi
      ;;
    mysql://*|mariadb://*)
      note "MySQL/MariaDB 可达性由 backend 启动 pre-flight 校验（BBLBB_ENV=production 时）"
      ;;
    *) fail "不支持的数据库 scheme: $DB_URL" ;;
  esac
fi

note "==> 4/7 目录权限"
for dir in "$STORAGE_DIR" /var/lib/bblbb/backups; do
  if [[ -n "$dir" && -d "$dir" ]]; then
    note "目录存在: $dir"
  elif [[ -n "$dir" ]]; then
    fail "目录不存在: $dir"
  fi
done

note "==> 5/7 迁移状态（bblbb-migrate --check，只读）"
if [[ -n "$MIGRATIONS_DIR" && -d "$MIGRATIONS_DIR" ]]; then
  MIGRATE_BIN="/opt/bblbb/current/backend/bblbb-migrate"
  if [[ -x "$MIGRATE_BIN" ]]; then
    if BBLBB__MIGRATIONS_DIR="$MIGRATIONS_DIR" BBLBB__DATABASE_URL="$DB_URL" "$MIGRATE_BIN" --check; then
      note "迁移状态一致（或仅有待应用迁移，由发布脚本显式 apply）"
    else
      fail "迁移检查失败：checksum 不匹配或版本超前（禁止发布）"
    fi
  else
    note "bblbb-migrate 不在 new current（发布脚本负责 apply）"
  fi
else
  fail "BBLBB__MIGRATIONS_DIR 未配置或不存在"
fi

note "==> 6/7 OIDC 密钥"
if [[ -z "${BBLBB__OIDC_KEY_ENCRYPTION_KEY:-}" ]]; then
  note "OIDC 默认关闭；BBLBB__OIDC_KEY_ENCRYPTION_KEY 未配置（启用 OIDC 前必须配置并备份分离）"
else
  note "OIDC 密钥加密主密钥已配置（密文在 DB，主密钥与备份分离存储）"
fi

note "==> 7/7 外部配置"
if [[ -z "${BBLBB__FEATURE_KILL_SWITCH:-}" ]]; then
  note "FEATURE_KILL_SWITCH 未设置（默认 false）"
else
  note "FEATURE_KILL_SWITCH=${BBLBB__FEATURE_KILL_SWITCH}"
fi
for var in BBLBB__S3_BUCKET BBLBB__S3_REGION BBLBB__SMTP_HOST; do
  if [[ -n "${!var:-}" ]]; then
    note "$var 已配置"
  fi
done

if [[ $FAILED -eq 0 ]]; then
  note "ALL CHECKS PASSED"
  exit 0
else
  note "CHECKS FAILED"
  exit 1
fi
