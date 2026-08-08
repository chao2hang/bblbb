# BBLBB — CI 分层（M16-HARNESS-09 / M16-RELEASE-TEST-01）

> PR 快速检查与发布长测试分离；每一层失败输出最小复现命令（job 步骤即复现命令）。

## 四层 CI

| 层 | 触发 | 超时 | 内容 | 失败即 |
|---|---|---|---|---|
| **PR（pull_request / push）** | `.github/workflows/ci.yml` | contracts 15m · rust 30m · frontend 20m · migrations 15m | 契约治理（OpenAPI 覆盖/错误码/写契约/路由覆盖/权限矩阵/状态枚举/事件目录/TS 类型/路线图/链接）；Rust fmt+clippy -D warnings+test；前端 format/lint/check/vitest/build；SQLite 迁移 + FK；MySQL 8/MariaDB 10.11 迁移 + crossdb 语义测试；原型 check | 阻断合并 |
| **nightly** | `.github/workflows/nightly.yml`（`schedule: cron 0 2 * * *` + workflow_dispatch） | 全层 60m | PR 全量 + Playwright desktop/mobile + axe、错误码/状态机/客户端兼容矩阵脚本、故障注入回归（storage adapter/faults/economy step-injection）、`cargo audit`（若可用） | 次日告警；不阻断合并 |
| **RC** | `.github/workflows/release-rc.yml`（workflow_dispatch，RC 候选人标记） | 全层 90m | PR + nightly 全量 + release bundle 测试（`deploy/tests/test-release-bundle.sh`）、恢复演练（`ops/restore/verify.sh` + attachments + OIDC keys）、冒烟 `ops/smoke/smoke.sh`、告警表推、日志脱敏扫描 | 阻断 RC 声明 |
| **production smoke** | 发布流程内 `deploy/scripts/release.sh` + `ops/smoke/smoke.sh` | 15m | 真实部署后 DB/登录/发帖/回复/附件/账本/管理 API 冒烟 | 回滚触发点 |

## 最小复现命令

每层失败时，负责人按该步骤的原始命令本地复现：

```sh
# 契约
ruby scripts/sync-operation-coverage.rb --check
ruby scripts/check-roadmap.rb
ruby scripts/check-error-codes.rb && ruby scripts/check-code-fixtures.rb
ruby scripts/check-state-machine-matrix.rb
ruby scripts/check-client-compat.rb
ruby scripts/check-openapi.rb && ruby scripts/check-write-contract.rb
ruby scripts/check-route-coverage.rb && ruby scripts/check-permission-matrix.rb
ruby scripts/check-state-enums.rb && ruby scripts/check-event-catalog.rb
ruby scripts/generate-ts-types.rb --check
# Rust
cd backend && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features -- -D warnings
cd backend && cargo test --all-features && cargo test --test migration_equivalence
# 前端
cd frontend && npm run check && npm run test && npm run build
# 原型
cd prototype && npm run check:all
# 迁移
make migrate-check-sqlite
# 演练
bash deploy/tests/test-release-bundle.sh && bash ops/smoke/smoke.sh
```

失败报告模板见 `reports/rc/failure-template.md`；报告聚合见 `reports/rc/harness.md`
与 `reports/rc/release-test.md`。
