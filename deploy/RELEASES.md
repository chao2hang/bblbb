# BBLBB 发布记录与版本化证据索引（M15-UPGRADE-01/08）

> 每次发布在此登记：版本、commit、迁移兼容性、API 兼容性、前后端发布顺序、
> 备份/回滚记录、checksum。证据由 `deploy/scripts/record-release-metadata.sh`
> 生成。

## 发布矩阵（M15-UPGRADE-01）

| Release | 迁移兼容性 | API 兼容性 | 前后端发布顺序 | 回滚 | 状态 |
|---|---|---|---|---|---|
| v1.0.0-rc.2 | 自 v0 起始（无历史线上版本）；迁移 `1..57`，不可变（checksum 保护）；0057_theme 为纯增量（建表/插入），可逆 | v1 首发：无旧客户端；OpenAPI 193 ops 全量交付 | 首发：后端迁移 → backend → worker → frontend；后端先于前端（前端依赖新 API） | 无旧版本可回退；代码回滚仅当数据库兼容；不可逆迁移必须备份恢复 | 规划中 |

## 版本化证据索引（M15-UPGRADE-08）

| 版本 | commit | bundle | sha256 | 迁移 drill | 备份恢复 drill | 冒烟 | 部署/回滚记录 |
|---|---|---|---|---|---|---|---|
| v1.0.0-rc.2 | 468883e | `dist/<version>.tar.gz` | 待填 | `deploy/scripts/drill-migration-upgrade.sh`（apply_ms=68, lock_events=0，见 M15-UPGRADE-02 证据） | `ops/backup/drill-sqlite.sh`（RPO=0, RTO=0.18s，见 M15-BACKUP-06 证据） | `ops/smoke/smoke.sh`（PASS=14，见 M15-UPGRADE-07 证据） | 待填 |

> 回填规则：发布执行后在 `docs/CHANGELOG.md` 与本文登记 commit/checksum/部署
> 时间与回滚记录；任何发布必须先在副本库通过 `drill-migration-upgrade.sh`。

## 每版本 release note 模板（M15-UPGRADE-01/03）

每个 release note 必须包含：

1. **迁移兼容性**：新增迁移列表与影响；`reversible`（可逆）/`irreversible`
   （不可逆——禁止代码回滚，只允许备份恢复并接受数据窗口损失）；
2. **API compatibility**：新增/变更 operation（`docs/API-COMPATIBILITY.md`）；
   旧客户端兼容性声明；
3. **前后端发布顺序**：后端先行，前端随后；不兼容变化标注所需窗口；
4. **回滚**：restore point（发布前备份位置）、回滚命令、数据窗口说明。
