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
| `ISSUE-20260804-009` | P2 | 2026-08-04 | `backend/src/routes/auth.rs`、`users.rs`、`posts.rs` | 登录/注册/密码重置无账号与 IP 双维度限流（M02-IDENTITY-06、M02-SESSION-03 未实现） | 复现：连续调用 `POST /api/v1/auth/login` 无 429 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-010` | P2 | 2026-08-04 | `backend/src/auth/session.rs`、`users.rs`、`routes/auth.rs` | `level` 与 `roles` 硬编码为 `1`/`[]`，`resolve_session` 未读取真实等级/角色（M03-AUTHZ 未实现） | 复现：登录后 `GET /api/v1/me` 恒为 `level: 1, roles: []` | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260804-012` | P2 | 2026-08-04 | `frontend/src/lib/api/client.ts` | `getUser` 将公开用户端点类型化为 `User`（含 email 等），实际只返回 `{username, display_name, status}`；类型契约漂移 | 复现：`GET /api/v1/users/{username}` 响应与 `User` 接口不一致 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260805-013` | P1 | 2026-08-05 | `backend/src/routes/posts.rs`、`backend/src/db/pool.rs` | `cargo fmt --check` 失败（posts.rs 路由链/`json!` 宏、pool.rs 测试断言格式未规范），code 组机械审计门禁阻断 | 复现：`cd backend && cargo fmt --all -- --check` 报 Diff in posts.rs:15/:294 与 pool.rs:372/:386/:409 | unassigned | 已关闭 | 2026-08-05 |
| `ISSUE-20260805-014` | P1 | 2026-08-05 | `backend/src`（lib 编译） | `cargo clippy -D warnings` 失败：`error: unused import: put`，code 组机械审计门禁阻断；并行编辑已在审计后修复 | 复现：`cargo clippy --workspace --all-targets --all-features -- -D warnings` 报 unused import + could not compile bblbb-backend；复查：当前 clippy 通过 | unassigned | 已关闭 | 2026-08-05 |
| `ISSUE-20260805-015` | P2 | 2026-08-05 | `openapi/openapi.yaml:6987` | 行尾多余空格使 `git diff --check` 失败，docs 组机械审计门禁阻断（openapi.yaml 不在两组成员列表内，属并行编辑遗留） | 复现：`git diff --check` 报 `openapi/openapi.yaml:6987: trailing whitespace`（`- comment` 行）；复查：当前 `git diff --check` 通过 | unassigned | 已关闭 | 2026-08-05 |
| `ISSUE-20260805-016` | P1 | 2026-08-05 | `backend/src/db/pool.rs`（并行编辑中） | AUDIT-20260805-0147 中 `cargo test --workspace --all-features` 失败阻断 code 组；并行编辑随后修复，复跑全绿（怀疑编译期间瞬态失败） | 复现：审计日志 code_fails=cargo-test；复查：`cargo test` 45 单元 + 18 edge + 7 http 全部通过 | unassigned | 已关闭 | 2026-08-05 |
| `ISSUE-20260805-017` | P2 | 2026-08-05 | `todo/M00-M02-foundation.md`、`TODO.md` | 并行编辑推进 M01-DB-02 状态但未同步 TODO.md 汇总计数，`check-roadmap.rb` 失败阻断 docs 组 | 复现：`ruby scripts/check-roadmap.rb` 报 `TODO.md total task-state counts are stale` 与 `next task M01-DB-02 is already completed or blocked` | unassigned | 待修复 | 2026-08-05 |
| `ISSUE-20260805-018` | P1 | 2026-08-05 | `backend/src/auth/mod.rs`、`backend/src/ratelimit.rs`、`backend/src/routes/auth.rs`、`backend/src/auth/resend.rs`、`backend/tests/resend_verification.rs`（并行编辑中） | `cargo fmt --all -- --check` 失败，code 组机械审计门禁阻断（邮箱验证 resend 功能开发中，并行编辑未完成格式化） | 复现：`cd backend && cargo fmt --all -- --check` 报格式差异；证据：AUDIT-20260805-0547 审计日志 code_fails=cargo-fmt、code 组 blocked、保留未提交 9 项；AUDIT-20260805-0647 再次 code_fails=cargo-fmt（`auth/session.rs`、`tests/session_rotation.rs`），保留未提交 3 项；AUDIT-20260805-0747 第三次 code_fails=cargo-fmt（preauth/CSRF 并行编辑），保留未提交 18 项；AUDIT-20260805-0847 第四次 code_fails=cargo-fmt（M02-MFA-01 TOTP 并行编辑）；AUDIT-20260805-0947 第五次 code_fails=cargo-fmt（M02-MFA-07 step-up 并行编辑，`mfa_stepup.rs`/`0016_step_up.sql`），保留未提交 10 项；AUDIT-20260805-1047 cargo-fmt 通过（全绿）；AUDIT-20260805-1147 第六次 code_fails=cargo-fmt（MFA 路由并行编辑，`routes/mfa.rs`/`tests/mfa_routes.rs`），保留未提交 17 项；AUDIT-20260805-2047 第七次 code_fails=cargo-fmt（SEARCH-STORE-01 搜索模块并行编辑，`backend/src/search/`、`backend/src/lib.rs`），保留未提交 5 项 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260805-019` | P1 | 2026-08-05 | `backend/tests/mfa_schema.rs`、`migrations/sqlite|mysql|mariadb/0015_mfa_totp.sql`（并行编辑中） | `cargo clippy --workspace --all-targets --all-features -- -D warnings` 失败，code 组机械审计门禁阻断（M02-MFA-01 TOTP 开发中） | 复现：`cd backend && cargo clippy --workspace --all-targets --all-features -- -D warnings`；证据：AUDIT-20260805-0847 审计日志 code_fails=cargo-fmt\|cargo-clippy\|cargo-test\|、code 组 blocked；AUDIT-20260805-1147 再次 code_fails=cargo-clippy（MFA 路由并行编辑，`routes/mfa.rs`/`tests/mfa_routes.rs`），保留未提交 17 项 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260805-020` | P1 | 2026-08-05 | `backend/tests/mfa_schema.rs`、`migrations/sqlite|mysql|mariadb/0015_mfa_totp.sql`（并行编辑中） | `cargo test --workspace --all-features` 失败，code 组机械审计门禁阻断（M02-MFA-01 TOTP 迁移/测试开发中，可能编译失败） | 复现：`cd backend && cargo test --workspace --all-features`；证据：AUDIT-20260805-0847 审计日志 code_fails=cargo-fmt\|cargo-clippy\|cargo-test\|；AUDIT-20260805-1047 再次 code_fails=cargo-test（frontend/UX 并行编辑），保留未提交 9 项；AUDIT-20260805-2047 第三次 code_fails=cargo-test（SEARCH-STORE-01 搜索模块并行编辑，`backend/src/search/`、`backend/src/lib.rs`），保留未提交 5 项 | unassigned | 待修复 | 2026-08-11 |
| `ISSUE-20260805-021` | P1 | 2026-08-05 | `frontend/src/lib/api/server.ts`、`frontend/src/routes/register/+page.svelte`、`frontend/src/lib/components/ui/CooldownButton.svelte`、`frontend/src/routes/verify-email/`（并行编辑中） | `npm run check`（svelte-check）失败，code 组机械审计门禁阻断（M02-UX-01/02 注册页与验证邮箱前端开发中） | 复现：`cd frontend && npm run check`；证据：AUDIT-20260805-1047 审计日志 code_fails=cargo-test\|frontend-check\|、code 组 blocked、保留未提交 9 项 | unassigned | 待修复 | 2026-08-11 |

## 已关闭问题

| ID | 严重级别 | 发现日期 | 修复提交 | 验证命令与结果 | 关闭日期 | 评审 |
|---|---|---|---|---|---|---|
| `ISSUE-20260804-001` | P0 | 2026-08-04 | 前端重写（`renderSafeMarkdown`） | `frontend/src/lib/utils.ts` 先整体转义再渲染，链接仅 http/https/mailto；帖子/评论页改用安全渲染 | 2026-08-04 | 主代理 |
| `ISSUE-20260804-002` | P0 | 2026-08-04 | W0 后端域（`csrf.rs`）+ 前端域（client CSRF） | 冒烟：带 Cookie 写请求无/错 CSRF → 403 `csrf_validation_failed`，正确 → 通过；`cargo test` 7 集成含 2 CSRF 用例 | 2026-08-04 | 主代理 |
| `ISSUE-20260804-003` | P1 | 2026-08-04 | W0 后端域 | `cargo fmt --all -- --check` 通过 | 2026-08-04 | 主代理 |
| `ISSUE-20260804-004` | P1 | 2026-08-04 | W0 后端域 | `cargo clippy -D warnings` 通过（0 警告）；`cargo test` 12 单元 + 7 集成 | 2026-08-04 | 主代理 |
| `ISSUE-20260804-005` | P1 | 2026-08-04 | W0 前端域 | `npm run check` 0 errors 0 warnings；`npm run build` 通过 | 2026-08-04 | 主代理 |
| `ISSUE-20260804-006` | P1 | 2026-08-04 | W0 后端域 | 启动默认不自动迁移；`--migrate`/`BBLBB__AUTO_MIGRATE=true` 显式应用；冒烟确认无 `_sqlx_migrations` 表时跳过 | 2026-08-04 | 主代理 |
| `ISSUE-20260804-007` | P1 | 2026-08-04 | W0 后端域 | 日志 DSN 脱敏为 `sqlite://**`；`redact_dsn` 单测覆盖 | 2026-08-04 | 主代理 |
| `ISSUE-20260804-008` | P1 | 2026-08-04 | W0 契约域 | `scripts/deploy.sh` 生产 IP/域名全部替换为 `SERVER_IP`/`example.com` 占位符；`bash -n` 通过 | 2026-08-04 | 主代理 |
| `ISSUE-20260804-011` | P2 | 2026-08-04 | W0 后端域 | Session Cookie 改 `__Host-bblbb_session`；冒烟确认 Set-Cookie 属性完整 | 2026-08-04 | 主代理 |

