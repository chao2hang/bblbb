# Runbook：数据库不可用（DB unavailable）

> 严重级别：P1（核心论坛只读降级）→ P0（完全不可用）
> 执行人：on-call 值班（见 `oncall.md`）；升级阈值见文末。

## 症状

- `/readyz` 返回 503，`checks.database=error` 或 `not_configured`；
- 后端日志：`database pool created` 缺失 / `failed to create database pool`；
- 请求大量 5xx（`bblbb_http_errors_total` 上升，`bblbb_db_pool_size` 为 0）。

## 处置

```sh
# 1) 确认进程与池状态
systemctl status bblbb-backend bblbb-worker --no-pager
journalctl -u bblbb-backend -n 200 --no-pager | grep -iE "database|pool|error"

# 2) SQLite：检查文件系统与 WAL
df -h /var/lib/bblbb/database
sqlite3 /var/lib/bblbb/database/bblbb.db "PRAGMA integrity_check;"

# 3) MySQL/MariaDB：检查服务与网络
mysqladmin --host <host> --user <user> ping
# 连接池参数（BBLBB__DB_*）不合理时按 docs/CONFIGURATION.md 调整后重启

# 4) 若数据库文件损坏（SQLite integrity_check 非 ok）：
#    - 立即按备份恢复：ops/restore/sqlite.sh <最新备份> <db> --verify
#    - 恢复后启动并验证：curl -fsS http://127.0.0.1:8080/readyz

# 5) 数据库不可用期间：服务不会完全退出（无 DB 时以降级模式启动），
#    写路径返回 503/4xx。若预期停机>30 分钟，通知值班负责人并考虑切读副本。
```

## 升级

- 30 分钟内未恢复 → P0，通知值班负责人 + 运维经理；
- 需要恢复演练 → 走 `ops/runbooks/backup-failure.md` §4/§5（部分恢复/切流）。

## 恢复确认

- `/readyz` 200 且 `database=ok`；
- 冒烟：`ops/smoke/smoke.sh`；
- 账本抽查：`ops/restore/verify.sh`（Σ(delta)=balance）。
