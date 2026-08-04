# BBLBB 项目代码变更审计问题

> 本文件由每小时代码审计任务维护。问题必须有可复现证据；没有问题的代码只能在独立验证后提交并推送。
> 审计范围：自上次审计以来的工作区变更、未提交变更和新增提交；首次审计同时检查当前工作区基线。

## 使用规则

- `P0`：安全、数据丢失、权限绕过、账务不一致、隐私泄漏或无法恢复；立即阻止相关代码提交/推送。
- `P1`：功能错误、契约破坏、迁移/跨数据库不一致、关键测试缺失或发布阻断；相关变更不得推送。
- `P2`：非阻断质量问题、文档漂移、可维护性或测试增强；可单独记录，不能与无关代码混合提交。
- 每个问题必须包含：变更范围、复现命令、实际结果、预期结果、影响、建议修复和状态。
- 已确认无问题的变更必须记录审计命令、测试结果、提交哈希和推送结果。
- 审计任务不得执行 `git reset --hard`、`git clean`、强制推送、删除分支或覆盖未审查的他人改动。
- 发现并发编辑时只读检查并记录，不覆盖、回滚或代提交并发中的文件。
- 注意区分「本轮引入的缺陷」与「路线图已跟踪但尚未实现的功能」：后者不阻止仓库提交，但不得被当作已完成。

## 当前未关闭问题