## 审计记录

| 审计 ID | 时间 | 审计范围 | 结果 | 验证命令/结果 | 可提交变更 | 提交/推送 |
|---|---|---|---|---|---|---|
| `AUDIT-SETUP` | 2026-08-04 | 建立每小时审计机制 | 首次代理运行超时失败，遗留锁文件；调度任务需修复 | 首次运行 183 次工具调用后超时；后续每小时运行 0 工具调用即失败 | 待修复调度后重审 | — |
| `AUDIT-20260804-MANUAL` | 2026-08-04 | 当前工作区全部变更（路线图组已暂存；后端/前端/迁移组未提交） | 发现 12 个问题（2×P0、6×P1、4×P2） | 验证通过：Rust 7 单元 + 5 HTTP 测试、SQLite 空库迁移、0002/0003 三库迁移等价、原型 interaction 22 路由、路线图 87/783、OpenAPI 172/172；失败：`cargo fmt --check`、`cargo clippy -D warnings`（28）、`npm run check`（5） | 路线图组（`.gitignore`、`TODO.md`、`todo/`、`scripts/*.rb`） | `docs: establish v1.0.0-rc.2 execution roadmap` 提交并推送（待执行） |
| `AUDIT-20260804-W0` | 2026-08-04 | W0 多 agent 并行：后端/前端/契约三域修复 12 个审计问题 | 关闭 9 个（2×P0、5×P1、2×P2），保留 3 个 P2 关联路线图任务 | 全量验证通过：Rust 12 单元 + 7 集成、`npm run check` 0 errors 0 warnings、build、原型 interaction、路线图 87/783、OpenAPI 172/172、`git diff --check`；CSRF 冒烟 403/403/通过 | W0 基线（backend/frontend/契约工具/文档四组） | 由主代理提交推送（本次） |

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

