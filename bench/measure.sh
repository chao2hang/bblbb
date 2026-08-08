#!/usr/bin/env bash
# M16-PERF-03：对 release 构建发真实请求测量 p95。
#
# 拓扑（模拟生产 Caddy）：
#   backend（release）127.0.0.1:8181
#   frontend SSR（adapter-node build）127.0.0.1:4175
#   proxy 127.0.0.1:4174（/api→backend，其余→SSR）
#
# 测量：SSR 首页/板块/文章页 + API 登录/发帖/回复/搜索/列表/详情。
# p95 = 排序后第 ceil(0.95*N) 个采样。结果写入 reports/perf/baseline.md（由主流程汇总）。

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND_PORT="${BACKEND_PORT:-8181}"
SSR_PORT="${SSR_PORT:-4175}"
PROXY_PORT="${PROXY_PORT:-4174}"
N="${SAMPLES:-25}"
DB="${ROOT}/data/perf-bench.sqlite"
PID_DIR="$(mktemp -d)"
PIDS=()

cleanup() {
  for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done
  wait 2>/dev/null || true
  rm -rf "$PID_DIR"
}
trap cleanup EXIT

mkdir -p "$ROOT/reports/perf"

echo "== measure: samples=$N db=$DB =="
[ -f "$DB" ] || { echo "missing $DB — 先运行 bash bench/gen-synthetic.sh"; exit 1; }

# --- build release backend ---
if [ ! -x "$ROOT/backend/target/release/bblbb-backend" ]; then
  echo "-- building backend release --"
  (cd "$ROOT/backend" && cargo build --release --bin bblbb-backend)
fi

# --- start backend ---
echo "-- starting backend on 127.0.0.1:$BACKEND_PORT --"
BBLBB__BIND_ADDRESS="127.0.0.1:$BACKEND_PORT" \
BBLBB__DATABASE_URL="sqlite://$DB" \
BBLBB__AUTO_MIGRATE=false \
BBLBB__MIGRATIONS_DIR="$ROOT/migrations/sqlite" \
BBLBB__STORAGE_BACKEND=local \
BBLBB__STORAGE_DIR="$ROOT/data/uploads-bench" \
BBLBB__LOG_FORMAT=json \
BBLBB__LOG_FILTER=bblbb_backend=info \
  "$ROOT/backend/target/release/bblbb-backend" >"$PID_DIR/backend.log" 2>&1 &
PIDS+=($!)
for i in $(seq 1 60); do
  curl -s -o /dev/null "http://127.0.0.1:$BACKEND_PORT/healthz" && break
  sleep 1
done
curl -s "http://127.0.0.1:$BACKEND_PORT/healthz" >/dev/null || { echo "backend failed"; cat "$PID_DIR/backend.log" | tail -20; exit 1; }

# --- build/start frontend SSR + proxy ---
if [ ! -d "$ROOT/frontend/build" ]; then
  echo "-- building frontend --"
  (cd "$ROOT/frontend" && npm run build)
fi
PORT="$SSR_PORT" node "$ROOT/frontend/build/index.js" >"$PID_DIR/ssr.log" 2>&1 &
PIDS+=($!)
SSR_PORT="$SSR_PORT" node "$ROOT/bench/proxy.mjs" "$PROXY_PORT" "$BACKEND_PORT" >"$PID_DIR/proxy.log" 2>&1 &
PIDS+=($!)
sleep 2
curl -s -o /dev/null "http://127.0.0.1:$PROXY_PORT/" || { echo "proxy failed"; cat "$PID_DIR/proxy.log"; exit 1; }

# --- 准备会话（u000000001）---
SESSION_ID="$(uuidgen | tr -d '-' || openssl rand -hex 8)"
CSRF_SECRET="$(openssl rand -hex 16)"
TOKEN="$(openssl rand -hex 24)"
TOKEN_HASH="$(printf '%s' "$TOKEN" | shasum -a 256 | cut -d' ' -f1)"
CSRF_TOKEN="$(printf '%s:%s:%s' "$SESSION_ID" "$CSRF_SECRET" "csrf" | shasum -a 256 | cut -d' ' -f1)"
NOW_MS="$(( $(date +%s) * 1000 ))"
IDLE_EXPIRES="$(( NOW_MS + 3600000 ))"
ABS_EXPIRES="$(( NOW_MS + 86400000 ))"
sqlite3 "$DB" "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, user_agent, created_at, last_seen_at, idle_expires_at, absolute_expires_at, auth_verified_at)
  VALUES ('$SESSION_ID', 'u000000001', '$TOKEN_HASH', '$CSRF_SECRET', 'perf-measure', $NOW_MS, $NOW_MS, $IDLE_EXPIRES, $ABS_EXPIRES, $NOW_MS);"
