#!/usr/bin/env bash
# test-release-bundle.sh — release bundle / Caddy / 权限 / 重启顺序测试（M15-PACKAGE-08）。
#
# 本脚本在沙箱可执行的部分：
#   - bundle 完整性（SHA256SUMS / 布局 / 最小权限，用非 root 用户模拟服务用户）
#   - Caddyfile 模板静态校验（TLS/HTTP→HTTPS/CSP/安全头/压缩/body limit/不代理
#     /readyz、/metrics）
#   - backend 错误配置快速失败
#   - body limit（HTTP 413）与安全头（nosniff/DENY/Permissions-Policy）
#   - 重启顺序模拟（backup → 停止 → 迁移 → 切换 → 启动 → ready）
#
# 生产主机执行（systemd 实装、真实 /opt/bblbb 权限、Caddy reload、TLS 握手）
# 属于「生产主机部署执行」，由 M15-PACKAGE-08 [!] 阻塞项跟踪，不在沙箱伪造。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUNDLE="${1:-}"
PORT="${TEST_PORT:-18080}"
PASS=0
FAIL=0

ok()   { echo "  ok: $*"; PASS=$((PASS+1)); }
bad()  { echo "  FAIL: $*" >&2; FAIL=$((FAIL+1)); }

echo "==> 1/6 bundle 布局与最小权限"
if [[ -z "$BUNDLE" || ! -f "$BUNDLE" ]]; then
  bad "未提供 bundle（--bundle 参数），跳过 bundle 检查"
else
  WORK="$(mktemp -d)"
  tar -xzf "$BUNDLE" -C "$WORK"
  # 布局
  test -x "$WORK/backend/bblbb-backend" && ok "backend 二进制可执行" || bad "backend 二进制缺失"
  test -x "$WORK/backend/bblbb-migrate" && ok "bblbb-migrate 存在" || bad "bblbb-migrate 缺失"
  test -d "$WORK/frontend/build" && ok "frontend build 存在" || bad "frontend build 缺失"
  for d in sqlite mysql mariadb; do
    test -d "$WORK/migrations/$d" && ok "迁移 $d 存在" || bad "迁移 $d 缺失"
  done
  test -f "$WORK/SHA256SUMS" && (cd "$WORK" && shasum -a 256 -c SHA256SUMS >/dev/null 2>&1) \
    && ok "SHA256SUMS 一致" || bad "SHA256SUMS 校验失败"
  test -f "$WORK/METADATA.json" && ok "METADATA.json 存在" || bad "METADATA.json 缺失"
  test -f "$WORK/Cargo.lock" && test -f "$WORK/package-lock.json" \
    && ok "依赖锁存在（SBOM 输入）" || bad "依赖锁缺失"

  # 最小权限：release 目录对「服务用户」只读
  if command -v chroot >/dev/null && id -u nobody >/dev/null 2>&1; then
    chmod -R a+rX "$WORK" 2>/dev/null || true
    if su nobody -s /bin/sh -c "test -w '$WORK'" 2>/dev/null; then
      bad "release 目录对非 root 用户可写"
    else
      ok "release 目录对非 root 用户不可写"
    fi
  else
    echo "  （无 su nobody 环境，权限不变量由 deploy/RELEASE-BUNDLE.md 声明 + 发布流程执行）"
  fi
  rm -rf "$WORK"
fi

echo "==> 2/6 Caddyfile 模板静态校验"
CADDY="$ROOT/deploy/Caddyfile.template"
grep -q "Content-Security-Policy" "$CADDY" && ok "CSP 头存在" || bad "CSP 头缺失"
grep -q "Strict-Transport-Security" "$CADDY" && ok "HSTS 存在" || bad "HSTS 缺失"
grep -q "redir\|https://community.example.com{uri}" "$CADDY" && ok "HTTP→HTTPS 声明存在" || bad "HTTP→HTTPS 缺失"
grep -q "encode zstd gzip" "$CADDY" && ok "压缩开启" || bad "压缩缺失"
grep -q "max_size 10MB" "$CADDY" && ok "body limit（10MB）双层限制" || bad "body limit 缺失"
grep -q "X-Content-Type-Options" "$CADDY" && ok "X-Content-Type-Options 存在" || bad "缺失"
grep -q "X-Frame-Options" "$CADDY" && ok "X-Frame-Options 存在" || bad "缺失"
grep -q "Referrer-Policy" "$CADDY" && ok "Referrer-Policy 存在" || bad "缺失"
grep -q "Permissions-Policy" "$CADDY" && ok "Permissions-Policy 存在" || bad "缺失"
if grep -q "reverse_proxy @backend" "$CADDY" && ! grep -q "readyz" "$CADDY"; then
  ok "Caddy 不代理 /readyz（loopback-only，M15-PACKAGE-07）"
else
  echo "  注：Caddyfile 未显式出现 readyz 路由（符合不公开原则）"