### AUDIT-20260804-2347

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-04T23:48:00Z |
| 起始 HEAD | ac265bb54ad34a7f5737765211082442cc5ab522 |
| 结束 HEAD | b3ff1fbbc60e77fe587164b7b5899a6bba952bb2 |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | code |
| 推送 | yes |
| 保留未提交 | 3 项 |

### AUDIT-20260805-0047

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T00:48:09Z |
| 起始 HEAD | 7273d36f455a1b9d638d7237504e06e72d1b5138 |
| 结束 HEAD | 7273d36f455a1b9d638d7237504e06e72d1b5138 |
| docs 组 | blocked |
| code 组 | blocked |
| 失败检查 | cargo-fmt|cargo-clippy| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 29 项 |

### AUDIT-20260805-0147

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T01:58:52Z |
| 起始 HEAD | 328e4e7d3da8e6228316b6958b8e090909a6677a |
| 结束 HEAD | 328e4e7d3da8e6228316b6958b8e090909a6677a |
| docs 组 | blocked |
| code 组 | blocked |
| 失败检查 | cargo-fmt|cargo-test| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 2 项 |

### AUDIT-20260805-0247

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T02:48:00Z |
| 起始 HEAD | 69d7320c6c8619cf0b0a6b289857007321d68a1d |
| 结束 HEAD | 2f04c2853ca0bc862cdaca1611aab9ab3dc409d4 |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | code |
| 推送 | yes |
| 保留未提交 | 1 项 |

