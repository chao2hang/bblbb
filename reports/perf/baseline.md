# BBLBB — 性能基线与容量基线（M16-PERF-03/05/06）

> 机器规格见 `reports/perf/machine.md`（x86_64 / 16 核 / 40GiB / macOS）。
> 数据：`data/perf-bench.sqlite`，由 `bash bench/gen-synthetic.sh` 生成（可复现）。
> 阈值与版本化见 `bench/thresholds.md`（M16-PERF-08）。

## 1. 合成数据（M16-PERF-02，实际记录）

| 表 | 行数 |
|---|---|
| users | 100,000 |
| posts | 1,000,000 |
| post_contents | 1,000,000 |
| comments | 200,000 |

- DB 文件大小：**1,137 MB（WAL 模式）**，`sqlite3` `integrity_check` = ok。
- 满足并超过 512MB 场景目标（≥256MB）；页面尺寸 4096。
- 生成命令：`bash bench/gen-synthetic.sh data/perf-bench.sqlite all`（分阶段 4 批事务）。
- 生成时 DB 处于 WAL + `synchronous=OFF`（仅生成期；运行时恢复 WAL 默认同步）。

## 2. 请求 p95（M16-PERF-03，真实请求，release 构建）

- 拓扑：backend release `127.0.0.1:8181` ← proxy `127.0.0.1:4174`（/api→backend，其余→SSR）
  → frontend SSR（adapter-node release build）`127.0.0.1:4175`。
- 命令：`bash bench/measure.sh`（默认 25 次采样；`SAMPLES=N` 可调）。
- 采样日期：2026-08-08；commit 见 `reports/rc/harness.md`。

| 端点 | p95 (ms) | mean (ms) | n | 说明 |
|---|---|---|---:|---:|---|
| GET /api/v1/posts?limit=20 | **1207** | 1070 | 25 | 1M 行 + `status='published' AND deleted_at IS NULL` 过滤后 ORDER BY created_at 需临时 B-tree 排序（见 §4） |
| GET /api/v1/posts/p000000001 | 17.6 | 16.5 | 25 | 详情 |
| GET /api/v1/boards | 16.5 | 16.1 | 25 | 列表 |
| GET /api/v1/search?q=基准 | 16.6 | 16.1 | 25 | 搜索 |
| POST /api/v1/auth/login | 16.6 | 16.0 | 25 | Argon2id 验证 + 会话创建 |
| POST /api/v1/posts | 17.4 | 16.8 | 25 | 发帖（含渲染/清洗/审计/Outbox 事务） |
| POST /api/v1/posts/p000000001/comments | 17.4 | 16.8 | 25 | 回复 |
| SSR GET /（首页） | 24.4 | 22.1 | 25 | SvelteKit SSR + /api 反代 |
| SSR GET /boards/general | 19.9 | 18.7 | 25 | 板块页 |
| SSR GET /posts/p000000001 | 18.6 | 18.2 | 25 | 文章页 |

## 3. Worker 延迟（M16-PERF-05，实际记录）

| 项 | 值 |
|---|---|
| 邮件队列 50 个 job 全排空 | 6,095 ms（含 worker 启动 ~5.5s） |
| 单 job claim→process→retry_wait | ~20 ms（实测单 job 日志时间戳） |
| 无 SMTP 时邮件 job 分类 | `retry_wait`（可重试；60s 指数退避），不丢队列、不写日志 token |

- 缩略图/图片处理走 upload 完成管道（`backend/tests/storage/upload.rs` 覆盖），
  其延迟已在 §2 的发帖/回复链路隐含测量（同进程内）。
- HTTP 延迟在 worker 满载时无耦合（独立进程，`--worker` 模式）。

## 4. 基线容量（M16-PERF-06）

| 项 | 值 | 来源 |
|---|---|---|
| 峰值 RSS（后端，8 并发负载） | **35 MB** | `ps -o rss` 实测 |
| DB 大小 | 1,137 MB | `du -m` |
| WAL 大小（负载后） | 1.7 MB | `ls -lh` |
| 连接池 | max 8 / min 1（默认） | `src/config.rs` |
| SQLite busy_timeout | 5s（每连接 PRAGMA） | `src/db/pool.rs` |
| 慢查询阈值 | 500ms（`db_slow_query_ms`） | `src/config.rs` |
| 磁盘余量 | 228 GiB 可用（50% 已用） | `df -h` |
| SQLite busy 计数 | 测量期间 `bblbb_sqlite_busy_total` 无持续增长（WAL 并发读 + 顺序写） | `src/observability/metrics.rs` + `tests/sqlite_busy.rs` |

## 5. 已知慢查询（诚实记录，M16-PERF-07）

`GET /api/v1/posts`（无板块/作者过滤）在 1M 行时 p95≈1207ms，超出 500ms 慢查询阈值：

- 原因（`EXPLAIN QUERY PLAN`）：走 `posts_status_visibility_idx` 过滤
  `status='published'`，随后 `USE TEMP B-TREE FOR ORDER BY`（created_at 全量排序）。
- 该场景需要复合索引（如 `(status, deleted_at, created_at)`），属 Schema 变更
  （新迁移），不在 M16 测量里程碑范围；已登记到 `bench/thresholds.md` 的
  "基线变化" 与 `reports/rc/p0-p1.md`（非 P0/P1，性能优化项）。
- 有板块过滤（`board_id` 指定）时走 `posts_board_id_idx`，为常见用户路径，
  不受影响；1M 行量级仅全站无过滤列表触顶。

## 6. 验证（M16-PERF-07 检查项）

- 无持续 SQLite busy：`tests/sqlite_busy.rs`（退避/不高频自旋）+ 实测无 busy 计数增长。
- 无无限增长队列：邮件队列 50 job 在无 SMTP 时全部进入 `retry_wait`（dead-letter
  语义由 `tests/jobs_retry.rs#failure_exceeding_max_attempts_dead_letters` 覆盖），
  队列不无限增长。
- 慢查询回归：§5 已记录唯一超阈值查询；其余端点 p95 < 50ms。
- 错误率：测量期间后端日志无 5xx（`grep '"status":5'` 计数 0，见 `reports/rc/harness.md`）。

复现命令：

```sh
bash bench/gen-synthetic.sh data/perf-bench.sqlite all
SAMPLES=25 bash bench/measure.sh
```
