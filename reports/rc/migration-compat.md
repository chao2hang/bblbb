# BBLBB — 迁移兼容、升级时长与不可逆步骤说明（M17-FREEZE-04）

> 执行：platform/release-manager；日期：2026-08-08。

## 1. 迁移兼容性

- 全部迁移 `0001..0058` 不可变（checksum 保护，`/readyz` 检测 checksum_mismatch）。
- 三方言结构等价：`cargo test --test migration_equivalence` 4 passed（sqlite/mysql/mariadb）。
- 上一版本→当前版本升级：`ops/backup/drill-*.log` 与 M16 记录 `apply_ms=125, lock_events=0`；
  重复应用幂等（第二次 apply 0 行变更）。
- 追加迁移（0056 marketplace / 0057 theme / 0058 users cover meta）均为只增，兼容回滚。

## 2. 升级时长与锁（本机实测）

- 空库安装：全量 58 个迁移 < 1s（SQLite）。
- 上一版本（0055 截点）→ 当前：apply_ms=125，lock_events=0。
- 生产建议维护窗口 ≥ 15 分钟（含备份 + 迁移 + 冒烟 + 回滚预案）。

## 3. 不可逆步骤

- 本版本无删除列/破坏性数据迁移；历史 migration 不可修改（变更控制 §10）。
- 若未来引入破坏性迁移：必须先备份、标注不可回滚、提供恢复点（restore point）
  与前置备份，禁止绕过。

## 4. 恢复点

- 每次发布脚本先执行 `ops/backup/sqlite.sh`（RPO=0，WAL checkpoint 快照；
  实测备份 134ms / 恢复 RTO=0.18s）。
- `/opt/bblbb/releases/<version>` + `current` symlink 切换；失败保留诊断并恢复
  current symlink（见 `deploy/scripts/release.sh` 与 M15-RUNBOOK）。
