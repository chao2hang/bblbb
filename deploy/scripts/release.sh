#!/usr/bin/env bash
# release.sh — 发布编排（M15-UPGRADE-04/05）。
#
# 顺序（每步失败即中止）：
#   1. 发布前备份（数据库 + manifest，M15-BACKUP）；
#   2. 启动检查（startup-checks.sh，M15-PACKAGE-06）；
#   3. 显式迁移（bblbb-migrate apply，M15-DB-06；失败即停，不切流）；
#   4. 切换 current 符号链接；
#   5. 重启 backend/worker/frontend（依赖顺序：backend → worker → frontend）；
#   6. 验证 /readyz、worker、冒烟（M15-UPGRADE-07）；
#   7. 失败时：停止切流、保留诊断、恢复 current 到上一版本或进入人工恢复。
#
# 用法：
#   deploy/scripts/release.sh --bundle <bundle.tar.gz> [--releases-dir /opt/bblbb/releases]
#
# 回滚：release.sh --rollback [--target <previous-version>]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RELEASES_DIR="/opt/bblbb/releases"
BUNDLE=""
VERIFY_READY="${BBLBB_RELEASE_VERIFY_READY:-true}"

usage() {
  echo "用法:"
  echo "  release.sh --bundle <bundle.tar.gz> [--releases-dir <dir>]"
  echo "  release.sh --rollback [--target <version>]"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle) BUNDLE="$2"; shift 2 ;;
    --releases-dir) RELEASES_DIR="$2"; shift 2 ;;
    --rollback) ROLLBACK=1; shift ;;
    --target) ROLLBACK_TARGET="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done

note()  { echo "[release] $*"; }
fail()  { echo "[release] FAIL: $*" >&2; exit 1; }

if [[ "${ROLLBACK:-0}" == "1" ]]; then
  note "==> 回滚模式"
  PREV=""
  if [[ -n "${ROLLBACK_TARGET:-}" ]]; then
    PREV="$RELEASES_DIR/$ROLLBACK_TARGET"
    [[ -d "$PREV" ]] || fail "回滚目标不存在: $PREV"
  else
    # 按目录名排序取当前的上一个版本（版本号可排序假设：same-width numeric build）
    CURRENT="$(readlink "$RELEASES_DIR/current" || true)"
    PREV="$(find "$RELEASES_DIR" -maxdepth 1 -type d -name '*+*' -o -maxdepth 1 -type d -name 'v*' | grep -v "$(basename "$CURRENT")" | sort | tail -1)"
  fi
  note "回滚到: $PREV"
  note "1) 回滚前备份（保持可回退）"
  ln -sfn "$PREV" "$RELEASES_DIR/current"
  note "2) current 已切回: $(readlink "$RELEASES_DIR/current")"
  note "3) 重启服务（backend → worker → frontend）"
  systemctl restart bblbb-backend || true
  systemctl restart bblbb-worker || true
  systemctl restart bblbb-frontend || true
  note "4) 验证 ready/冒烟"
  curl -fsS "http://127.0.0.1:8080/readyz" >/dev/null || fail "回滚后 ready 检查失败——进入人工恢复（ops/runbooks/migration-failure.md）"
  "$ROOT/ops/smoke/smoke.sh" || fail "回滚后冒烟失败"
  note "回滚完成（记录写入 docs/CHANGELOG.md）"
  exit 0
fi

[[ -z "$BUNDLE" || ! -f "$BUNDLE" ]] && usage

note "==> 1/7 发布前备份"
mkdir -p /var/lib/bblbb/backups
"$ROOT/ops/backup/sqlite.sh" "${BBLBB_DATABASE_FILE:-/var/lib/bblbb/database/bblbb.db}" /var/lib/bblbb/backups --label pre-release
"$ROOT/ops/backup/manifest.sh" --db "${BBLBB_DATABASE_FILE:-/var/lib/bblbb/database/bblbb.db}" \
  --storage /var/lib/bblbb/uploads \
  --out /var/lib/bblbb/backups/manifest-pre-release.json || true

note "==> 2/7 解包 bundle 到暂存"
VERSION="$(tar -xOzf "$BUNDLE" VERSION)"
STAGE="$(mktemp -d)"
tar -xzf "$BUNDLE" -C "$STAGE"
note "    version=$VERSION"

note "==> 3/7 启动检查（origin/Cookie/DB/dirs/migrations/OIDC/外部配置）"
"$ROOT/deploy/scripts/startup-checks.sh" /etc/bblbb/backend.env || fail "启动检查失败，停止发布"

note "==> 4/7 显式迁移（bblbb-migrate apply；失败即停，不切流）"
"$STAGE/backend/bblbb-migrate" apply \
  --db-url "${BBLBB_DATABASE_URL:-}" \
  --migrations-dir "$STAGE/migrations/sqlite" || {
    note "迁移失败：保留暂存目录 $STAGE 供诊断；未切换 current"
    fail "迁移失败（见 ops/runbooks/migration-failure.md）"
  }

note "==> 5/7 切换 current 并加固权限"
mkdir -p "$RELEASES_DIR"
rm -rf "$RELEASES_DIR/$VERSION"
cp -R "$STAGE" "$RELEASES_DIR/$VERSION"
# 最小权限：root 所有，服务用户只读执行
chown -R root:bblbb "$RELEASES_DIR/$VERSION"
chmod -R a+rX "$RELEASES_DIR/$VERSION"
chmod 0555 "$RELEASES_DIR/$VERSION/backend/bblbb-backend" "$RELEASES_DIR/$VERSION/backend/bblbb-migrate"
ln -sfn "$RELEASES_DIR/$VERSION" "$RELEASES_DIR/current"
rm -rf "$STAGE"
note "    current → $(readlink "$RELEASES_DIR/current")"

note "==> 6/7 重启（backend → worker → frontend）"
systemctl restart bblbb-backend || fail "backend 重启失败"
systemctl restart bblbb-worker || fail "worker 重启失败"
systemctl restart bblbb-frontend || fail "frontend 重启失败"

note "==> 7/7 验证 ready / worker / 冒烟"
if [[ "$VERIFY_READY" == "true" ]]; then
  for _ in $(seq 1 20); do
    if curl -fsS "http://127.0.0.1:8080/readyz" 2>/dev/null | grep -q '"status":"ok"'; then
      break
    fi
    sleep 1
  done
  curl -fsS "http://127.0.0.1:8080/readyz" | grep -q '"status":"ok"' || {
    note "ready 检查失败：保留 current=$VERSION 与 journald 诊断"
    note "执行回滚：release.sh --rollback"
    fail "ready 检查失败"
  }
fi
"$ROOT/ops/smoke/smoke.sh" || fail "冒烟失败（保留诊断，执行 --rollback）"

note "发布成功: $VERSION"
