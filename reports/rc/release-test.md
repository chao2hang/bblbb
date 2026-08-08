# BBLBB — RC 发布聚合报告（M16-RELEASE-TEST-02）

> 聚合 OpenAPI、Rust、前端、原型、三数据库、Playwright、axe、安全与性能报告。
> 命令与结果的完整日志见 `reports/rc/harness.md`；失败登记见 `failure-template.md`；
> P0/P1 见 `p0-p1.md`。

## 1. 报告清单

| 报告 | 结果 | artifact |
|---|---|---|
| OpenAPI/契约 | 193/193 ops；error codes 106/106；write-contract/route/permission/state-enum/event/TS 全绿 | `openapi/openapi.yaml` · `todo/openapi-operation-coverage.json` |
| Rust | fmt 0 diff · clippy -D warnings 0 · cargo test 全绿（含 4 个新 M16 二进制） · migration_equivalence 4 passed | `backend/` |
| 前端 | svelte-check 0/0 · vitest 86 files/567 passed · build 成功 | `frontend/` |
| 原型 | render + interaction checks passed | `prototype/` |
| Playwright | desktop+mobile 全量 + axe serious/critical=0 | `frontend/tests/a11y/axe-report.json` · `tests/a11y/records.json` |
| 三数据库 | SQLite 本地全绿 + CI mysql-family-migrations（MySQL 8/MariaDB 10.11 迁移 + 6 个 crossdb 测试）；实机执行为外部阻塞项（M16-HARNESS-02 `[!]`） | `.github/workflows/ci.yml` |
| 安全 | ASVS 基线映射、泄漏扫漏 PASS、scan-report（Secret OK / audit 4 项登记处置 / SBOM 634 组件） | `security/` |
| 性能 | 见 `reports/perf/baseline.md`（1M 行数据、p95、RSS 35MB、DB 1137MB） | `reports/perf/` |
| 演练 | 迁移升级 apply_ms=125/lock_events=0 · 备份恢复 RPO=0/RTO=0.18s · 冒烟 PASS=14 · 优雅停机 PASS=8 · bundle PASS=26 · alerts PASS=71 · 日志脱敏 CLEAN | `deploy/` · `ops/` |

## 2. 恢复后的 API smoke（M16-RELEASE-TEST-04 关联）

- 迁移升级演练（上一版本→当前）：`drill-migration-upgrade.sh` **PASSED**（apply_ms=125, lock_events=0, 57/57 checksum 一致）。
- SQLite 备份→擦除→恢复→内容校验：`drill-sqlite.sh` **PASSED**（RPO=0, RTO=0.18s）；`ops/restore/verify.sh` 对恢复库校验
  用户/账本恒等式/迁移 checksum/grant/outbox/audit 全绿。
- 恢复后 API smoke：`ops/smoke/smoke.sh` **PASS=14 FAIL=0**（db/登录/发帖/回复/附件/账本/管理权限门）。
- 上一版本生成 client 兼容：`check-client-compat.rb` **OK**（193/193 ops 向后兼容，`compat/frozen-client/` 为 M15 冻结契约）。

## 3. 环境备注（可复现性）

- macOS `/tmp` 是符号链接：本地存储适配器的符号链接防护会拒绝 `/tmp` 下的存储根。
  冒烟/性能/附件测试的存储目录使用非符号链接路径（如仓库内 `data/`）后全部通过；
  生产 Linux 无此行为。
- MySQL/MariaDB/S3/SMTP 实机演练为外部基础设施阻塞项（M16-HARNESS-02 /
  M16-STORAGE-FAULTS-01 `[!]`；M15-RUNBOOK-03 SMTP 既有阻塞）。

## 4. 发布门槛状态（TODO.md §9 映射）

| 门槛 | 状态 |
|---|---|
| P0/P1 全部完成/批准 | RC 无未关闭 P0；P1 性能项已登记审批（p0-p1.md） |
| 三数据库迁移/契约 | SQLite 绿 + CI 矩阵；实机由 `[!]` 跟踪 |
| OpenAPI 193/193 verified | 全 assigned/verified（sync-operation-coverage --check 绿） |
| Persona 权限 | 后端 persona 测试 + Playwright 全绿 |
| 隐藏内容防泄漏 | leak-sweep PASS（全渠道） |
| 不重复扣款/不可变流水 | economy/faults/step-injection 全绿 |
| S3 URL 生命周期 | adapter/billing 测试全绿（mock）；实机 `[!]` |
| AI 同意/不自动裁决 | ai tests 全绿 |
| 反爬行为检测 | antibot tests 全绿 |
| 主题/axe/键盘/移动/无 JS | Playwright + vitest 全绿 |
| 备份恢复证据 | drill RPO=0/RTO=0.18s + verify 全绿 |
| 可选能力关闭时核心可用 | feature_flags + defaults（AI/Video/DownloadBilling/OIDC/Marketplace 默认关闭） |
| 发布地区/法律 | M17-LEGAL 阶段（todo/M13-M17-release.md#m17） |
| RC 报告完整 | 本文档 + harness.md + p0-p1.md + failure-template.md + checklist.md |
