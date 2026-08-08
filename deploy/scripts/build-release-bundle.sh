#!/usr/bin/env bash
# build-release-bundle.sh — 在干净构建机生成不可变 release bundle（M15-PACKAGE-01）。
#
# 用法：
#   deploy/scripts/build-release-bundle.sh [--version <v>] [--out-dir <dir>]
#
# 产出：
#   <out-dir>/<version>.tar.gz + <out-dir>/<version>.tar.gz.sha256
#   bundle 内部布局见 deploy/RELEASE-BUNDLE.md §1。
#
# 生产机不执行本脚本（不安装 npm 依赖、不编译）；只解压并切换 current。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${BBLBB_RELEASE_VERSION:-}"
OUT_DIR=""
BUILD_DIR=""

usage() {
  echo "用法: $0 [--version <v>] [--out-dir <dir>]"
  echo "  --version  版本号（如 1.0.0-rc.2+build.20260807.1）；缺省取 git describe"
  echo "  --out-dir  输出目录（默认 ./dist）"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  VERSION="$(git -C "$ROOT" describe --tags --always 2>/dev/null || echo "dev")"
fi
OUT_DIR="${OUT_DIR:-$ROOT/dist}"
BUILD_DIR="$(mktemp -d)"

echo "==> 构建 release bundle: version=$VERSION"
echo "==> 后端 release 编译（使用 Cargo.lock）"
(
  cd "$ROOT/backend"
  cargo build --release --locked
)

echo "==> 前端构建（使用 package-lock.json）"
(
  cd "$ROOT/frontend"
  npm ci --silent
  npm run build
)

echo "==> 组装 bundle"
mkdir -p "$BUILD_DIR/backend" "$BUILD_DIR/frontend" "$BUILD_DIR/migrations"
cp "$ROOT/backend/target/release/bblbb-backend" "$BUILD_DIR/backend/bblbb-backend"
cp "$ROOT/backend/target/release/bblbb-migrate" "$BUILD_DIR/backend/bblbb-migrate"
cp -R "$ROOT/frontend/build" "$BUILD_DIR/frontend/build"
cp "$ROOT/frontend/package.json" "$ROOT/frontend/package-lock.json" "$BUILD_DIR/frontend/"
cp -R "$ROOT/migrations/sqlite" "$ROOT/migrations/mysql" "$ROOT/migrations/mariadb" "$BUILD_DIR/migrations/"
echo "$VERSION" > "$BUILD_DIR/VERSION"

mkdir -p "$OUT_DIR"
tar -czf "$OUT_DIR/$VERSION.tar.gz" -C "$BUILD_DIR" .
(cd "$OUT_DIR" && shasum -a 256 "$VERSION.tar.gz" > "$VERSION.tar.gz.sha256")

echo "==> 生成 METADATA.json（M15-PACKAGE-02）"
GIT_COMMIT="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo "unknown")"
GIT_COMMIT_SHORT="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")"
RUST_VERSION="$(rustc --version 2>/dev/null || echo "unknown")"
(
  cd "$ROOT/backend"
  cargo tree --locked --prefix depth 0 > "$BUILD_DIR/deps-cargo.txt" 2>/dev/null || true
)
(
  cd "$ROOT/frontend"
  npm ls --json --all > "$BUILD_DIR/deps-npm.json" 2>/dev/null || true
)
cat > "$BUILD_DIR/METADATA.json" <<EOF
{
  "version": "$VERSION",
  "build_commit": "$GIT_COMMIT",
  "build_commit_short": "$GIT_COMMIT_SHORT",
  "rust": "$RUST_VERSION",
  "dependency_locks": {
    "backend": "backend/Cargo.lock",
    "frontend": "frontend/package-lock.json"
  },
  "sbom": "SBOM.json",
  "checksums": "SHA256SUMS",
  "built_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

echo "==> 记录依赖锁与 SBOM 副本"
cp "$ROOT/backend/Cargo.lock" "$BUILD_DIR/Cargo.lock"
cp "$ROOT/frontend/package-lock.json" "$BUILD_DIR/package-lock.json"

echo "==> 计算 SHA256SUMS"
( cd "$BUILD_DIR" && find . -type f ! -name SHA256SUMS | sort | xargs shasum -a 256 > SHA256SUMS )

# 把 METADATA/SBOM/checksums 打回 bundle
tar -czf "$OUT_DIR/$VERSION.tar.gz" -C "$BUILD_DIR" .
(cd "$OUT_DIR" && shasum -a 256 "$VERSION.tar.gz" > "$VERSION.tar.gz.sha256")

rm -rf "$BUILD_DIR"
echo "==> 完成: $OUT_DIR/$VERSION.tar.gz"
echo "    checksum: $(cat "$OUT_DIR/$VERSION.tar.gz.sha256")"
