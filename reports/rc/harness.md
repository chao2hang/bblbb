# BBLBB — M16 测试基线与报告（M16-HARNESS-08）

> 本报告记录测试日志、DB 版本、迁移 checksum、commit 与 artifact 地址，
> 供 RC 聚合（`reports/rc/release-test.md`）引用。全部为真实命令输出。

## 环境

| 项 | 值 |
|---|---|
| commit | `e4faf25658ac135a5c4c90991d7aee4deb3064c3`（本里程碑最终 commit 见 `git rev-parse HEAD` 回填） |
| 机器 | x86_64 / 16 核 / 40GiB（`reports/perf/machine.md`） |
| Rust | 1.97.1（rust-toolchain.toml） |
| SQLite | 3.51.0（2025-06-12） |
| Node | v26.5.1 |
| Ruby | 2.6.10 |

## 契约与治理（exit 0）

```text
$ ruby scripts/sync-operation-coverage.rb --check
OpenAPI coverage OK: 193/193 operations assigned

$ ruby scripts/check-roadmap.rb
Roadmap OK: 783 unique leaf tasks across 87 work packages
OpenAPI coverage: 193/193 operations assigned

$ ruby scripts/check-openapi.rb
OpenAPI OK: 193 operations, unique operationIds, internal $refs resolve, schemas structurally sound

$ ruby scripts/check-error-codes.rb
Error codes OK: 106 documented codes, 106 enumerated in OpenAPI, no missing/spelling/deprecation diffs

$ ruby scripts/check-code-fixtures.rb
Code fixtures OK: 106 stable codes（backend Fixture + frontend 映射 四方一致）

$ ruby scripts/check-state-machine-matrix.rb
State-machine matrix OK: 引用的状态机迁移测试文件与函数全部存在

$ ruby scripts/check-client-compat.rb
Client compat OK: frozen=193 ops, current=193 ops（上一版本 client 向后兼容）

$ ruby scripts/check-write-contract.rb
write-contract OK: all write operations declare their required contract
$ ruby scripts/check-route-coverage.rb / check-permission-matrix.rb / check-state-enums.rb / check-event-catalog.rb
全部 OK（route 覆盖 / 40 个 x-permission / 28 枚举 / 23 事件）
```

## 后端（最终全量）

```text
$ cd backend && cargo fmt --all -- --check        # 0 diff
$ cargo clippy --workspace --all-targets --all-features -- -D warnings   # 0 警告
$ cargo test --all-features                        # 全部二进制 0 failed
$ cargo test --test migration_equivalence          # 4 passed
新增 M16 测试二进制：harness_contract（5）· storage_adapter（15）· faults（3）· economy_step_injection（5）
```

## 前端

```text
$ cd frontend && npm run check     # svelte-check 0 errors 0 warnings
$ npm run test                     # 86 files, 567 tests passed
$ npm run build                    # adapter-node build 成功
$ npx playwright test              # desktop+mobile 194 用例（axe serious/critical=0，见 tests/a11y/axe-report.json）
```

## 原型

```text
$ make check-prototype
render checks: passed；interaction checks: passed（admin routes 22 + core flows）
```

## 迁移与 DB

```text
$ make migrate-check-sqlite    # 空库应用 57 个迁移，PRAGMA foreign_key_check 空
$ bash deploy/scripts/drill-migration-upgrade.sh
MIGRATION-DRILL: PASSED (apply_ms=125, lock_events=0)，迁移版本数 57/57，checksum 一致
$ sqlite3 data/perf-bench.sqlite 'PRAGMA integrity_check;' → ok
迁移 checksum：由 verify.sh 逐文件 SHA-256 与 schema_migrations 比对一致（reports/rc/release-test.md §2）
```

## Artifact 地址

| artifact | 路径 |
|---|---|
| 状态机矩阵 | `reports/rc/state-machine-coverage.md` |
| 性能基线 | `reports/perf/baseline.md` · `reports/perf/machine.md` |
| 安全映射 | `security/ASVS-BASELINE.md` · `security/leak-sweep.md` |
| 扫描记录 | `security/scan-report.md` · `security/sbom-*.json` |
| 合成数据 | `data/perf-bench.sqlite`（1137MB） |
| CI 分层 | `docs/CI-LAYERS.md` · `.github/workflows/{ci,nightly,release-rc}.yml` |
| E2E 记录 | `frontend/tests/a11y/axe-report.json` · `frontend/tests/a11y/records.json` |

## 复现

```sh
make check && make test          # 根门禁
cd backend && cargo test --all-features
cd frontend && npm run test && npm run check && npm run build
ruby scripts/check-roadmap.rb && ruby scripts/sync-operation-coverage.rb --check
```