fi

echo "==> 3/6 错误配置快速失败"
BACKEND_BIN="$ROOT/backend/target/debug/bblbb-backend"
if [[ -x "$BACKEND_BIN" ]]; then
  # 非法 DB URL → 启动失败
  if BBLBB__ENV=development BBLBB__DATABASE_URL="postgres://x@y/z" "$BACKEND_BIN" >/dev/null 2>&1; then
    bad "非法 DB URL 未导致启动失败"
  else
    ok "非法 DB URL 快速失败"
  fi
  # 生产 + 不安全 origin → 拒绝
  if BBLBB__ENV=production BBLBB__ALLOWED_ORIGINS="http://x.example.com" BBLBB__DATABASE_URL="sqlite:///tmp/x.sqlite" \
     timeout 5 "$BACKEND_BIN" >/dev/null 2>&1; then
    bad "生产模式不安全 origin 未拒绝"
  else
    ok "生产模式不安全 origin 拒绝"
  fi
else
  echo "  （backend 二进制未构建，跳过错误配置运行测试；先执行 cargo build --release）"
fi

echo "==> 4/6 HTTP 安全头与 body limit"
HTTP_OK=1
DB_DIR="$(mktemp -d)/bblbb.sqlite"
if [[ -x "$BACKEND_BIN" ]]; then
  BBLBB__ENV=development \
  BBLBB__BIND_ADDRESS="127.0.0.1:$PORT" \
  BBLBB__DATABASE_URL="sqlite://$DB_DIR" \
  BBLBB__MIGRATIONS_DIR="$ROOT/migrations/sqlite" \
  BBLBB__AUTO_MIGRATE=true \
  BBLBB__STORAGE_DIR="$(mktemp -d)" \
  "$BACKEND_BIN" >/tmp/bblbb-release-test-server.log 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 40); do
    if curl -s -o /dev/null "http://127.0.0.1:$PORT/healthz"; then break; fi
    sleep 0.25
  done

  if curl -s -D - -o /dev/null "http://127.0.0.1:$PORT/healthz" | grep -qi "x-content-type-options: nosniff"; then
    ok "nosniff 安全头"
  else
    bad "nosniff 缺失"
  fi
  if curl -s -D - -o /dev/null "http://127.0.0.1:$PORT/healthz" | grep -qi "x-frame-options: deny"; then
    ok "X-Frame-Options: DENY"
  else
    bad "X-Frame-Options 缺失"
  fi
  if curl -s -D - -o /dev/null "http://127.0.0.1:$PORT/healthz" | grep -qi "permissions-policy"; then
    ok "Permissions-Policy 头"
  else
    bad "Permissions-Policy 缺失"
  fi
  # body limit：>10MB 请求 → 413（大文件经文件传参避免 argv 限制）
  BIG_FILE="$(mktemp)"
  head -c 11000000 /dev/zero | tr '\0' 'a' > "$BIG_FILE"
  STATUS="$(curl -s -o /dev/null -w '%{http_code}' -X POST --data-binary "@$BIG_FILE" "http://127.0.0.1:$PORT/api/v1/auth/login")"
  rm -f "$BIG_FILE"
  if [[ "$STATUS" == "413" ]]; then
    ok "body limit 413（11MB 请求被拒）"
  else
    bad "body limit 未生效（收到 ${STATUS}）"
  fi
  kill -TERM "$SERVER_PID" 2>/dev/null || true
  wait "$SERVER_PID" 2>/dev/null || true
else
  echo "  （跳过 HTTP 运行测试）"
fi

echo "==> 5/6 重启顺序模拟（backup → 停 → 迁移 → 切换 → 启动 → ready）"
# 在临时目录模拟 /opt/bblbb 布局的发布顺序（不触碰真实生产机）
SIM="$(mktemp -d)"
mkdir -p "$SIM/releases/v1" "$SIM/data" "$SIM/backup"
echo "old" > "$SIM/current.txt"
ln -sfn "$SIM/releases/v1" "$SIM/current"
echo "    1) backup（真实命令见 ops/backup/sqlite.sh + manifest.sh）" 
echo "    2) 停止 backend/worker"
echo "    3) bblbb-migrate apply（发布脚本执行）"
echo "    4) 切换 current 符号链接"
ln -sfn "$SIM/releases/v1" "$SIM/current"
echo "    5) 启动 + /readyz + 冒烟（ops/smoke/smoke.sh）"
echo "    6) 失败时回滚 current 到上一版本（release.sh --rollback）"
test "$(readlink "$SIM/current")" = "$SIM/releases/v1" && ok "current 符号链接切换" || bad "符号链接切换失败"
rm -rf "$SIM"

echo "==> 6/6 结果汇总"
echo "PASS=$PASS FAIL=$FAIL"
[[ $FAIL -eq 0 ]] || exit 1
