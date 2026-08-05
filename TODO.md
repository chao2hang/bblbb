# BBLBB v1.0 正式上线执行路线图

> 路线图版本：v1.0.0-rc.2
> 更新日期：2026-08-05
> 产品基线：Frozen v0.5；产品所有者已确认。
> 当前阶段：**M3 资料/RBAC/板块/标签（进行中）**；下一任务：[`M03-BOARDS-07`](todo/M03-M05-community.md#m3)。
> 目标：从已交付的规格、OpenAPI、Rust/SvelteKit/迁移/CI 骨架，推进到可正式上线并可恢复的 v1.0。

## 1. 结论与使用方式

本路线图现在分成两层，避免旧版中“里程碑 checkbox”和“NOW checkbox”重复计数：

1. **本文件是唯一仪表盘**：记录状态、依赖、覆盖率、当前批次和上线门槛，不重复列叶子任务。
2. **`todo/` 是唯一执行清单**：共 87 个工作包、783 个唯一叶子任务；每项带 15-60 分钟估时，工作包带优先级、owner、风险、依赖、目标文件和验收方式。

任何实现工作必须从对应执行册选择一项叶子任务。不能通过修改本页的汇总数字来代替完成任务。

## 2. 状态、优先级与证据规则

### 2.1 状态

- `[ ]`：未开始。
- `[~]`：进行中；全仓同一时刻只允许一个叶子任务为此状态。
- `[x]`：已完成；任务行必须追加完整 `证据：`。
- `[!]`：阻塞；任务行必须追加 `阻塞：原因；负责人；复查日期；解除条件`。

工作包、里程碑和总进度由叶子任务推导，不另设可勾选的重复父任务：

- `未开始`：全部叶子任务为 `[ ]`。
- `进行中`：至少一项 `[~]` 或部分 `[x]`，且仍有未完成项。
- `阻塞`：存在 `[!]`。
- `完成`：所有叶子任务均为 `[x]` 且出口门槛通过。

### 2.2 优先级

- `P0`：安全、权限、数据一致性、隐私、恢复或发布阻断项；未完成不得上线。
- `P1`：冻结 v1.0 功能或质量能力；未完成不得声明完整 v1.0。
- `P2`：默认关闭能力的启用动作或非阻断优化；只有在对应 P0/P1 实现和专项门槛完成后才可执行。P2 默认关闭不能掩盖其 v1.0 实现任务未完成。

### 2.3 Owner、风险和阻塞

- 工作包默认使用 `owner=unassigned/<role>`；开始前必须替换为实际负责人。
- `risk=critical/high/medium/low` 表示影响，不表示优先级。
- 发现前置契约缺失、跨库语义未冻结或安全方案不可验证时，立即标记 `[!]`，不要先写临时代码绕过。
- 负责人、阻塞和完成状态必须写入 Git，可追踪到 commit/PR；不依赖聊天记录。

### 2.4 完成证据

每个 `[x]` 任务行末必须追加：

```text
证据：files=<实现/迁移/文档>；commands=<实际命令与结果摘要>；
contract=<OpenAPI/权限/错误码/状态机/事件影响或 none>；
commit=<hash 或 PR>；review=<评审人/日期或 none>
```

完成定义同时要求：

1. 实现和不可变迁移存在；
2. 单元/仓储/HTTP/浏览器/专项测试按任务风险通过；
3. SQLite、MySQL 8、MariaDB 10.11 的适用契约一致；
4. OpenAPI、权限、错误码、状态机、事件、Schema 和文档同步；
5. 失败、重试、幂等、审计、隐私、缓存和回滚行为已验证；
6. 证据能在干净环境复现。

## 3. ADHD 友好的执行协议

每次只推进一个叶子任务：

1. **0-5 分钟：** 分配 owner，把任务改为 `[~]`，打开目标文件，运行最小失败测试。
2. **5-25 分钟：** 只做该任务，不顺手重构其他领域。
3. **25-30 分钟：** 休息 5 分钟；记录具体进展或阻塞。
4. **30-50 分钟：** 完成实现、测试和文档。
5. **50-60 分钟：** 运行验收、补证据、标记 `[x]`；若仍未完成，先拆成更小任务。

一个小胜利的标准是“一项叶子任务有可复现证据”，而不是一次完成整个工作包。

## 4. 已交付基线（不等于 v1 功能完成）

| ID | 状态 | 已交付内容 | 证据 |
|---|---|---|---|
| `BASE-001` | 已完成 | 产品需求、关键决策和文档事实来源冻结 | `docs/REQUIREMENTS.md`、`docs/PRODUCT-DECISIONS.md`、`docs/DOCUMENT-STATUS.md`；commit `5e17fa3` |
| `BASE-002` | 已完成 | OpenAPI 3.1 契约：133 paths、172 operations、172 唯一 operationId | `openapi/openapi.yaml`；本次 Ruby 校验 172/172 且必需扩展完整；commit `5e17fa3` |
| `BASE-003` | 已完成 | Rust/axum 可运行骨架、`/healthz`、请求 ID、Problem 边界和 OpenAPI JSON | `backend/`；`cargo fmt`、Clippy、4 tests 通过；commit `5e17fa3` |
| `BASE-004` | 已完成 | SvelteKit 2/Svelte 5/adapter-node 骨架和同源健康 API client | `frontend/`；`npm ci && npm run check && npm run build` 通过；commit `5e17fa3` |
| `BASE-005` | 已完成 | SQLite/MySQL/MariaDB 初始 users/session 骨架迁移 | `migrations/*/0001_skeleton.sql`；本次 SQLite 空库应用和 foreign key check 通过；commit `5e17fa3` |
| `BASE-006` | 已完成 | GitHub Actions 的文档、Rust、前端、原型和三数据库基础 CI | `.github/workflows/ci.yml`；commit `5e17fa3`；后续仍须加入完整契约/安全/发布门槛 |
| `BASE-007` | 已完成 | 66 路由高保真原型和 22 条后台路由 | `prototype/`；render 66/66、interaction 通过；golden 视觉差异尚未冻结，由 `M16-RELEASE-TEST-06` 跟踪 |
| `BASE-008` | 已完成 | rc.2 执行册、OpenAPI 逐项登记和路线图校验器 | `todo/`、`scripts/sync-operation-coverage.rb`、`scripts/check-roadmap.rb`；2026-08-04：Ruby 2.6 语法检查通过，路线图 87/783、依赖无环、仪表盘/53 个本地链接有效，OpenAPI 172/172 同步检查通过 |

基线只证明“正式开发入口存在”，不表示认证、论坛、附件、账本或其他业务已经实现。`getHealth` 在 API 覆盖登记中标记为 `baseline_only`，其余 operation 默认 `not_started`。

## 5. 进度仪表盘

> 当前静态快照由 rc.2 建立时统计；每次合并任务状态变更必须运行 `ruby scripts/check-roadmap.rb` 并同步本表。数字不能领先于执行册证据。

| 里程碑 | 范围 | 工作包 | 叶子任务 | P0 / P1 / P2 | 当前状态 | 入口 |
|---|---|---:|---:|---:|---|---|
| M0 | 工具链、契约、后端/前端边界 | 4 | 42 | 33 / 9 / 0 | 完成 | [`M00-M02`](todo/M00-M02-foundation.md#m0) |
| M1 | 三数据库、配置、Job/Outbox、审计 | 4 | 43 | 43 / 0 / 0 | 完成 | [`M00-M02`](todo/M00-M02-foundation.md#m1) |
| M2 | 注册、邮箱验证、Session/CSRF、MFA | 4 | 44 | 34 / 10 / 0 | 阻塞 | [`M00-M02`](todo/M00-M02-foundation.md#m2) |
| M3 | 资料、RBAC、板块、标签、搜索仓储 | 6 | 56 | 28 / 28 / 0 | 进行中 | [`M03-M05`](todo/M03-M05-community.md#m3) |
| M4 | Markdown、内容、回复、可见性 | 6 | 61 | 33 / 28 / 0 | 未开始 | [`M03-M05`](todo/M03-M05-community.md#m4) |
| M5 | 风险审核、举报、处罚、申诉、通知 | 7 | 63 | 37 / 26 / 0 | 未开始 | [`M03-M05`](todo/M03-M05-community.md#m5) |
| M6 | Local/S3、配额、下载抵扣、迁移 | 7 | 65 | 50 / 15 / 0 | 未开始 | [`M06-M07`](todo/M06-M07-storage-economy.md#m6) |
| M7 | 账本、等级、签到、商城、装扮 | 5 | 46 | 18 / 28 / 0 | 未开始 | [`M06-M07`](todo/M06-M07-storage-economy.md#m7) |
| M8 | 搜索、Feed、SEO、反爬 | 4 | 36 | 12 / 24 / 0 | 未开始 | [`M08-M12`](todo/M08-M12-integrations.md#m8) |
| M9 | AI Gateway、同意、任务、建议 | 5 | 39 | 24 / 15 / 0 | 未开始 | [`M08-M12`](todo/M08-M12-integrations.md#m9) |
| M10 | Direct/HLS/Xigua 视频 | 2 | 18 | 12 / 6 / 0 | 未开始 | [`M08-M12`](todo/M08-M12-integrations.md#m10) |
| M11 | OIDC Provider | 3 | 27 | 27 / 0 / 0 | 未开始 | [`M08-M12`](todo/M08-M12-integrations.md#m11) |
| M12 | 第三方 Marketplace 原子账务 | 5 | 43 | 36 / 7 / 0 | 未开始 | [`M08-M12`](todo/M08-M12-integrations.md#m12) |
| M13 | 主题、插件、管理后台 | 4 | 34 | 18 / 16 / 0 | 未开始 | [`M13-M17`](todo/M13-M17-release.md#m13) |
| M14 | 全量前端、a11y、无 JS、SEO | 4 | 32 | 10 / 22 / 0 | 未开始 | [`M13-M17`](todo/M13-M17-release.md#m14) |
| M15 | 部署、观测、备份、恢复、升级 | 5 | 42 | 42 / 0 / 0 | 未开始 | [`M13-M17`](todo/M13-M17-release.md#m15) |
| M16 | 契约、安全、故障、性能、RC 测试 | 6 | 48 | 48 / 0 / 0 | 未开始 | [`M13-M17`](todo/M13-M17-release.md#m16) |
| M17 | 冻结、预发布、专项 Gate、上线 | 6 | 44 | 39 / 0 / 5 | 未开始 | [`M13-M17`](todo/M13-M17-release.md#m17) |
| **总计** |  | **87** | **783** | **544 / 234 / 5** | **163 完成 / 1 进行中 / 2 阻塞 / 617 未开始** |  |

5 个 P2 任务只负责在生产中实际开启 Download Billing、AI、Video、OIDC 和 Marketplace。对应实现、安全和专项门槛仍属于 P0/P1，不能用“保持关闭”掩盖实现未完成。若 v1.0 首发继续关闭某项 P2，必须记录负责人、原因、观察条件和后续启用计划。

## 6. 执行依赖与推荐顺序

### 6.1 主关键路径

```text
M0 工程治理
  → M1 数据库/配置/Job/审计
  → M2 身份/Session/CSRF/MFA
  → M3 用户/RBAC/板块
  → M4 内容/可见性
  → M5 审核/通知
  → M7-LEDGER 账本内核
  → M6 下载抵扣 + M7 商城/活跃
  → M8 公开投影/反爬
  → M9 AI + M10 视频
  → M11 OIDC
  → M12 Marketplace
  → M13 主题/插件/后台
  → M14 前端/a11y
  → M15 运维/恢复
  → M16 全面验收
  → M17 RC/上线
```

特殊依赖：

- M6 的上传/配额可以在 M7 前推进，但 `M06-DOWNLOAD` 必须依赖 `M07-LEDGER`。
- M9/M10 可在 M8 完成公开投影后推进，默认 Flag 关闭且不能阻塞核心论坛。
- M12 必须同时依赖 OIDC user-bound Token 和不可变账本。
- M13 管理后台是各领域管理端点的汇总，不替代领域内的权限、审计和测试。
- M14 的页面可以随各领域增量交付，但全量 a11y/E2E 出口必须在领域 API 稳定后完成。

### 6.2 当前唯一执行批次

当前只推进以下任务，不并行开始下一批：

| 顺序 | 任务 | 目标 |
|---:|---|---|
| 1 | `M00-TOOL-01` | 固定开发/CI 工具版本 |
| 2 | `M00-TOOL-02` | 建立根命令帮助 |
| 3 | `M00-TOOL-03` | 接通后端根检查 |
| 4 | `M00-TOOL-04` | 接通前端根检查 |
| 5 | `M00-TOOL-05` | 接通原型检查 |
| 6 | `M00-TOOL-06` | 接通契约、迁移、文档和 Secret 检查 |
| 7 | `M00-TOOL-07` | 聚合 `make check` 并测试失败传播 |
| 8 | `M00-TOOL-08` | 核对 ignore 边界 |
| 9 | `M00-TOOL-09` | 清理 README 过期事实 |
| 10 | `M00-TOOL-10` | 干净环境复现并记录证据 |

完成 `M00-TOOL` 后再从 `M00-CONTRACT-01` 开始。不要提前实现业务 Handler。

## 7. OpenAPI 172 operations 机械覆盖

覆盖文件：

- 人类可读表：[`todo/OPENAPI-COVERAGE.md`](todo/OPENAPI-COVERAGE.md)
- 机器状态源：[`todo/openapi-operation-coverage.json`](todo/openapi-operation-coverage.json)
- 同步命令：`ruby scripts/sync-operation-coverage.rb`
- 只检查：`ruby scripts/sync-operation-coverage.rb --check`

每个 operation 必须依次达到：

```text
not_started → baseline_only/in_progress → implemented → verified
                                  ↘ blocked
```

`verified` 至少要求：

- method/path/operationId 与契约一致；
- handler 和 domain service 存在；
- 权限、对象范围、CSRF、幂等、If-Match、Cache-Control 和审计按契约实现；
- 正常响应和全部稳定 Problem code 有 Fixture；
- 适用的三数据库、浏览器、隐私或专项安全测试通过；
- owner、handler、tests 和 evidence 字段完整。

任何 OpenAPI 新增/删除/重命名必须先更新契约并运行同步；未分配工作包或覆盖文件过期会使路线图校验失败。

## 8. 需求、测试与工作包追踪

| 冻结范围/发布门槛 | 主要工作包 | 关键测试工作包 |
|---|---|---|
| 账号、邮箱验证、Session、TOTP | `M02-IDENTITY`、`M02-SESSION`、`M02-MFA`、`M02-UX` | `M16-HARNESS`、`M16-SECURITY` |
| 资料、Cover、注销匿名化 | `M03-PROFILE`、`M06-QUOTA` | `M16-SECURITY`、`M16-STORAGE-FAULTS` |
| RBAC、板块与对象授权 | `M03-AUTHZ`、`M03-BOARDS` | `M16-SECURITY` |
| Markdown、文章、讨论、回复 | `M04-SCHEMA`、`M04-MARKDOWN`、`M04-POSTS`、`M04-COMMENTS` | `M16-HARNESS`、`M16-SECURITY` |
| public/logged_in/after_reply/level/paid | `M04-VISIBILITY`、`M06-DOWNLOAD`、`M07-LEDGER` | `M16-SECURITY`、`M16-ECONOMY` |
| 举报、审核、处罚、申诉、通知 | `M05-RISK`、`M05-CASES`、`M05-SANCTIONS`、`M05-APPEALS`、`M05-NOTIFY` | `M16-HARNESS`、`M16-SECURITY` |
| Local/S3、短效 URL、共享容量 | `M06-ADAPTER`、`M06-UPLOAD`、`M06-QUOTA`、`M06-MIGRATION` | `M16-STORAGE-FAULTS` |
| 下载抵扣、Range、授权重签 | `M06-DOWNLOAD`、`M07-LEDGER` | `M16-ECONOMY`、`M16-STORAGE-FAULTS` |
| B 币、签到、等级、商城、装扮 | `M07-LEDGER`、`M07-LEVELS`、`M07-SHOP` | `M16-ECONOMY` |
| 搜索、RSS/Atom、SEO、AI 防爬 | `M08-INDEX`、`M08-FEEDS`、`M08-CRAWL` | `M16-SECURITY`、`M16-PERF` |
| AI 逐次同意和建议 | `M09-GATEWAY`、`M09-TASKS`、`M09-SUGGESTIONS` | `M16-SECURITY`、`M16-STORAGE-FAULTS` |
| Direct/HLS/Xigua | `M10-VIDEO`、`M10-UI` | `M16-SECURITY`、`M16-STORAGE-FAULTS` |
| OIDC Provider | `M11-PROTOCOL`、`M11-CONSENT` | `M16-SECURITY`、`M17-FLAGS` |
| 第三方 Marketplace | `M12-CLIENTS`、`M12-CHECKOUT`、`M12-REFUND` | `M16-ECONOMY`、`M16-SECURITY` |
| 主题、配置型插件、管理后台 | `M13-THEME`、`M13-PLUGIN`、`M13-ADMIN` | `M14-A11Y`、`M16-SECURITY` |
| SvelteKit、WCAG、无 JS | `M14-ROUTES`、`M14-COMPONENTS`、`M14-A11Y` | `M16-RELEASE-TEST` |
| 部署、优雅停机、备份恢复 | `M15-PACKAGE`、`M15-BACKUP`、`M15-UPGRADE`、`M15-RUNBOOK` | `M16-RELEASE-TEST`、`M17-ENV` |
| 三数据库、上一 client、故障矩阵 | `M01-DB`、`M16-HARNESS`、`M16-STORAGE-FAULTS` | `M16-RELEASE-TEST` |
| 法律、运营、隐私与正式上线 | `M17-LEGAL`、`M17-LAUNCH` | `M17-FREEZE`、`M17-SMOKE` |

专项遗漏已经显式落入叶子任务：Range 不重扣、S3 virtual-host/path-style/multipart、local↔S3 迁移回滚、无 JavaScript、优雅停机、上一版本生成 client、Worker lease/崩溃/SQLite busy、邮件 token 日志和 S3 403/404/429/5xx 故障矩阵。

## 9. v1.0 上线定义与阻断条件

以下断言必须全部为真；它们是发布 Gate，不是另一份可勾选任务，证据来自 M15-M17：

1. 所有 544 个 P0 和 234 个 P1 任务完成并有证据；没有未批准 `[!]`；5 个 P2 若未执行，必须保持默认关闭并记录负责人和启用计划。
2. SQLite、MySQL 8、MariaDB 10.11 的迁移、仓储契约、HTTP 契约和关键并发测试全部绿。
3. OpenAPI 173/173 operations 为 `verified`，权限、错误码、状态机、事件和 Schema 无未批准差异。
4. 匿名、未验证、冷静期、member、moderator、administrator、restricted、mute 和 banned persona 的服务端权限通过。
5. 隐藏正文不出现在 API、SSR、DOM、hydration、搜索、Feed、SEO、通知、日志、审计、AI、附件或公共缓存。
6. 积分、解锁、下载、商城、Marketplace 和退款不重复扣款，不修改历史流水，故障时完整回滚。
7. S3 URL 到期不删除对象、不释放容量、不重复收费；对象、授权和 URL 生命周期独立。
8. AI 每次正文外发有明确同意；AI 不能自动封禁、删除、放行或修改权限。
9. 反爬不仅依赖 robots/User-Agent，行为检测和分级响应可用且有人工复核。
10. 默认主题通过 Playwright、axe/WCAG 2.2 AA、键盘、移动端、减少动效和无 JS 验收。
11. 数据库、附件和 OIDC key 有真实备份恢复证据；迁移、回滚、停用和事故 Runbook 被非作者执行过。
12. 核心论坛在 AI、Video、Download Billing、OIDC 和 Marketplace 关闭/故障时可独立运行。
13. 可选能力只有通过专项门槛后才按 Provider/Client/Scope 开启，并具备紧急关闭和观察指标。
14. 实际部署地区、运营主体、条款、隐私、邮件、内容处理和法律责任完成批准。
15. RC 无未关闭 P0/P1 缺陷；构建、SBOM、Secret、依赖、性能、恢复和人工验收报告完整。

## 10. 变更控制

- 冻结产品语义变化必须先取得产品所有者确认；不能由实现任务自行改为 v1.1/v2。
- API、状态、权限、账务、隐私或保留语义变化必须同步 Requirements、OpenAPI、Schema、Security、Testing 和专项文档。
- 不得修改已执行迁移；只能新增不可变迁移。
- 不得提交真实 Secret、用户数据、生产 URL、备份、日志或构建产物。
- 可选 Provider 未通过 Gate 时保持关闭，不得用“临时开启测试”绕过审批。
- 每次任务状态变化后运行：

```sh
ruby scripts/sync-operation-coverage.rb --check
ruby scripts/check-roadmap.rb
```

## 11. 唯一参考文档

- 产品与事实来源：[`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md)、[`docs/PRODUCT-DECISIONS.md`](docs/PRODUCT-DECISIONS.md)、[`docs/DOCUMENT-STATUS.md`](docs/DOCUMENT-STATUS.md)
- API 契约：[`openapi/openapi.yaml`](openapi/openapi.yaml)、[`docs/API.md`](docs/API.md)、[`docs/API-CONTRACTS.md`](docs/API-CONTRACTS.md)、[`docs/ERROR-CODES.md`](docs/ERROR-CODES.md)
- 数据与规则：[`docs/SCHEMA.md`](docs/SCHEMA.md)、[`docs/STATE-MACHINES.md`](docs/STATE-MACHINES.md)、[`docs/PERMISSION-MATRIX.md`](docs/PERMISSION-MATRIX.md)、[`docs/EVENT-CATALOG.md`](docs/EVENT-CATALOG.md)
- 安全与验证：[`docs/SECURITY.md`](docs/SECURITY.md)、[`docs/TESTING.md`](docs/TESTING.md)、[`docs/RETENTION-PRIVACY.md`](docs/RETENTION-PRIVACY.md)
- 运维与配置：[`docs/OPERATIONS.md`](docs/OPERATIONS.md)、[`docs/CONFIGURATION.md`](docs/CONFIGURATION.md)
- 专项能力：[`docs/STORAGE.md`](docs/STORAGE.md)、[`docs/DOWNLOAD-BILLING.md`](docs/DOWNLOAD-BILLING.md)、[`docs/INTERNAL-MARKETPLACE.md`](docs/INTERNAL-MARKETPLACE.md)、[`docs/AI.md`](docs/AI.md)、[`docs/VIDEO-PLUGIN.md`](docs/VIDEO-PLUGIN.md)、[`docs/AUTH-OIDC.md`](docs/AUTH-OIDC.md)、[`docs/MARKETPLACE.md`](docs/MARKETPLACE.md)、[`docs/MARKETPLACE-ACCOUNTING.md`](docs/MARKETPLACE-ACCOUNTING.md)、[`docs/CRAWLER-POLICY.md`](docs/CRAWLER-POLICY.md)
