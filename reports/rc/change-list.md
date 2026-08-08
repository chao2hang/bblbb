# BBLBB — RC 变更清单（M17-FREEZE-01）

> 用途：从冻结基线到 v1.0.0-rc.2 的 API/schema/state/permission/privacy 差异清单，
> 逐项标注文档同步状态与证据。冻结基线 = `docs/PRODUCT-DECISIONS.md` +
> `docs/REQUIREMENTS.md` v0.5（2026-08-04 产品所有者确认）与
> `compat/frozen-client/openapi.yaml`（M15 commit `468883e` 冻结契约）。
> 复核人：platform/release-manager；日期：2026-08-08。

## 0. 结论摘要

- 契约基线冻结为 **193 operations**，当前 `openapi/openapi.yaml` 仍为 193，
  `ruby scripts/sync-operation-coverage.rb --check` 与 `check-openapi.rb` 均 exit 0。
- 上一版本生成 client 向后兼容：`check-client-compat.rb`（193/193）OK。
- 无未批准的破坏性变更；冻结后所有差异都属于「只增不破」类别
  （新增 operation/字段/枚举/错误码/权限），且均同步了 Requirements/OpenAPI/
  Schema/Security/Testing 与专项文档（见 `doc-sync.md`）。
- Schema：迁移 `1..57` 不可变（checksum 保护），三方言结构等价
  （`cargo test --test migration_equivalence` 4 passed）。
- State：23 个状态机全部进入 `reports/rc/state-machine-coverage.md` 矩阵，
  `check-state-machine-matrix.rb` OK。
- Permission：68 项 `PERMISSION_REGISTRY` ↔ OpenAPI(38 real) ↔
  `PERMISSION-MATRIX.md` 附录注册表三方一致（`check-permission-matrix.rb` OK）。
- Privacy：隐藏内容防泄漏 16 渠道扫漏 PASS（`security/leak-sweep.md`）；
  日志脱敏扫描 CLEAN（`ops/scan-log-corpus.sh --test` PASSED）。

## 1. API 差异（冻结基线 → 当前）

| 维度 | 冻结基线 | 当前 rc.2 | 差异 | 文档同步 |
|---|---|---|---|---|
| operation 总数 | 193（`compat/frozen-client/`） | 193（`openapi/openapi.yaml`） | 无增删；全部 assigned/verified | OpenAPI、API.md、API-CONTRACTS.md、ERROR-CODES.md、TS 类型同步（`generate-ts-types.rb --check` 可复现） |
| 稳定错误码 | 106 码（M16 四方一致） | 106 码 | 无漂移 | `check-error-codes.rb`（106/106）+ `check-code-fixtures.rb`（106 稳定码 fixture） |
| 方法/路径 | 193 条 | 193 条 | 无删除/重命名 | `check-client-compat.rb`（操作表面全兼容） |
| 非契约端点 | `/sitemap.xml`（feeds.rs 登记） | 不变 | 无 | 路由覆盖登记（`check-route-coverage.rb`） |

## 2. Schema 差异（数据/迁移）

| 维度 | 冻结基线 | 当前 rc.2 | 差异 | 文档同步 |
|---|---|---|---|---|
| 迁移数量 | v1 首发无线上版本（M15 规划 1..57） | 57 迁移（`schema_migrations` checksum 保护） | 均为新交付，无已执行迁移被修改 | `SCHEMA.md`、`deploy/RELEASES.md`、`docs/OPERATIONS.md` §19.4 |
| 三方言等价 | 未冻结 | sqlite/mysql/mariadb `0001..0057` 结构等价 | `migration_equivalence` 4 passed | `SCHEMA.md` + 迁移文件 |
| 不可逆步骤 | — | 0057_theme 纯增量可逆；其余按发布矩阵判定（见 `migration-compat.md`） | 无不可逆迁移 | `migration-compat.md`、`OPERATIONS.md` §19.4 |

## 3. State 差异（状态机）

| 维度 | 冻结基线 | 当前 rc.2 | 差异 | 文档同步 |
|---|---|---|---|---|
| 状态机矩阵 | 未冻结 | 23 个状态机合法/非法迁移矩阵 | 全部有对应测试函数 | `reports/rc/state-machine-coverage.md` + `check-state-machine-matrix.rb` |
| 状态枚举 | 文档 v0.5 | 与 OpenAPI/后端/前端枚举一致 | 无漂移 | `check-state-enums.rb` |
| 事件目录 | v0.5 | 23/23 事件与 payload version 一致 | 无漂移 | `check-event-catalog.rb` |

## 4. Permission 差异

| 维度 | 冻结基线 | 当前 rc.2 | 差异 | 文档同步 |
|---|---|---|---|---|
| 权限注册表 | v0.5 矩阵 | 68 `PERMISSION_REGISTRY` 项 | 与 OpenAPI/文档三方一致 | `check-permission-matrix.rb` OK |
| Persona 执行 | 未冻结 | anonymous/unverified/cooldown/member/moderator/admin/mute/banned/restricted 后端测试 + Playwright 全绿 | 服务端裁决为准 | `authz_persona.rs`、`flows-*.spec.ts`、`PERMISSION-MATRIX.md` |
| 管理操作 | 未冻结 | reason + recent-auth + If-Match + 审计强制 | 无绕过 | `admin_routes.rs`、`SECURITY.md` |

## 5. Privacy 差异

| 维度 | 冻结基线 | 当前 rc.2 | 差异 | 文档同步 |
|---|---|---|---|---|
| 隐藏内容防泄漏 | 未冻结 | 16 渠道扫漏 PASS（API/SSR/DOM/hydration/搜索/RSS/SEO/通知/日志/AI/缓存/附件） | 无泄漏 | `security/leak-sweep.md`、`SECURITY.md` §18 |
| 日志脱敏 | v0.5 规则 | 字段名+值级 redaction + corpus 扫描 CLEAN | 无完整邮箱/Token/签名 URL | `OPERATIONS.md` §19.2、`ops/scan-log-corpus.sh` |
| 注销/保留 | v0.5 规则 | 匿名化保留讨论、30 天延迟可撤销、legal_hold 暂停删除 | 实现与说明一致 | `RETENTION-PRIVACY.md`、`deletion_lifecycle.rs` |
| AI 同意 | v0.5 规则 | 逐次同意、撤回阻断、Provider 无自动裁决 | 实现与说明一致 | `AI.md`、`ai/gateway.rs`、`ai/tasks.rs` |

## 6. 待人工评审项（不阻塞 RC 冻结，阻塞正式上线）

- 本变更清单与 `review-meeting.md` 的产品/后端/前端/安全/测试/运维/运营评审
  尚待各负责人签字；未签署前不得执行正式上线（M17-FREEZE-06 `[!]`）。
- M17-LEGAL（法律/运营）、M17-LAUNCH（真实生产执行）为独立 `[!]` 阻塞项。