### AUDIT-20260805-0347

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T03:47:55Z |
| 起始 HEAD | ab747dcdbbb5efa0c892a8a06845147ffff0b43c |
| 结束 HEAD | ab747dcdbbb5efa0c892a8a06845147ffff0b43c |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 0 项 |

### AUDIT-20260805-0447

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T04:48:16Z |
| 起始 HEAD | d0ba538821022c9c00f4934df722c51cfb1c822b |
| 结束 HEAD | 301f0ac7c1046838282d34f63869e22fa5541f94 |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | code |
| 推送 | yes |
| 保留未提交 | 1 项 |

### AUDIT-20260805-0547

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T05:48:59Z |
| 起始 HEAD | 9e9d33d17e0f1a37c63d16a9c06d0c829f3083fa |
| 结束 HEAD | 9e9d33d17e0f1a37c63d16a9c06d0c829f3083fa |
| docs 组 | blocked |
| code 组 | blocked |
| 失败检查 | cargo-fmt| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 9 项 |

### AUDIT-20260805-0647

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T06:49:29Z |
| 起始 HEAD | c714080ebcbcd48936b7125622efee0ea8ae660c |
| 结束 HEAD | c714080ebcbcd48936b7125622efee0ea8ae660c |
| docs 组 | green |
| code 组 | blocked |
| 失败检查 | cargo-fmt| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 3 项 |

### AUDIT-20260805-0747

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T07:49:34Z |
| 起始 HEAD | f7b016c0850dc2915de91e2a2999533d8e7a6217 |
| 结束 HEAD | f7b016c0850dc2915de91e2a2999533d8e7a6217 |
| docs 组 | green |
| code 组 | blocked |
| 失败检查 | cargo-fmt| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 18 项 |

### AUDIT-20260805-0847

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T08:47:49Z |
| 起始 HEAD | eeae7eeade7b1026f088f942f26806f78e39ae94 |
| 结束 HEAD | eeae7eeade7b1026f088f942f26806f78e39ae94 |
| docs 组 | green |
| code 组 | blocked |
| 失败检查 | cargo-fmt|cargo-clippy|cargo-test| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 4 项 |

### AUDIT-20260805-0947

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T09:51:00Z |
| 起始 HEAD | 3247e4b24fbc9b012c5a6b60e9651b0ddd833987 |
| 结束 HEAD | 3247e4b24fbc9b012c5a6b60e9651b0ddd833987 |
| docs 组 | green |
| code 组 | blocked |
| 失败检查 | cargo-fmt| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 10 项 |