COOKIE="__Host-bblbb_session=$TOKEN"
CSRF_HEADER="X-CSRF-Token: $CSRF_TOKEN"

# --- 注册登录用户（一次性；重复运行命中注册限流时复用已有用户）---
LOGIN_USER="perflogin"
if ! sqlite3 "$DB" "SELECT 1 FROM users WHERE username_normalized='$LOGIN_USER';" | grep -q 1; then
  echo "-- registering login user --"
  PRE="$(curl -s -c "$PID_DIR/pre.jar" "http://127.0.0.1:$BACKEND_PORT/api/v1/auth/csrf")"
  PRE_COOKIE="$(awk '/csrf/{print $7}' "$PID_DIR/pre.jar" | head -1)"
  PRE_TOKEN="$(printf '%s' "$PRE" | ruby -rjson -e 'puts JSON.parse(STDIN.read)["token"]')"
  curl -s -b "$PID_DIR/pre.jar" -H "Content-Type: application/json" -H "X-CSRF-Token: $PRE_TOKEN" \
    -d "{\"username\":\"$LOGIN_USER\",\"email\":\"$LOGIN_USER@example.com\",\"password\":\"benchmark-password-123\"}" \
    -X POST "http://127.0.0.1:$BACKEND_PORT/api/v1/auth/register" >/dev/null || true
  VERIFY_TOKEN="$(sqlite3 "$DB" "SELECT token_hash FROM email_verification_tokens WHERE user_id=(SELECT id FROM users WHERE username_normalized='$LOGIN_USER') ORDER BY created_at DESC LIMIT 1;")"
  # verify-email 需要原始 token（DB 存哈希）——改用直接置为已验证。
  sqlite3 "$DB" "UPDATE users SET email_verified=1, email_verified_at=$NOW_MS, status='active' WHERE username_normalized='$LOGIN_USER';"
fi
LOGIN_USER_ID="$(sqlite3 "$DB" "SELECT id FROM users WHERE username_normalized='$LOGIN_USER';")"
if [ -z "$LOGIN_USER_ID" ]; then
  echo "login user missing"; exit 1
fi

# --- p95 helper ---
p95() {
  ruby -e '
    times = STDIN.each_line.map(&:to_f).sort
    n = times.length
    idx = [(n * 0.95).ceil - 1, n - 1].min
    idx = 0 if idx < 0
    mean = times.sum / n
    puts "#{times[idx].round(3)}\t#{mean.round(3)}\t#{n}"
  '
}

measure() {
  local name="$1"; shift
  local times_file="$PID_DIR/times-${name}.txt"
  : > "$times_file"
  for _ in $(seq 1 "$N"); do
    local t0 t1
    t0="$(date +%s%N)"
    "$@" >/dev/null 2>&1
    t1="$(date +%s%N)"
    echo "scale=3; ($t1 - $t0) / 1000000" | bc >> "$times_file"
  done
  local result
  result="$(p95 < "$times_file")"
  local p95v meanv cnt
  p95v="$(echo "$result" | cut -f1)"; meanv="$(echo "$result" | cut -f2)"; cnt="$(echo "$result" | cut -f3)"
  echo "$name: p95=${p95v}ms mean=${meanv}ms n=$cnt"
  echo -e "$name\t$p95v\t$meanv\t$cnt" >> "$PID_DIR/summary.tsv"
}

H_AUTH="-H \"Cookie: $COOKIE\" -H \"$CSRF_HEADER\" -H \"Content-Type: application/json\""

