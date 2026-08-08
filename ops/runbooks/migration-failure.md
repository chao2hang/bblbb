# Runbook：迁移失败（migration failure）

> 严重级别：P0（发布阻断；可能数据风险）
> 执行人：release-engineering on-call；涉及回滚必须运维+安全双人确认。

## 症状

- `bblbb-migrate apply` 退出非零；日志 `migration failed`；
- `/readyz` 503，`checks.migrations=checksum_mismatch|ahead|behind|error`；
- 发布脚本 `release.sh` 停在「迁移」步骤。

## 处置

```sh
# 1) 只读诊断（不触碰数据库）
/opt/bblbb/current/backend/bblbb-migrate --check

# 2) 常见原因
#    - checksum mismatch：迁移文件被改动（禁止）或目录放错方言 → 对照
#      METADATA.json 校验 release bundle 的 migrations/ 与 DB 记录；
#    - ahead：DB 比代码新（发布顺序错误，先回滚代码版本）；
#    - behind：正常待应用；apply 失败说明具体 SQL 错误（外键/约束/缺列）。

# 3) 迁移失败处理（关键）
#    - 迁移在事务内执行，失败即回滚，DB 不会被标记成功（M01-DB-07）：
#      sqlite3 ... "SELECT COUNT(*) FROM schema_migrations;" 确认无半应用版本；
#    - 修复后重试 apply；不要手工改 schema_migrations 表；
#    - 若 SQL 语义有误：新增**不可变**补丁迁移（不得修改已发布迁移文件）。

# 4) 需要回滚（不可逆迁移见 docs/OPERATIONS.md §9 + M15-UPGRADE-03）
#    - 只有向后兼容迁移才能代码回滚；不可逆迁移必须备份恢复：
ops/restore/sqlite.sh <发布前备份> /var/lib/bblbb/database/bblbb.db --verify
#    - 回滚后校验：用户/账本/grant/outbox/audit + 迁移 checksum 一致。

# 5) 发布脚本内失败：release.sh 已停止切流并保留诊断（M15-UPGRADE-05）；
#    恢复 current 符号链接到上一版本或进入人工恢复。
```

## 恢复确认

- `bblbb-migrate --check` 一致；`/readyz` 200；
- 冒烟：`ops/smoke/smoke.sh`；
- 事故记录 + 版本化证据索引（`deploy/RELEASES.md`）。
