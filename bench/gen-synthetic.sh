#!/usr/bin/env bash
# M16-PERF-02：SQLite 合成数据生成（分阶段，可复现）。
#
# 目标场景：10 万用户、100 万帖子、20 万评论（帖子/回复级），DB ≥256MB。
# 用法：bash bench/gen-synthetic.sh [输出 DB 路径] [阶段]
#   阶段：all | users | posts | contents | comments | report
# 每次执行把最终行数 + DB 大小追加写入 reports/perf/baseline.md（由主流程汇总）。

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DB="${1:-$ROOT/data/perf-bench.sqlite}"
STAGE="${2:-all}"
MIGRATIONS="$ROOT/migrations/sqlite"

N_USERS=100000
N_POSTS=1000000
N_COMMENTS=200000

echo "== gen-synthetic: DB=$DB stage=$STAGE =="
mkdir -p "$(dirname "$DB")"

run_sql() {
  sqlite3 -bail "$DB" "$1"
}

if [ "$STAGE" = "all" ] || [ "$STAGE" = "users" ]; then
  rm -f "$DB" "$DB-wal" "$DB-shm"
  echo "-- applying migrations --"
  for f in "$MIGRATIONS"/*.sql; do
    sqlite3 -bail "$DB" < "$f"
  done
  sqlite3 -bail "$DB" 'PRAGMA foreign_key_check;' | { grep -q . && echo "FK FAIL" && exit 1 || true; }
  sqlite3 -bail "$DB" 'PRAGMA journal_mode=WAL; PRAGMA synchronous=OFF; PRAGMA cache_size=-200000;'
fi

if [ "$STAGE" = "all" ] || [ "$STAGE" = "users" ]; then
  echo "-- users x$N_USERS --"
  run_sql "BEGIN;
  WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM cnt WHERE x < $N_USERS)
  INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
  SELECT printf('u%09d', x), printf('user%09d', x), printf('user%09d@example.com', x),
         '\$argon2id\$v=19\$m=65536,t=3,p=1\$benchmark', 'active',
         1750000000000 + (x % 86400000), 1750000000000 + (x % 86400000)
  FROM cnt;
  COMMIT;"
fi

if [ "$STAGE" = "all" ] || [ "$STAGE" = "posts" ]; then
  echo "-- posts x$N_POSTS (分 4 批) --"
  for batch in 0 1 2 3; do
    LO=$((batch * N_POSTS / 4 + 1)); HI=$(((batch + 1) * N_POSTS / 4))
    run_sql "BEGIN;
    WITH RECURSIVE cnt(x) AS (SELECT $LO UNION ALL SELECT x+1 FROM cnt WHERE x < $HI)
    INSERT INTO posts (id, board_id, author_id, title, content, status, visibility, reply_count, view_count, created_at, updated_at, post_type)
    SELECT printf('p%09d', x),
           (CASE (x % 5) WHEN 0 THEN '01911fd5-f000-7561-a2a5-3dd6434157f0'
                         WHEN 1 THEN '01911fd5-f001-758e-a95d-a58489fbb61d'
                         WHEN 2 THEN '01911fd5-f002-7222-8742-68e793fcdbd5'
                         WHEN 3 THEN '01911fd5-f003-7772-b594-c29b2b8c9021'
                         ELSE '01911fd5-f004-7d9c-b6c0-d2c3387e5534' END),
           printf('u%09d', (x % $N_USERS) + 1),
           printf('基准测试帖子标题 %09d：性能验收合成数据', x),
           printf('这是第 %09d 号帖子的正文内容。本仓库用于 M16-PERF 的 SQLite 512MB 场景合成数据，包含公开、分页与搜索测量所需的最小正文体积。', x),
           'published', 'public', (x % 2), (x % 997), 1750000000000 + (x % 86400000), 1750000000000 + (x % 86400000), 'discussion'
    FROM cnt;
    COMMIT;"
  done
fi

if [ "$STAGE" = "all" ] || [ "$STAGE" = "contents" ]; then
  echo "-- post_contents x$N_POSTS --"
  for batch in 0 1 2 3; do
    LO=$((batch * N_POSTS / 4 + 1)); HI=$(((batch + 1) * N_POSTS / 4))
    run_sql "BEGIN;
    WITH RECURSIVE cnt(x) AS (SELECT $LO UNION ALL SELECT x+1 FROM cnt WHERE x < $HI)
    INSERT INTO post_contents (post_id, body_markdown, body_html, renderer_version, excerpt, updated_at)
    SELECT printf('p%09d', x),
           printf('这是第 %09d 号帖子的 Markdown 正文，包含链接 [示例](https://example.com) 与列表用于渲染体积模拟。', x),
           printf('<p>这是第 %09d 号帖子的 <strong>HTML</strong> 正文。</p>', x),
           'markdown-v1',
           printf('第 %09d 号帖子摘要', x),
           1750000000000 + (x % 86400000)
    FROM cnt;
    COMMIT;"
  done
fi

if [ "$STAGE" = "all" ] || [ "$STAGE" = "comments" ]; then
  echo "-- comments x$N_COMMENTS --"
  run_sql "BEGIN;
  WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM cnt WHERE x < $N_COMMENTS)
  INSERT INTO comments (id, post_id, author_id, content, status, floor, created_at, updated_at)
  SELECT printf('c%09d', x), printf('p%09d', x + 1), printf('u%09d', ((x * 7) % $N_USERS) + 1),
         printf('第 %09d 号回复内容，用于回复分页与楼层唯一性测量。', x),
         'published', 1, 1750000000000 + (x % 86400000), 1750000000000 + (x % 86400000)
  FROM cnt;
  COMMIT;"
fi

if [ "$STAGE" = "all" ] || [ "$STAGE" = "report" ]; then
  echo "-- report --"
  run_sql "PRAGMA journal_mode; PRAGMA page_size;"
  U="$(run_sql 'SELECT COUNT(*) FROM users;')"
  P="$(run_sql 'SELECT COUNT(*) FROM posts;')"
  C="$(run_sql 'SELECT COUNT(*) FROM comments;')"
  PC="$(run_sql 'SELECT COUNT(*) FROM post_contents;')"
  SIZE_MB="$(du -m "$DB" | cut -f1)"
  echo "rows: users=$U posts=$P comments=$C post_contents=$PC"
  echo "db size: ${SIZE_MB}MB ($DB)"
  {
    echo "- users=$U posts=$P comments=$C post_contents=$PC"
    echo "- db_size_mb=$SIZE_MB db=$DB"
  } > /tmp/gen-synthetic-report.txt
fi

echo "== done =="
