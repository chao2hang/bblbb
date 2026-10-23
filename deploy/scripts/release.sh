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
#   BBLBB_RELEASE_EXPECTED_SHA256=<trusted-out-of-band-sha256> deploy/scripts/release.sh --bundle <bundle.tar.gz>
# A publisher-trusted digest is mandatory; the bundle's own SHA256SUMS is not authentication.
#
# 回滚：release.sh --rollback --target <previous-version>（必须显式指定）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=release-service-state.sh
source "$(dirname "${BASH_SOURCE[0]}")/release-service-state.sh"
RELEASES_DIR="${BBLBB_RELEASES_DIR:-/opt/bblbb/releases}"
CURRENT_LINK="${BBLBB_CURRENT_LINK:-}"
ENV_FILE="${BBLBB_ENV_FILE:-/etc/bblbb/backend.env}"
BUNDLE=""
VERIFY_READY="${BBLBB_RELEASE_VERIFY_READY:-true}"

usage() {
  echo "用法:"
  echo "  release.sh --bundle <bundle.tar.gz> [--releases-dir <dir>]"
  echo "  release.sh --rollback --target <version>"
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

if [[ -z "$CURRENT_LINK" ]]; then
  CURRENT_LINK="$(dirname "$RELEASES_DIR")/current"
fi

note()  { echo "[release] $*"; }
fail()  { echo "[release] FAIL: $*" >&2; exit 1; }

if [[ "${ROLLBACK:-0}" == "1" ]]; then
  release_acquire_lock "$RELEASES_DIR" 0 || fail "无法获取发布/回滚锁"
  note "==> 回滚模式"
  PREV=""
  if [[ -n "${ROLLBACK_TARGET:-}" ]]; then
    [[ "$ROLLBACK_TARGET" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$ && "$ROLLBACK_TARGET" != *..* ]] || fail "回滚目标版本名无效"
    PREV="$RELEASES_DIR/$ROLLBACK_TARGET"
    [[ -d "$PREV" && ! -L "$PREV" ]] || fail "回滚目标不存在或不是普通目录: $PREV"
    RELEASES_REAL="$(realpath -e "$RELEASES_DIR")"
    PREV_REAL="$(realpath -e "$PREV")"
    [[ "$(dirname "$PREV_REAL")" == "$RELEASES_REAL" ]] || fail "回滚目标路径越界"
  else
    fail "必须显式指定 --target <version>；不按目录字典序猜测回滚版本"
  fi
  note "回滚到: $PREV"
  note "注意：此命令仅回切代码链接，不执行数据库备份或迁移回滚；操作者须先确认 schema 向后兼容及现有备份有效"
  release_capture_service_state allow-nonactive || fail "无法确认当前服务状态，拒绝回滚"
  CURRENT_DIR="$(dirname "$CURRENT_LINK")"
  [[ -d "$CURRENT_DIR" ]] || fail "current 符号链接父目录不存在: $CURRENT_DIR"
  [[ -L "$CURRENT_LINK" || ! -e "$CURRENT_LINK" ]] || fail "current 路径不是符号链接"
  TMP_CURRENT="$CURRENT_DIR/.rollback-$(basename "$PREV")-$$"
  [[ ! -e "$TMP_CURRENT" && ! -L "$TMP_CURRENT" ]] || fail "回滚临时链接路径已存在"
  cleanup_rollback_link() {
    if [[ -n "${TMP_CURRENT:-}" && -L "$TMP_CURRENT" && "$(readlink "$TMP_CURRENT" 2>/dev/null || true)" == "$PREV" ]]; then rm -f -- "$TMP_CURRENT"; fi
  }
  trap 'status=$?; cleanup_rollback_link; release_handle_exit "$status"' EXIT
  note "进入维护窗口：停止 frontend → worker → backend"
  release_stop_stack || fail "停止服务失败，未修改 current"
  RELEASE_CUTOVER_STARTED=1
  ln -s "$PREV" "$TMP_CURRENT"
  mv -Tf "$TMP_CURRENT" "$CURRENT_LINK"
  note "2) current 已切回: $(readlink "$CURRENT_LINK")"
  note "3) 重启服务（backend → worker → frontend）"
  systemctl start bblbb-backend || fail "backend 启动失败"
  systemctl start bblbb-worker || fail "worker 启动失败"
  systemctl start bblbb-frontend || fail "frontend 启动失败"
  note "4) 验证 ready/冒烟"
  curl -fsS "http://127.0.0.1:8080/readyz" | grep -q '"status":"ok"' || fail "回滚后 ready 检查失败——进入人工恢复（ops/runbooks/migration-failure.md）"
  "$ROOT/ops/smoke/smoke.sh" || fail "回滚后冒烟失败"
  note "回滚完成（记录写入 docs/CHANGELOG.md）"
  exit 0
fi

[[ -z "$BUNDLE" || ! -f "$BUNDLE" || -L "$BUNDLE" ]] && usage
[[ "${BBLBB_RELEASE_EXPECTED_SHA256:-}" =~ ^[A-Fa-f0-9]{64}$ ]] || fail "必须提供带外可信 BBLBB_RELEASE_EXPECTED_SHA256"
release_acquire_lock "$RELEASES_DIR" 1 || fail "无法获取发布/回滚锁"
BUNDLE_DIR="$(mktemp -d /tmp/bblbb-release-bundle.XXXXXX)" || fail "无法创建私有 bundle 暂存目录"
BUNDLE_SNAPSHOT="$BUNDLE_DIR/bundle.tar.gz"
cleanup_bundle_snapshot() {
  if [[ -n "${BUNDLE_DIR:-}" && "$BUNDLE_DIR" == /tmp/bblbb-release-bundle.* && -d "$BUNDLE_DIR" && ! -L "$BUNDLE_DIR" ]]; then
    if [[ -f "$BUNDLE_SNAPSHOT" && ! -L "$BUNDLE_SNAPSHOT" && "$(dirname "$BUNDLE_SNAPSHOT")" == "$BUNDLE_DIR" ]]; then rm -f -- "$BUNDLE_SNAPSHOT"; fi
    rmdir -- "$BUNDLE_DIR" 2>/dev/null || true
  fi
}
trap cleanup_bundle_snapshot EXIT
python3 - "$BUNDLE" "$BUNDLE_SNAPSHOT" <<'PY' || fail "无法建立 bundle 私有快照"
import os, stat, sys
source, target = sys.argv[1:]
MAX_BUNDLE_BYTES = 2 * 1024 * 1024 * 1024
source_fd = os.open(source, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
try:
    source_stat = os.fstat(source_fd)
    if not stat.S_ISREG(source_stat.st_mode):
        raise SystemExit("bundle source is not a regular file")
    if source_stat.st_size <= 0 or source_stat.st_size > MAX_BUNDLE_BYTES:
        raise SystemExit("bundle size is empty or exceeds 2 GiB limit")
    target_fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(source_fd, "rb", closefd=False) as incoming:
        with os.fdopen(target_fd, "wb") as outgoing:
            copied = 0
            while True:
                block = incoming.read(1024 * 1024)
                if not block: break
                copied += len(block)
                if copied > MAX_BUNDLE_BYTES:
                    raise SystemExit("bundle grew beyond 2 GiB copy limit")
                outgoing.write(block)
            outgoing.flush()
            os.fsync(outgoing.fileno())
finally:
    os.close(source_fd)
PY
chmod 0600 "$BUNDLE_SNAPSHOT"
BUNDLE="$BUNDLE_SNAPSHOT"
[[ "${BBLBB_RELEASE_EXPECTED_SHA256:-}" =~ ^[A-Fa-f0-9]{64}$ ]] || fail "必须提供带外可信 BBLBB_RELEASE_EXPECTED_SHA256"
if command -v sha256sum >/dev/null 2>&1; then
  BUNDLE_SHA256="$(sha256sum "$BUNDLE" | awk '{print $1}')"
else
  BUNDLE_SHA256="$(shasum -a 256 "$BUNDLE" | awk '{print $1}')"
fi
[[ "${BUNDLE_SHA256,,}" == "${BBLBB_RELEASE_EXPECTED_SHA256,,}" ]] || fail "bundle 与带外可信 SHA-256 不匹配"

# 发布编排目前的备份/迁移实装只支持 SQLite。失败时不得继续到数据库写入。
if [[ -f "$ENV_FILE" && ! -L "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  set -a; source "$ENV_FILE"; set +a
fi
DB_URL="${BBLBB__DATABASE_URL:-}"
[[ "$DB_URL" == sqlite:///* ]] || fail "当前发布流程仅支持配置绝对路径 SQLite DSN（BBLBB__DATABASE_URL）"
DB_FILE="${DB_URL#sqlite://}"
[[ -n "$DB_FILE" && "$DB_FILE" == /* ]] || fail "SQLite 数据库路径必须是绝对路径"
STORAGE_BACKEND="${BBLBB__STORAGE_BACKEND:-local}"
[[ "$STORAGE_BACKEND" == local ]] || fail "发布前完整备份尚不支持 storage backend=$STORAGE_BACKEND；拒绝 S3/非 local 发布"
STORAGE_DIR="${BBLBB__STORAGE_DIR:-/var/lib/bblbb/uploads}"

note "==> 1/7 预检 bundle（不触碰数据库）"
# Inspect the untrusted archive before extracting any member. Python emits VERSION only
# after checking every archive path and rejecting links/special files/duplicates.
VERSION="$(python3 - "$BUNDLE" <<'PY'
import gzip, pathlib, sys, tarfile
MAX_MEMBERS = 250_000
MAX_UNPACKED_BYTES = 20 * 1024 * 1024 * 1024
MAX_STREAM_BYTES = MAX_UNPACKED_BYTES + MAX_MEMBERS * 1024
class BoundedReader:
    def __init__(self, stream, limit):
        self.stream, self.limit, self.read_bytes = stream, limit, 0
    def read(self, size=-1):
        remaining = self.limit - self.read_bytes
        if size is None or size < 0 or size > remaining + 1:
            size = remaining + 1
        block = self.stream.read(size)
        self.read_bytes += len(block)
        if self.read_bytes > self.limit:
            raise SystemExit("bundle exceeds decompressed-stream safety limit")
        return block
with open(sys.argv[1], "rb") as compressed:
    decompressed = gzip.GzipFile(fileobj=compressed, mode="rb")
    bounded = BoundedReader(decompressed, MAX_STREAM_BYTES)
    archive = tarfile.open(fileobj=bounded, mode="r|")
    seen = set()
    version = None
    total_size = 0
    count = 0
    for member in archive:
        count += 1
        if count > MAX_MEMBERS:
            raise SystemExit("bundle has too many archive members")
        name = pathlib.PurePosixPath(member.name)
        normalized = str(name)
        if name.is_absolute() or ".." in name.parts or "\\" in member.name or normalized in seen:
            raise SystemExit(f"unsafe or duplicate bundle member: {member.name}")
        seen.add(normalized)
        if not (member.isdir() or member.isfile()):
            raise SystemExit(f"unsupported bundle member type: {member.name}")
        if member.isdir() and member.size != 0:
            raise SystemExit(f"bundle directory member has nonzero size: {member.name}")
        if member.isfile():
            total_size += member.size
            if total_size > MAX_UNPACKED_BYTES:
                raise SystemExit("bundle exceeds uncompressed size limit")
        if normalized == "VERSION":
            if not member.isfile() or member.size > 128:
                raise SystemExit("bundle VERSION missing, invalid, or oversized")
            stream = archive.extractfile(member)
            content = stream.read(129).decode("ascii").strip() if stream else ""
            if not content or len(content) > 128:
                raise SystemExit("bundle VERSION missing or oversized")
            version = content
    while bounded.read(1024 * 1024):
        pass
    if version is None:
        raise SystemExit("bundle VERSION missing")
    print(version)
PY
)" || fail "bundle 路径或 VERSION 预检失败"
[[ "$VERSION" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$ ]] || fail "bundle VERSION 格式无效"
[[ "$VERSION" != *..* ]] || fail "bundle VERSION 不安全"
[[ -d "$STORAGE_DIR" && ! -L "$STORAGE_DIR" ]] || fail "配置的本地附件存储目录不存在、不可用或为符号链接: $STORAGE_DIR"
[[ "$RELEASES_DIR" == /* && "$CURRENT_LINK" == /* ]] || fail "release 与 current 路径必须为绝对路径"
[[ ! -e "$RELEASES_DIR/$VERSION" && ! -L "$RELEASES_DIR/$VERSION" ]] || fail "release 版本目录已存在，拒绝覆盖: $VERSION"
STAGE=""
PREPARED_RELEASE=""
TMP_CURRENT=""
CREATED_TMP_CURRENT=0
CREATED_PREPARED_RELEASE=0
KEEP_PREPARED_RELEASE=0
cleanup_release_temps() {
  if [[ "${CREATED_TMP_CURRENT:-0}" == 1 && -n "$TMP_CURRENT" && -L "$TMP_CURRENT" && "$(readlink "$TMP_CURRENT" 2>/dev/null || true)" == "$RELEASES_DIR/$VERSION" ]]; then
    rm -f -- "$TMP_CURRENT"
  fi
  if [[ -n "$STAGE" && "$STAGE" == /tmp/bblbb-release-stage.* && -d "$STAGE" && ! -L "$STAGE" ]]; then
    rm -rf -- "$STAGE"
  fi
  if [[ "$KEEP_PREPARED_RELEASE" == 0 && "${CREATED_PREPARED_RELEASE:-0}" == 1 && -n "$PREPARED_RELEASE" && \
        "$PREPARED_RELEASE" == "$RELEASES_DIR/.staging-$VERSION-$$" && \
        -d "$PREPARED_RELEASE" && ! -L "$PREPARED_RELEASE" ]]; then
    rm -rf -- "$PREPARED_RELEASE"
  fi
}
trap 'status=$?; cleanup_release_temps; cleanup_bundle_snapshot; release_handle_exit "$status"' EXIT
STAGE="$(mktemp -d /tmp/bblbb-release-stage.XXXXXX)" || fail "无法创建暂存目录"
python3 "$ROOT/deploy/scripts/safe-extract-release-bundle.py" --archive "$BUNDLE" --destination "$STAGE" || fail "安全解包 bundle 失败"
python3 - "$STAGE" <<'PY'
import pathlib, re, sys
root = pathlib.Path(sys.argv[1])
manifest = root / "SHA256SUMS"
if not manifest.is_file():
    raise SystemExit("bundle 缺少 SHA256SUMS")
listed = set()
for raw in manifest.read_text(encoding="ascii").splitlines():
    match = re.fullmatch(r"([0-9a-fA-F]{64})  (.+)", raw)
    if not match:
        raise SystemExit("SHA256SUMS contains an invalid row")
    name = pathlib.PurePosixPath(match.group(2))
    if not match.group(2).startswith("./") or name.is_absolute() or ".." in name.parts:
        raise SystemExit("SHA256SUMS contains an unsafe path")
    normalized = name.as_posix()
    if normalized in (".", "SHA256SUMS") or normalized in listed:
        raise SystemExit("SHA256SUMS contains a duplicate or invalid path")
    listed.add(normalized)
actual = {
    path.relative_to(root).as_posix()
    for path in root.rglob("*")
    if path.is_file() and path.relative_to(root).as_posix() != "SHA256SUMS"
}
if listed != actual:
    missing = sorted(actual - listed)[:5]
    extra = sorted(listed - actual)[:5]
    raise SystemExit(f"SHA256SUMS coverage mismatch; unlisted={missing}, missing-files={extra}")
PY
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$STAGE" && sha256sum -c SHA256SUMS) || fail "bundle 内部 SHA256SUMS 校验失败"
else
  (cd "$STAGE" && shasum -a 256 -c SHA256SUMS) || fail "bundle 内部 SHA256SUMS 校验失败"
fi
[[ -x "$STAGE/backend/bblbb-backend" && -x "$STAGE/backend/bblbb-migrate" ]] || fail "bundle 后端二进制不完整"
[[ -d "$STAGE/frontend/build" && -d "$STAGE/frontend/node_modules" ]] || fail "bundle 前端产物不完整"
[[ -d "$STAGE/migrations/sqlite" ]] || fail "bundle 缺少 SQLite migrations"
[[ -f "$STAGE/openapi/openapi.yaml" ]] || fail "bundle 缺少 OpenAPI 契约"
note "    version=$VERSION"

# 先在发布目录所在文件系统准备不可变候选版本；权限/复制失败必须发生在迁移前。
mkdir -p "$RELEASES_DIR"
PREPARED_RELEASE="$RELEASES_DIR/.staging-$VERSION-$$"
[[ ! -e "$PREPARED_RELEASE" && ! -L "$PREPARED_RELEASE" ]] || fail "候选目录已存在: $PREPARED_RELEASE"
mkdir "$PREPARED_RELEASE" || fail "无法原子创建候选目录"
CREATED_PREPARED_RELEASE=1
cp -R "$STAGE/." "$PREPARED_RELEASE/"
chown -R root:bblbb "$PREPARED_RELEASE"
chmod -R a+rX,a-w,a-s,-t "$PREPARED_RELEASE"
chmod 0555 "$PREPARED_RELEASE/backend/bblbb-backend" "$PREPARED_RELEASE/backend/bblbb-migrate"
CURRENT_DIR="$(dirname "$CURRENT_LINK")"
[[ -d "$CURRENT_DIR" ]] || fail "current 符号链接父目录不存在: $CURRENT_DIR"
[[ -L "$CURRENT_LINK" || ! -e "$CURRENT_LINK" ]] || fail "current 路径已存在且不是符号链接: $CURRENT_LINK"
TMP_CURRENT="$CURRENT_DIR/.current-$VERSION-$$"
[[ ! -e "$TMP_CURRENT" && ! -L "$TMP_CURRENT" ]] || fail "临时 current 路径已存在: $TMP_CURRENT"
ln -s "$RELEASES_DIR/$VERSION" "$TMP_CURRENT" || fail "无法预备 current 符号链接"
CREATED_TMP_CURRENT=1

note "==> 2/7 发布前完整备份（SQLite + local attachments）"
BACKUP_ROOT="${BBLBB_BACKUP_ROOT:-/var/lib/bblbb/backups}"
BACKUP_ENV_FILE="${BBLBB_BACKUP_ENV_FILE:-/etc/bblbb/backup.env}"
if [[ -f "$BACKUP_ENV_FILE" && ! -L "$BACKUP_ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  set -a; source "$BACKUP_ENV_FILE"; set +a
fi
[[ "$BACKUP_ROOT" == /* && "$BACKUP_ROOT" == */backups && ! -L "$BACKUP_ROOT" ]] || fail "发布前备份目录必须是绝对的非符号链接 /backups 路径"
mkdir -p "$BACKUP_ROOT"
"$PREPARED_RELEASE/ops/backup/daily.sh" --db "$DB_FILE" \
  --storage-backend local --storage "$STORAGE_DIR" \
  --backup-dir "$BACKUP_ROOT" --label "pre-release-$VERSION" || fail "发布前数据库/附件完整备份失败"
BACKUP_DB="$BACKUP_ROOT/sqlite/pre-release-$VERSION/bblbb.sqlite"
[[ -f "$BACKUP_DB" && ! -L "$BACKUP_DB" && -f "$BACKUP_ROOT/manifest-pre-release-$VERSION.json" && -f "$BACKUP_ROOT/attachments-pre-release-$VERSION.tar.gz" ]] || fail "发布前完整备份集合未生成"

note "==> 3/7 启动检查（origin/Cookie/DB/dirs/migrations/OIDC/外部配置）"
"$ROOT/deploy/scripts/startup-checks.sh" --env-file "$ENV_FILE" \
  --migrations-dir "$PREPARED_RELEASE/migrations/sqlite" \
  --migrate-bin "$PREPARED_RELEASE/backend/bblbb-migrate" || fail "启动检查失败，停止发布"

note "进入维护窗口：停止 frontend → worker → backend；迁移期间保持服务停机"
release_capture_service_state || fail "无法确认当前服务状态，拒绝迁移"
release_require_running || fail "正常发布前要求 backend/worker/frontend 全部 active"
release_stop_stack || fail "停止服务失败，未开始迁移"
note "==> 4/7 显式迁移（bblbb-migrate apply；失败即停，不切流）"
RELEASE_MIGRATION_STARTED=1
BBLBB__DATABASE_URL="$DB_URL" "$PREPARED_RELEASE/backend/bblbb-migrate" apply \
  --migrations-dir "$PREPARED_RELEASE/migrations/sqlite" || {
    KEEP_PREPARED_RELEASE=1
    note "迁移失败：保留候选目录 $PREPARED_RELEASE 供诊断；未切换 current"
    fail "迁移失败（见 ops/runbooks/migration-failure.md）"
  }

note "==> 5/7 原子切换 current"
RELEASE_CUTOVER_STARTED=1
[[ ! -e "$RELEASES_DIR/$VERSION" && ! -L "$RELEASES_DIR/$VERSION" ]] || fail "release 版本目录在迁移期间出现，拒绝覆盖: $VERSION"
mv -Tn -- "$PREPARED_RELEASE" "$RELEASES_DIR/$VERSION"
[[ ! -e "$PREPARED_RELEASE" ]] || fail "版本目标已存在或 no-clobber 发布移动未完成: $VERSION"
PREPARED_RELEASE="$RELEASES_DIR/$VERSION"
KEEP_PREPARED_RELEASE=1
mv -Tf "$TMP_CURRENT" "$CURRENT_LINK" || fail "current 原子切换失败；已保留候选版本 $VERSION"
TMP_CURRENT=""
note "    current → $(readlink "$CURRENT_LINK")"

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
    note "执行回滚：先按发布记录选择明确版本，再运行 release.sh --rollback --target <version>"
    fail "ready 检查失败"
  }
fi
"$ROOT/ops/smoke/smoke.sh" || fail "冒烟失败（保留诊断，执行 --rollback）"

RELEASE_STOP_ATTEMPTED=0
note "发布成功: $VERSION"
