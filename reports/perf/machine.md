# BBLBB — 压测机器规格（M16-PERF-01）

> 本文件固定压测机器硬件/软件、数据库版本、commit、命令与数据生成参数；
> 所有性能基线（`reports/perf/baseline.md`）必须引用本规格，换机重测必须新建记录。

## 硬件

| 项 | 值 | 采集命令 |
|---|---|---|
| 架构 | `x86_64` | `uname -m` |
| CPU 核心 | 16 | `sysctl -n hw.ncpu` |
| 内存 | 42949672960 字节（40 GiB） | `sysctl -n hw.memsize` |
| 磁盘（工作区所在卷） | 477 GiB 总量 / 236 GiB 可用 / 49% 已用 | `df -h` |

## 软件

| 项 | 值 |
|---|---|
| OS | macOS (Darwin 25) |
| Rust | 1.97.1（`rust-toolchain.toml` 固定） |
| SQLite | 3.51.0 2025-06-12（系统 CLI + sqlx bundled） |
| Ruby | 2.6.10 |
| Node | v26.5.1 |
| 后端构建 | `cargo build --release`（opt-level 3 默认） |

## 基准 commit

- 数据生成与测量执行时的 commit：本里程碑（M16）提交 hash，见 `git rev-parse HEAD`
  与 `reports/rc/harness.md`。
- 上一基线（M15）commit：`468883e`。

## 数据生成参数（M16-PERF-02）

命令：`bash bench/gen-synthetic.sh data/perf-bench.sqlite`

- 用户：100,000
- 帖子：1,000,000（每用户 10 篇，循环分配 author）
- 评论：200,000（每帖 0–1 条，覆盖回复分页）
- 阶段式生成（每 10 万帖一个事务批次），最终行数与 DB 大小记录在
  `reports/perf/baseline.md`（目标 ≥256MB 或如实记录）。

## 测量方法（M16-PERF-03）

命令：`bash bench/measure.sh`（release 构建 + 迁移 + 种子 + 真实 HTTP 请求）

- 测量对象：`GET /`、`GET /api/v1/posts/{id}`、`GET /api/v1/boards`（公开 SSR 路径）、
  `POST /api/v1/auth/login`、`POST /api/v1/posts`、`POST /api/v1/comments`、
  `GET /api/v1/search?q=...`。
- 每端点 N 次采样 → p95（排序取 95 分位），全部为对运行中真实服务器的 curl 请求。
- 不在同一机器并发压测多个后端进程（避免锁竞争污染数据）。
