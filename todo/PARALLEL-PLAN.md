# BBLBB 多 Agent 并行执行计划

> 目标：用多 agent 并行推进 87 个工作包 / 783 个叶子任务（P0=544、P1=234、P2=5），逐 Wave 完成验收、证据和提交推送，最终达到 v1.0 正式上线标准。
> 原则：依赖 DAG 分层 → Wave 内按文件域并行 → 每项完成标记 `[x]` 并附证据 → 每 Wave 合并前跑全套验证 → 干净组提交推送。
> 状态：计划由主代理维护；每 Wave 完成在「执行进度」更新。

## 1. 并行规则

1. **依赖先行**：只有 `depends` 全部满足（或该依赖已并入本批）的工作包才能进入并行池。
2. **文件域分区**：同 Wave 的并行 agent 按文件域划分，互不重叠，避免覆盖：
   - backend 域：`backend/`、`migrations/*/`
   - frontend 域：`frontend/`
   - 契约域：`openapi/`、`docs/`、`scripts/`（校验类）
   - 工具/运维域：`Makefile`、`.github/`、`.gitignore`、`README.md`、`deploy/`、`ops/`
3. **验收门**：每个 agent 完成时运行自己域的验证命令，产出结果和证据；标记任务 `[x]` 并附 `证据：`（files/commands/contract/commit/review）。
4. **合并前全量验证**：每 Wave 合并后运行：
   ```sh
   ruby scripts/sync-operation-coverage.rb --check
   ruby scripts/check-roadmap.rb
   cd backend && cargo fmt --all -- --check && cargo clippy -D warnings && cargo test --all-features
   cd frontend && npm run check && npm run build
   cd prototype && npm run check:all
   git diff --check
   ```
   三数据库迁移、Secret/生产 URL 扫描按改动范围执行。
5. **提交推送**：只有通过验证的变更组由主代理按逻辑组 commit，fast-forward 校验后普通 push。禁止 force-push。
6. **问题记录**：agent 发现问题写入 `todo/issue.md`（按 ID 去重），不擅自扩大修复范围。
7. **运行时约束**：并行 agent 不得同时运行全量 `make check`（避免锁竞争），只跑自己域的检查；主代理负责最终全量验证。

## 2. Wave 划分（依赖分层）

| Wave | 范围 | 并行组（文件域） | 前置 |
|---|---|---|---|
| W0 | 基线稳定：修复 12 个审计问题并提交工作区 | ①backend ②frontend ③契约/工具 | 无（当前工作区） |
| W1 | M0：工具链、契约治理、后端/前端边界 | ①M00-TOOL ②M00-CONTRACT ③M00-BACKEND ④M00-FRONTEND | W0 |
| W2 | M1：DB、配置、Job/Outbox、审计 | ①M01-DB ②M01-CONFIG ③M01-JOBS ④M01-AUDIT | W1 |
| W3 | M2：身份、Session/CSRF、MFA、身份 UX | ①M02-IDENTITY ②M02-SESSION ③M02-MFA ④M02-UX | W2 |
| W4 | M3：资料、RBAC、板块/标签、搜索仓储、UI | ①SCHEMA+PROFILE ②AUTHZ+BOARDS ③SEARCH-STORE+UI | W3 |
| W5 | M4：内容 Schema、Markdown、帖子、回复、可见性、UI | ①SCHEMA+MARKDOWN ②POSTS+COMMENTS ③VISIBILITY+UI | W4 |
| W6 | M5：审核、举报、处罚、申诉、通知、UI | ①SCHEMA+RISK ②CASES+SANCTIONS ③APPEALS+NOTIFY+UI | W5 |
| W7 | M6：附件/S3/配额/下载/迁移/UI | ①SCHEMA+ADAPTER ②UPLOAD+QUOTA ③DOWNLOAD+MIGRATION+UI | W5、W8-LEDGER 前置保持依赖 |
| W8 | M7：账本、等级/签到、商城、UI | ①LEDGER ②LEVELS+SHOP-SCHEMA ③SHOP+UI | W2（AUDIT/DB）、W7 并行侧 |
| W9 | M8：索引/公开投影、Feed/SEO、反爬、UI | ①INDEX ②FEEDS ③CRAWL+UI | W4、W5、W6 |
| W10 | M9：AI Schema/Gateway/Tasks/Suggestions/UI | ①SCHEMA+GATEWAY ②TASKS+SUGGESTIONS ③UI | W2、W5、W6 |
| W11 | M10：视频核心 + UI | ①M10-VIDEO ②M10-UI | W5、W2 |
| W12 | M11：OIDC Schema/Protocol/Consent | ①SCHEMA ②PROTOCOL ③CONSENT | W3、W2 |
| W13 | M12：Marketplace Schema/Clients/Checkout/Refund/UI | ①SCHEMA+CLIENTS ②CHECKOUT ③REFUND+UI | W8、W12 |
| W14 | M13+M14：主题/插件/后台 + 全量前端 | ①M13 ②M14-ROUTES+COMPONENTS ③M14-A11Y+SEO | W3-W13 各 UI 交付 |
| W15 | M15：部署/观测/备份/升级/Runbook | ①PACKAGE+OBSERVE ②BACKUP+UPGRADE ③RUNBOOK | W1、W2 |
| W16 | M16：测试/安全/故障/经济/性能/发布验收 | ①HARNESS+SECURITY ②STORAGE-FAULTS+ECONOMY ③PERF+RELEASE-TEST | W2-W15 |
| W17 | M17：RC/预发布/冒烟/Flag/法律/上线 | ①FREEZE+ENV ②SMOKE+FLAGS ③LEGAL+LAUNCH | W16 |