echo "== measuring API (backend direct) =="
measure posts_list curl -s "http://127.0.0.1:$BACKEND_PORT/api/v1/posts?limit=20"
measure post_detail curl -s "http://127.0.0.1:$BACKEND_PORT/api/v1/posts/p000000001"
measure boards_list curl -s "http://127.0.0.1:$BACKEND_PORT/api/v1/boards"
measure search curl -s "http://127.0.0.1:$BACKEND_PORT/api/v1/search?q=%E5%9F%BA%E5%87%86"
measure login curl -s -H "Content-Type: application/json" -d "{\"username\":\"$LOGIN_USER\",\"password\":\"benchmark-password-123\"}" -X POST "http://127.0.0.1:$BACKEND_PORT/api/v1/auth/login"
measure create_post curl -s -H "Cookie: $COOKIE" -H "X-CSRF-Token: $CSRF_TOKEN" -H "Content-Type: application/json" -d '{"type":"discussion","title":"perf measure post","markdown":"perf body"}' -X POST "http://127.0.0.1:$BACKEND_PORT/api/v1/posts"
measure create_comment curl -s -H "Cookie: $COOKIE" -H "X-CSRF-Token: $CSRF_TOKEN" -H "Content-Type: application/json" -d '{"markdown":"perf comment"}' -X POST "http://127.0.0.1:$BACKEND_PORT/api/v1/posts/p000000001/comments"

echo "== measuring SSR pages (through proxy) =="
measure ssr_home curl -s "http://127.0.0.1:$PROXY_PORT/"
measure ssr_board curl -s "http://127.0.0.1:$PROXY_PORT/boards/general"
measure ssr_article curl -s "http://127.0.0.1:$PROXY_PORT/posts/p000000001"

# --- worker 延迟（邮件队列 50 个 job 排空时间）---
echo "== measuring worker latency (50 mail jobs drain) =="
WORKER_N=50
NOW_MS="$(( $(date +%s) * 1000 ))"
sqlite3 "$DB" "BEGIN;
WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM cnt WHERE x < $WORKER_N)
INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, deduplication_key, created_at, updated_at)
SELECT printf('w%05d', x), 'mail', 'email.deliver',
       printf('{\"user_id\":\"u000000001\",\"template_key\":\"security.notice\",\"params\":{}}'),
       1, 'queued', 0, 5, $NOW_MS, printf('bench-mail-%05d', x), $NOW_MS, $NOW_MS
FROM cnt;
COMMIT;"
W0="$(date +%s%N)"
BBLBB__BIND_ADDRESS="127.0.0.1:$(( BACKEND_PORT + 1 ))" \
BBLBB__DATABASE_URL="sqlite://$DB" \
BBLBB__AUTO_MIGRATE=false \
BBLBB__MIGRATIONS_DIR="$ROOT/migrations/sqlite" \
BBLBB__STORAGE_BACKEND=local \
BBLBB__STORAGE_DIR="$ROOT/data/uploads-bench" \
BBLBB__LOG_FORMAT=json \
BBLBB__LOG_FILTER=bblbb_backend=info \
  "$ROOT/backend/target/release/bblbb-backend" --worker >"$PID_DIR/worker.log" 2>&1 &
PIDS+=($!)
DRAINED=0
for i in $(seq 1 120); do
  REMAIN="$(sqlite3 "$DB" "SELECT COUNT(*) FROM jobs WHERE kind='email.deliver' AND status IN ('queued','running');")"
  [ "$REMAIN" = "0" ] && { DRAINED=1; break; }
  sleep 0.25
done
W1="$(date +%s%N)"
ELAPSED_MS="$(echo "scale=3; ($W1 - $W0) / 1000000" | bc)"
if [ "$DRAINED" = "1" ]; then
  PER_JOB="$(echo "scale=3; $ELAPSED_MS / $WORKER_N" | bc)"
  echo "worker_mail_drain: ${ELAPSED_MS}ms for $WORKER_N jobs, per_job=${PER_JOB}ms"
  echo -e "worker_mail_drain\t${ELAPSED_MS}\t${PER_JOB}\t$WORKER_N" >> "$PID_DIR/summary.tsv"
else
  echo "worker: 未在 30s 内排空邮件队列（邮件无 SMTP 时按永久失败 dead-letter，属预期）"
fi
sqlite3 "$DB" "SELECT status, COUNT(*) FROM jobs WHERE kind='email.deliver' GROUP BY status;" | sed 's/^/job_status: /'

echo
echo "== summary (name p95_ms mean_ms n) =="
cat "$PID_DIR/summary.tsv" | sort