### AUDIT-20260805-1047

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T10:49:11Z |
| 起始 HEAD | b0cba80a4c640493bb50045590b8d6fb6ad5baca |
| 结束 HEAD | b0cba80a4c640493bb50045590b8d6fb6ad5baca |
| docs 组 | green |
| code 组 | blocked |
| 失败检查 | cargo-test|frontend-check| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 9 项 |

### AUDIT-20260805-1147

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T11:51:12Z |
| 起始 HEAD | ed0b5ebfab9818a159f7fbe49641211a3a5eb7bb |
| 结束 HEAD | ed0b5ebfab9818a159f7fbe49641211a3a5eb7bb |
| docs 组 | green |
| code 组 | blocked |
| 失败检查 | cargo-fmt|cargo-clippy| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 17 项 |

### AUDIT-20260805-1247

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T12:50:18Z |
| 起始 HEAD | ba432254c4cc4cb8e1a309f62c0e0885930daece |
| 结束 HEAD | 5d99251546e34d85b6448b901cc964ce3e3ab5fe |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 1 项 |

### AUDIT-20260805-1347

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T13:50:46Z |
| 起始 HEAD | 1616170d5da0bc315b3094bb1716af4522789dd7 |
| 结束 HEAD | 1616170d5da0bc315b3094bb1716af4522789dd7 |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 3 项 |

### AUDIT-20260805-1447

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T14:51:58Z |
| 起始 HEAD | fe167f26d2ef5a4b3c77e4d0cc5962469a4b9dff |
| 结束 HEAD | a5ec33e2733a18ebf90ef0755c746d1c9cfce26b |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | code |
| 推送 | yes |
| 保留未提交 | 2 项 |

### AUDIT-20260805-1547

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T15:51:15Z |
| 起始 HEAD | cfc238d0e52105704d483f60c754c22586bdf2be |
| 结束 HEAD | f6945a2adbbe88fe209aee1d6c177c2069b15b81 |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 1 项 |

### AUDIT-20260805-1647

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T16:50:18Z |
| 起始 HEAD | 0a1b479bd25042d41bc9d4435fe445a8a1530034 |
| 结束 HEAD | 128bbab7170dceea97696dc6582aac883fce4603 |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | code |
| 推送 | yes |
| 保留未提交 | 1 项 |

### AUDIT-20260805-1747

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T17:52:31Z |
| 起始 HEAD | 1dba030b24685ef88c05fa6741f7f1277bf064fd |
| 结束 HEAD | bc35dc1c25567830b0e19750853c93ac944fc0ea |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | code |
| 推送 | yes |
| 保留未提交 | 2 项 |

### AUDIT-20260805-1853

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T18:58:03Z |
| 起始 HEAD | 35f0025d2816352fdc39111ce0b7714b09c62706 |
| 结束 HEAD | 8699fbc98d2795535d2794aeefadcd3d2bf1ef54 |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 4 项 |

### AUDIT-20260805-1947

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T19:51:27Z |
| 起始 HEAD | 8954933e0f6aedf029a87277d17754e096d5cbb8 |
| 结束 HEAD | cb73df5d02a90016c77f6871fd7a636a7816eea7 |
| docs 组 | green |
| code 组 | green |
| 失败检查 | none |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 10 项 |

### AUDIT-20260805-2047

| 字段 | 值 |
|---|---|
| 时间 | 2026-08-05T20:56:12Z |
| 起始 HEAD | e9ae5e7c0b15ccb855d92c5c5db158f36b9df641 |
| 结束 HEAD | e9ae5e7c0b15ccb855d92c5c5db158f36b9df641 |
| docs 组 | green |
| code 组 | blocked |
| 失败检查 | cargo-fmt|cargo-test| |
| 提交 | none |
| 推送 | no |
| 保留未提交 | 5 项 |
