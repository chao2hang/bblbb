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
| W6 (M5) | 进行中 | — | `aaae708`(M05-SCHEMA) | M05-SCHEMA 已闭环（8 叶子任务 `[x]`，0041-0045 三库迁移 + moderation/notifications 模型 + schema 测试 + 文档；agent A 完成）；M05-RISK/CASES/SANCTIONS/APPEALS/NOTIFY/UI 逐包推进 |

## 5. 首批（W0）并行分工

- **A-backend**：修复 `backend/` + `migrations/`：CSRF 中间件（P0）、`cargo fmt` 全库、`clippy -D warnings` 清零、启动迁移治理（显式 migrate）、DSN 日志脱敏、Session cookie `__Host-`（P2）。验收：backend 域全部绿。
- **B-frontend**：修复 `frontend/`：TS 错误清零（若回归）、a11y warning 清理、登录/注册/帖子页细节。验收：`npm run check` 0 errors。
- **C-contract/tools**：修复 `openapi/`、`docs/`、`scripts/`、根配置：deploy.sh 生产 URL（P1）、OpenAPI/文档一致、`make check-openapi` 接入路线图校验。验收：契约与路线图校验绿。
