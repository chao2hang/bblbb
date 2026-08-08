# BBLBB — 差异文档同步验证（M17-FREEZE-02）

> 用途：对 `change-list.md` 中每一项差异，验证 Requirements、OpenAPI、Schema、
> Security、Testing 和专项文档均同步更新。所有命令在 2026-08-08 于仓库根目录
> 重跑并记录输出（干净环境可复现）。
> 执行：platform/release-manager；复核：待 M17-FREEZE-06 评审签字。

## 1. 自动治理脚本全绿（文档↔契约↔代码↔测试机械一致）

| 脚本 | 断言内容 | 结果（2026-08-08 重跑） |
|---|---|---|
| `ruby scripts/sync-operation-coverage.rb --check` | OpenAPI 193/193 operation assigned，coverage manifest 与契约同步 | `OpenAPI coverage OK: 193/193 operations assigned` exit 0 |
| `ruby scripts/check-openapi.rb` | operationId 唯一、内部 $ref 解析、schemas 结构、tags/security/x-permission/x-csrf/responses 必填 | `OpenAPI OK: 193 operations…` exit 0 |
| `ruby scripts/check-error-codes.rb` | ERROR-CODES.md ↔ OpenAPI 106 码无漂移 | `Error codes OK: 106 documented codes…` exit 0 |
| `ruby scripts/check-code-fixtures.rb` | 每个稳定码有后端 Fixture + 前端映射 | `Code fixtures OK: 106 stable codes` exit 0 |
| `ruby scripts/check-client-compat.rb` | 上一版本 client 向后兼容 | 193/193 OK |
| `ruby scripts/check-write-contract.rb` | 写操作契约声明 | OK |
| `ruby scripts/check-route-coverage.rb` | 路由覆盖登记（含非契约端点） | OK |
| `ruby scripts/check-permission-matrix.rb` | 权限注册表三方一致 | OK |
| `ruby scripts/check-state-enums.rb` | 状态枚举一致 | OK |
| `ruby scripts/check-state-machine-matrix.rb` | 状态机迁移矩阵引用存在 | `State-machine matrix OK` |
| `ruby scripts/check-event-catalog.rb` | 事件目录 23/23 一致 | OK |
| `ruby scripts/check-html-sinks.rb` | 前端无任意 HTML sink | OK |
| `ruby scripts/check-secrets.rb` | 无 Secret 模式 | OK |
| `ruby scripts/generate-ts-types.rb --check` | 冻结 TS 类型可复现 | OK |

## 2. 文档族逐项核对

| 文档族 | 冻结基线 | 更新证据（变更对应实现） |
|---|---|---|
| Requirements / 产品决策 | `REQUIREMENTS.md` v0.5、`PRODUCT-DECISIONS.md` | 无需求语义变更；可选能力默认关闭策略记录于 `docs/CONFIGURATION.md` §1.5 与 `ops/feature-flags/`（M17-FLAGS） |
| OpenAPI | `openapi/openapi.yaml` 193 ops | `compat/frozen-client/openapi.yaml`（M15 冻结）与当前一致（193/193 兼容）；`API.md`/`API-CONTRACTS.md`/`ERROR-CODES.md` 同步 |
| Schema | `SCHEMA.md` 逻辑模型 | 0057_theme（M13）同步 §15；三方言迁移等价（`migration_equivalence` 4 passed） |
| Security | `SECURITY.md` 威胁模型/基线 | ASVS 基线映射 `security/ASVS-BASELINE.md`；泄漏扫漏 PASS；scan-report 处置登记 |
| Testing | `TESTING.md` Release gate | `docs/FIXTURES.md`、`docs/CI-LAYERS.md`、`reports/rc/*`（M16-RELEASE-TEST 全套） |
| 专项文档 | STORAGE/DOWNLOAD-BILLING/AI/VIDEO-PLUGIN/AUTH-OIDC/MARKETPLACE/MARKETPLACE-ACCOUNTING/CRAWLER-POLICY/THEME/PLUGIN | 各领域测试证据引用专项文档；`DOCUMENT-STATUS.md` 发布矩阵逐行对应 |
| 运维文档 | OPERATIONS.md / CONFIGURATION.md | M15 部署/备份/升级/Runbook 章节；M17 追加 `deploy/staging/`、`ops/feature-flags/` |

## 3. 结论

- 14 项自动治理脚本全部 exit 0；未发现 Requirements/OpenAPI/Schema/Security/
  Testing/专项文档的未同步差异。
- 文档基线 `docs/DOCUMENT-STATUS.md` 标记所有实现/发布档案为 current（M17 收口
  时同步更新，见 commit 证据）。
