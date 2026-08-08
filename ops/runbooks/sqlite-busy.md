# Runbook：SQLite busy / 锁竞争 / WAL 过大

> 严重级别：P1（写路径延迟上升）→ P0（写路径持续失败）
> 执行人：on-call 值班。

## 症状

- 指标 `bblbb_sqlite_busy_total` 快速增长；
- 日志 `sqlite busy, backing off exponentially` / `database is locked`；
- 写路径 503 `storage_busy` / 超时；`/readyz` 正常但写操作慢。

## 处置

```sh
# 1) 确认 WAL 大小与 checkpoint 状态
ls -lh /var/lib/bblbb/database/bblbb.db*
sqlite3 /var/lib/bblbb/database/bblbb.db "PRAGMA wal_checkpoint(PASSIVE);"

# 2) 定位长事务/慢查询
#    慢查询已由 backend 记录（BBLBB__DB_SLOW_QUERY_MS 阈值，tracing warn，
#    带 label + elapsed_ms，无 SQL 明文）。
journalctl -u bblbb-backend -n 500 --no-pager | grep "slow query"

# 3) 常见原因
#    - 写并发过高：SQLite 单写者；减少 BBLBB__DB_MAX_CONNECTIONS 写并发，
#      或将批量任务（搜索索引/重渲染/删除）分流到 worker 错峰；
#    - 长事务持锁：检查 AI/视频/邮件任务是否在事务内做 IO（被 check-tx-io 禁止，
#      若出现说明回归）；
#    - 备份/迁移/checkpoint 竞争：备份脚本用 BEGIN IMMEDIATE + checkpoint，
#      错开备份窗口与高峰。

# 4) WAL 过大（> 主库 2 倍且持续增长）
sqlite3 /var/lib/bblbb/database/bblbb.db "PRAGMA wal_checkpoint(TRUNCATE);"
#    若 checkpoint 反复失败（busy）：在维护窗口停写后 TRUNCATE。
#    长期：调低 checkpoint 阈值或在空闲时段安排周期 checkpoint（配合每日备份）。

# 5) 持续锁竞争（P0）
#    维护窗口停服 → wal_checkpoint(TRUNCATE) → 启动 → 观察 busy 指标回落。
```

## 升级

- busy 指标 > 阈值（如 100/分钟）持续 15 分钟 → P1 通知；
- 写路径错误率 > 5% 持续 10 分钟 → P0。

## 恢复确认

- `bblbb_sqlite_busy_total` 增速回落；写请求 p95 正常；
- 冒烟：`ops/smoke/smoke.sh`。
