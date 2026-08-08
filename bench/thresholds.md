# BBLBB — 性能阈值版本化（M16-PERF-08）

> 版本：v1。基线变化必须解释并由负责人（platform/performance）批准，并同步
> `reports/perf/baseline.md`。SLO 依据 2026-08-08 首测基线设定（机器：x86_64/16 核/40GiB）。

## SLO（v1 阈值）

| 指标 | 阈值 | 依据（实测基线） | 负责人 |
|---|---|---|---|
| 公开文章/板块/搜索 API p95 | ≤ 200 ms | 实测 16–18ms | platform/performance |
| 登录/发帖/回复 p95 | ≤ 300 ms | 实测 16–18ms（Argon2id+事务） | platform/performance |
| SSR 首页/板块/文章页 p95 | ≤ 500 ms | 实测 19–24ms | platform/performance |
| 全站无过滤帖子列表 p95（1M 行） | ≤ 2000 ms（优化目标：引入复合索引后 ≤ 200ms） | 实测 1207ms（已知慢查询 §5） | platform/performance |
| 峰值 RSS（后端） | ≤ 256 MB | 实测 35MB | platform/performance |
| DB 大小预算（perf 场景） | ≥ 256 MB 目标场景已达标（1137MB） | 实测 1137MB | platform/performance |
| WAL 大小（负载后） | ≤ 100 MB | 实测 1.7MB | platform/performance |
| 连接池 | max 8 默认；并发写压测需 ≥8 且 busy 计数不持续增长 | — | platform/performance |
| 磁盘余量 | ≥ 20 GiB 可用 | 实测 228GiB | platform/operations |
| 慢查询 | 超出 500ms 的查询必须登记并附 EXPLAIN 与负责人 | 实测仅 posts_list 一项 | platform/performance |
| 队列增长 | 无 SMTP/外部不可用时队列进入 retry_wait/dead，不无限增长 | 实测 50 job 全排空 | platform/quality-storage |

## 基线变更流程

1. 任何 SLO 阈值调整或新基线记录：更新 `reports/perf/baseline.md`（含机器/数据/
   命令/commit）+ 本表，注明变化原因。
2. 超出阈值的事件：登记到 `reports/rc/p0-p1.md`（性能项）+ `docs/OPERATIONS.md`
   告警；由 platform/performance 评估（索引/查询/缓存/预算），修复后更新基线。
3. 测量机器/DB 数据量变化必须新建基线小节（不得覆盖旧记录）。

## 当前登记项

- `GET /api/v1/posts`（无过滤）1M 行 p95=1207ms > 慢查询阈值 500ms：
  需复合索引 `(status, deleted_at, created_at)`（新迁移，M17 后优化项）；
  有板块过滤的常见路径不受影响。