> P2（M17-FLAGS-02..06）默认关闭，只记录负责人/启用计划，不阻塞 W0-W16。

## 3. 推进节奏

- 每轮对话推进 1–2 个 Wave；Wave 内并行 2–4 个 agent。
- 每 Wave 完成后主代理：合并验证 → 更新 `todo/PARALLEL-PLAN.md` 执行进度 → 提交推送 → 进入下一 Wave。
- `M00-TOOL` 的 CI 接线完成前，由主代理手动跑全量验证。

## 4. 执行进度

| Wave | 状态 | 完成时间 | 提交 | 备注 |
|---|---|---|---|---|
| W0 | 已完成 | 2026-08-06 | 既有基线 | 历史基线：12 审计问题已在更早会话修复 |
| W5 (M4) | 已完成 | 2026-08-07 | `da4840b`(COMMENTS)、`afeb696`(VISIBILITY)、`3b73bae`(UI)、`+roadmap-sync` | M4 收口：30 个叶子任务全部 `[x]`（246/783 完成）；3 域并行 agent（backend-COMMENTS / backend-VISIBILITY / frontend-UI）+ 主代理收尾（after_reply grant、canary/边界测试、路由集成、Playwright UI-10、session level 修复） |
| W6 (M5) | 已完成 | 2026-08-07 | `aaae708`(M05-SCHEMA)、`c90e8db`(SCHEMA 仪表盘)、`ed909fe`(M05-RISK)、`f23cb4d`(RISK 仪表盘)、`b8552fa`+`16de278`(M05-CASES)、`2bde5db`(CASES 仪表盘)、`300ebfd`(M05-SANCTIONS)、`656b095`(SANCTIONS 仪表盘)、`f242434`(M05-APPEALS)、`82d7401`(APPEALS 仪表盘)、`64ce403`(M05-NOTIFY)、`36ac455`(NOTIFY 仪表盘)、`7f97651`(M05-UI)、`f827bb2`(UI 仪表盘) | M5 收口：63 个叶子任务全部 `[x]`（309/783 完成）；SCHEMA/RISK/CASES/SANCTIONS/APPEALS/NOTIFY/UI 七包闭环，全门禁绿（cargo clippy 0 警告 + 86 后端用例 + 306 前端用例） |
| W7/W8a (M6+M7) | 已完成 | 2026-08-07 | `3a46751` | W7/W8a 收口：M6 存储（SCHEMA/ADAPTER/UPLOAD/QUOTA/DOWNLOAD/MIGRATION/UI）+ M7 其余（LEVELS/SHOP-SCHEMA/SHOP/UI）111 个叶子任务全部 `[x]`（431/783 完成）；5 并行 agent（upload+quota / levels+activity / shop+reactions / frontend / main 存储脊柱+download+migration）；0048/0049/0050/0051 三库迁移；90 领域测试 + 76 后端二进制全绿；clippy -D warnings 0；frontend 0 warn 360 测试；openapi 契约 +9 op（193）并修复 M5 路由缺契约与 write-contract 缺口；make check 全绿 |
| W9 (M8+M9) | 已完成 | 2026-08-07 | `5119c7d`（代码+门禁修复） | W9 收口：M8（INDEX/FEEDS/CRAWL/UI）+ M9（SCHEMA/GATEWAY/TASKS/SUGGESTIONS/UI）75 个叶子任务全部 `[x]`（495/783 完成）；4 并行 agent（search+feeds / crawl / ai / frontend；crawl 与 ai 两次子代理 API 400 失败 → 主代理自实现 antibot 与 AI 全套）；0052/0053 三库等价迁移；AI 默认关闭；antibot 中间件接入 app.rs；后端全量测试 0 fail + clippy -D warnings 0；frontend check 0 err + 445 测试 + build 绿；make check 全绿（Problem.code enum 扩展 3 码仍 193 ops；robots/sitemap 记为非契约端点；JsonLd 白名单组件） |
| W13 (M12) | 已完成 | 2026-08-07 | ``9c40184`` | W13 收口：M12 Marketplace（SCHEMA/CLIENTS/CHECKOUT/REFUND/UI）43 个叶子任务全部 `[x]`（580/783 完成）；0056_marketplace.sql 三库等价迁移；marketplace 领域层（clients/offers/checkout/refunds/webhooks/reconcile/balance）+ 7 条公开路由 + 管理端 Client/Scope/Offer/Webhook/对账/紧急停用；29 个 marketplace 集成测试（clients 9 / checkout 11 / refund 9）+ 20 个前端 SSR 测试全绿；账本恒等式 Σ=0、BEGIN IMMEDIATE 并发恰一成功、退款 reversal-only、Webhook HMAC/时间窗/dead-letter、增量对账；M11 OIDC 测试保持绿；OpenAPI 12 个 marketplace op verified；check-roadmap exit 0 |
| W14a (M13) | 已完成 | 2026-08-08 | ``eca6120``（主）+ ``d3f8b26``/``8ebc846``（commit 回填） | W14a 收口：M13 主题/插件/管理后台/UI 34 个叶子任务全部 `[x]`（614/783 完成）；0057_theme.sql 三库等价迁移（themes/theme_revisions/plugins/plugin_call_metrics/plugin_data）；theme 领域层（封闭 Token schema、fallback、revision 一致性、上传隔离态、If-Match 偏好）；plugins 领域层（capability/event/settings 白名单、危险 URL/代码扫描、policy_revision、调用摘要、无在线代码执行）；管理 API（users/roles/boards/tags/sanctions/themes/plugins + 既有 storage/ai/video/oidc/marketplace 复用 require_recent_auth）；前端 17 个 admin 页面 + 主题投影库 + 按权限导航；后端 theme/plugins/admin_routes 集成测试 + profile_routes 主题偏好改造；OpenAPI 主题 8 op + admin 配置 op verified；cargo clippy -D warnings 0、全量测试 0 fail、frontend check 0 err + 541 测试 + build 绿、migration_equivalence 4 passed、check-roadmap exit 0 |

