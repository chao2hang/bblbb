#!/usr/bin/env bash
# record-release-metadata.sh — 把构建 commit/版本/依赖锁/SBOM/checksums 写入
# release metadata（M15-PACKAGE-02）。
#
# 用法：
#   deploy/scripts/record-release-metadata.sh --bundle <bundle.tar.gz>
#
# 读取 bundle 内的 METADATA.json/SHA256SUMS/SBOM 输入并校验；输出一份
# 摘要 manifest（stdout）供发布记录（docs/CHANGELOG.md / deploy/RELEASES.md）。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUNDLE=""

usage() {
  echo "用法: $0 --bundle <bundle.tar.gz>"
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle) BUNDLE="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done

if [[ -z "$BUNDLE" || ! -f "$BUNDLE" ]]; then
  echo "错误: --bundle 必须是存在的文件" >&2
  usage
fi

WORK="$(mktemp -d)"
tar -xzf "$BUNDLE" -C "$WORK"

echo "==> 校验 bundle 完整性（SHA256SUMS）"
( cd "$WORK" && shasum -a 256 -c SHA256SUMS >/dev/null )

echo "==> 校验 METADATA.json 必需字段"
python3 - "$WORK/METADATA.json" <<'PYEOF'
import json, sys
meta = json.load(open(sys.argv[1]))
for field in ("version", "build_commit", "rust", "dependency_locks", "sbom", "checksums", "built_at"):
    assert meta.get(field), f"METADATA.json 缺少 {field}"
print(f"  version={meta['version']} commit={meta['build_commit']} built_at={meta['built_at']}")
PYEOF

echo "==> 校验产物必须存在（backend 二进制 / frontend build / 三方言迁移）"
test -x "$WORK/backend/bblbb-backend"
test -d "$WORK/frontend/build"
for d in sqlite mysql mariadb; do
  test -d "$WORK/migrations/$d"
done
echo "  backend/frontend/migrations 齐全"

echo "==> SBOM 派生清单（依赖锁快照）"
SBOM_LINES="$(wc -l < "$WORK/Cargo.lock")"
NPM_DEPS="$(python3 -c "import json;d=json.load(open('$WORK/deps-npm.json'));print(len(d.get('dependencies',{})))" 2>/dev/null || echo "n/a")"
echo "  Cargo.lock 行数: ${SBOM_LINES}；npm 顶层依赖: ${NPM_DEPS}"

echo "==> 摘要 manifest（写入 deploy/RELEASES.md 的发布记录）"
VERSION="$(python3 -c "import json;print(json.load(open('$WORK/METADATA.json'))['version'])")"
COMMIT="$(python3 -c "import json;print(json.load(open('$WORK/METADATA.json'))['build_commit_short'])")"
SUM="$(cat "$WORK/METADATA.json" >/dev/null && grep -v '^$' "$WORK/SHA256SUMS" | head -1 | awk '{print $1}')"

cat <<EOF
| $VERSION | \`$COMMIT\` | \`$(basename "$BUNDLE")\` | \`$SUM\` |
EOF

rm -rf "$WORK"
echo "==> OK"