| ID | 严重级别 | 首次发现 | 变更范围 | 问题与影响 | 复现/证据 | 负责人 | 状态 | 复查日期 |
|---|---|---|---|---|---|---|---|---|
| `ISSUE-20260804-001` | P0 | 2026-08-04 | `frontend/src/routes/posts/[id]/+page.svelte` 132/148 行 | 存储型 XSS：`{@html formatContent(content)}` 在转义后才生成 `<a href>`，链接 URL 未二次清洗，`[x](javascript:alert(1))` 与 `x" onmouseover="alert(1)` 属性逃逸可执行；帖子/评论对全体访客渲染 | 复现：发布内容 `[x](javascript:alert(document.domain))` 后打开帖子页 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-002` | P0 | 2026-08-04 | `backend/src/app.rs`、`backend/src/routes/auth.rs`、`frontend/src/lib/api/client.ts` | CSRF 防护整体缺失：无 CSRF 校验中间件，`GET /api/v1/auth/csrf` 返回的 token 从未被验证，前端也不发送 `x-csrf-token`；登录 CSRF（跨站表单注入 Set-Cookie）与 Cookie 写操作无保护 | 复现：无 `x-csrf-token` 校验直接 POST `/api/v1/auth/login` 成功；`app.rs` 中间件链仅 security_headers/timeout/body-limit/trace/request_id | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-003` | P1 | 2026-08-04 | `backend/src/auth/`、`backend/src/routes/*` 新文件 | `cargo fmt --all -- --check` 失败：新代码未按 rustfmt 格式化，`make check-backend` 与 CI 会阻断 | 命令：`cd backend && cargo fmt --all -- --check` → exit 1（大量 Diff） | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-004` | P1 | 2026-08-04 | `backend/src/routes/*`、`backend/src/app.rs` | `cargo clippy -- -D warnings` 失败（28 个错误）：manual_clamp、unused imports/variables、字段未读等 | 命令：`cd backend && cargo clippy --workspace --all-targets --all-features -- -D warnings` → 28 errors | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-005` | P1 | 2026-08-04 | `frontend/src/routes/boards/[slug]/`、`posts/[id]/`、`users/[username]/` | `npm run check` 失败（5 个 TS 错误）：`page.params` 的 `slug/postId/username` 是 `string \| undefined`，传入需要 `string` 的 API 函数 | 命令：`cd frontend && npm run check` → 5 errors in 3 files | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-006` | P1 | 2026-08-04 | `backend/src/main.rs` 启动迁移逻辑 | 生产启动自动应用迁移，违反 M01-DB-06「生产服务启动不得自动应用未知迁移」；应改为显式 `migrate` 命令 | 命令：`grep -n run_migrations backend/src/main.rs` → 启动路径直接调用 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-007` | P1 | 2026-08-04 | `backend/src/main.rs`、`backend/src/db/pool.rs` | 数据库 URL 被完整写入日志（`tracing::info!(url = %config.database_url)`）；DSN 含密码（如 `mysql://user:pass@`）时会泄漏凭据，违反 M15-OBSERVE-02 | 命令：`grep -n 'database_url' backend/src/main.rs`；日志样例 `database pool created url=...` | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-008` | P1 | 2026-08-04 | `scripts/deploy.sh` | 脚本含真实生产服务器 IP（`root@186.241.84.165`）与域名（`bblbb.com`/`api.bblbb.com`），违反路线图「不得提交真实生产 URL」；且会整体覆盖远程 `/etc/caddy/Caddyfile` 与 systemd unit | 复现：读取脚本头部；`grep -n '186.241' scripts/deploy.sh` | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-009` | P2 | 2026-08-04 | `backend/src/routes/auth.rs`、`users.rs`、`posts.rs` | 登录/注册/密码重置无账号与 IP 双维度限流（M02-IDENTITY-06、M02-SESSION-03 未实现即提交了可访问端点） | 复现：连续调用 `POST /api/v1/auth/login` 无 429 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-010` | P2 | 2026-08-04 | `backend/src/auth/session.rs`、`users.rs`、`routes/auth.rs` | `level` 与 `roles` 硬编码为 `1`/`[]`，`resolve_session` 未读取真实等级/角色，用户等级功能为占位 | 复现：注册新用户登录后 `GET /api/v1/me` 恒为 `level: 1, roles: []` | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-011` | P2 | 2026-08-04 | `backend/src/auth/session.rs` | Session Cookie 未使用 `__Host-` 前缀（M02-SESSION-02），当前为 `bblbb_session`；建议改为 `__Host-bblbb_session` 并保持 Secure/HttpOnly/SameSite=Lax | 复现：登录响应 Set-Cookie 无 `__Host-` 前缀 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-012` | P2 | 2026-08-04 | `frontend/src/lib/api/client.ts` | `getUser` 将公开用户端点类型化为 `User`（含 email/email_verified 等），实际接口只返回 `{username, display_name, status}`；类型契约漂移，字段恒为 undefined | 复现：`GET /api/v1/users/{username}` 响应与 `User` 接口不一致 | unassigned | 待修复 | 2026-08-11 |

## 已关闭问题

| ID | 严重级别 | 发现日期 | 修复提交 | 验证命令与结果 | 关闭日期 | 评审 |
|---|---|---|---|---|---|---|
| 暂无 | — | — | — | — | — | — |

## 审计记录

| 审计 ID | 时间 | 审计范围 | 结果 | 验证命令/结果 | 可提交变更 | 提交/推送 |
|---|---|---|---|---|---|---|
| `AUDIT-SETUP` | 2026-08-04 | 建立每小时审计机制 | 首次代理运行超时失败，遗留锁文件；调度任务需修复 | 首次运行 183 次工具调用后超时；后续每小时运行 0 工具调用即失败 | 待修复调度后重审 | — |
| `AUDIT-20260804-MANUAL` | 2026-08-04 | 当前工作区全部变更（路线图组已暂存；后端/前端/迁移组未提交） | 发现 12 个问题（2×P0、6×P1、4×P2） | 验证通过：Rust 7 单元 + 5 HTTP 测试、SQLite 空库迁移、0002/0003 三库迁移等价、原型 interaction 22 路由、路线图 87/783、OpenAPI 172/172；失败：`cargo fmt --check`、`cargo clippy -D warnings`（28）、`npm run check`（5） | 路线图组（`.gitignore`、`TODO.md`、`todo/`、`scripts/*.rb`） | `docs: establish v1.0.0-rc.2 execution roadmap` 提交并推送（待执行） |

### AUDIT-20260804-2246

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-04T22:46:47Z |
| 起始 HEAD | 35e1653b74e194049dceac908117f07acf7e4608 |
| 结束 HEAD | 35e1653b74e194049dceac908117f07acf7e4608 |
| docs 组 | green |
| code 组 | blocked |
| 失败检查 | cargo-fmt|cargo-clippy|frontend-check|prod-url-scan| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 58 项 |