| W13 (M12) | 已完成 | 2026-08-08 | `9c40184` + `e3751e9` | M12 收口：Marketplace 43 任务全部 `[x]`（580/783）；0056 三库迁移；领域层 clients/checkout/refunds/webhooks/reconcile + 路由 + 前端；29 后端集成测试 + 20 SSR vitest；账务恒等式 Σ=0；clippy 0 / cargo test 0 fail / frontend 520 tests / make 门禁绿 |
| W14a (M13) | 已完成 | 2026-08-08 | `eca6120` + `d3f8b26` + `8ebc846` + `58abc4b` | M13 收口：主题/插件/后台 34 任务全部 `[x]`（614/783）；0057 三库迁移；theme token schema + plugin capability allowlist + admin 扩展；1346 后端用例 + 541 前端用例 |
| W14b (M14) | 已完成 | 2026-08-08 | 待提交 | M14 收口：全量前端/a11y/SEO 32 任务全部 `[x]`（646/783）；Playwright desktop/mobile 194 用例绿（axe serious/critical=0）；vitest 567；SEO meta/JSON-LD/noindex；M14 后无 prototype mock 生产依赖 |

## 5. 首批（W0）并行分工

- **A-backend**：修复 `backend/` + `migrations/`：CSRF 中间件（P0）、`cargo fmt` 全库、`clippy -D warnings` 清零、启动迁移治理（显式 migrate）、DSN 日志脱敏、Session cookie `__Host-`（P2）。验收：backend 域全部绿。
- **B-frontend**：修复 `frontend/`：TS 错误清零（若回归）、a11y warning 清理、登录/注册/帖子页细节。验收：`npm run check` 0 errors。
- **C-contract/tools**：修复 `openapi/`、`docs/`、`scripts/`、根配置：deploy.sh 生产 URL（P1）、OpenAPI/文档一致、`make check-openapi` 接入路线图校验。验收：契约与路线图校验绿。
