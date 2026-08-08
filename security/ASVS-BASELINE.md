# BBLBB — OWASP ASVS v4.0.3 基线映射（M16-SECURITY-01）

> 负责人：platform/application-security
> 基线：v1。映射 OWASP ASVS v4.0.3 控制到 BBLBB 实现与证据；状态：
> - `implemented`：有测试/实现证据
> - `partial`：部分覆盖（列出缺口）
> - `external`：需要外部基础设施（详见 M16 阻塞项）
> - `excluded`：范围外（在"排除项"给出理由）
>
> 证据均为本仓库真实文件；状态变更必须同步本表与 docs/SECURITY.md。

## V1 架构、威胁建模（ASVS 1.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V1.1.1 | 安全架构文档 | implemented | `docs/SECURITY.md` §1 信任边界、`docs/ARCHITECTURE.md` |
| V1.2.1 | 攻击面与信任边界 | implemented | `docs/SECURITY.md` §1；`backend/src/middleware/` |
| V1.5.1 | 输入验证组件分层（服务端权威） | implemented | `backend/src/routes/*` 服务端校验；`frontend/src/lib/validation.ts` 仅 UX |
| V1.11.1 | 业务逻辑文档 | implemented | `docs/MARKETPLACE.md`、`docs/DOWNLOAD-BILLING.md`、`docs/STATE-MACHINES.md` |

## V2 认证（ASVS 2.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V2.1.1 | 密码至少 8 位 | implemented | `backend/tests/session_login.rs`、`backend/src/auth/` Argon2id |
| V2.1.7 | 认证失败不限时/不区分账号存在性 | implemented | `backend/tests/session_login.rs`（统一 401） |
| V2.2.1 | 防暴力破解（限流+锁定） | implemented | `backend/src/antibot/`、`backend/tests/antibot.rs`、`backend/tests/session_login.rs`（锁定+Retry-After） |
| V2.2.5 | 密码重置 token 一次性、哈希存储、过期 | implemented | `backend/src/auth/`、`backend/tests/password_reset.rs` |
| V2.3.1 | TOTP 认证 | implemented | `backend/src/auth/totp.rs`、`backend/tests/mfa_*.rs`（enrollment/verify/login/recovery/forced） |
| V2.5.1 | 登录后会话重建（防 fixation） | implemented | `backend/tests/session_rotation.rs`、`backend/tests/session_lifecycle.rs` |
| V2.8.1 | 敏感操作重新认证（recent-auth） | implemented | `backend/src/middleware/`、`backend/src/routes/admin.rs`、`backend/tests/mfa_stepup.rs`（`step_up_required`） |

## V3 会话管理（ASVS 3.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V3.1.1 | 会话 ID 高熵 | implemented | `backend/src/auth/session.rs`（随机 token，DB 存哈希） |
| V3.1.4 | Cookie `__Host-`、Secure、HttpOnly、Path、SameSite | implemented | `backend/tests/session_csrf.rs`、`backend/tests/session_schema.rs`（cookie 属性断言） |
| V3.2.1 | idle/absolute 过期 | implemented | `backend/src/auth/session.rs`、`backend/tests/session_status.rs` |
| V3.4.1 | 登出/改密/封禁撤销 Session | implemented | `backend/tests/session_lifecycle.rs`、`backend/tests/moderation/sanctions.rs#ban_revokes_sessions_and_marks_banned` |
| V3.5.1 | 会话单点登录后重新认证 | partial | Refresh Token 轮换（`backend/tests/session_bearer_idempotency.rs`）；SPA 单点场景由 M16 后续跟踪 |

## V4 访问控制（ASVS 4.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V4.1.1 | 服务端强制授权，前端隐藏不是边界 | implemented | `backend/src/authz/`（`authorize_action` + `authorize_object`）、`backend/tests/authz_enforce.rs#handler_pattern_action_then_object_scope`、`backend/tests/authz_no_client_elevation.rs` |
| V4.1.2 | 对象级授权（IDOR 防护） | implemented | `backend/tests/authz_object.rs`、`backend/tests/authz_enforce.rs#resource_state_boundary`、`backend/tests/visibility_boundary.rs` |
| V4.1.3 | 权限提升防护 | implemented | `backend/tests/authz_roles.rs`、`backend/tests/authz_persona.rs`、`backend/tests/mfa_forced.rs`（TOTP 缺失降级） |
| V4.2.1 | 不可变审计日志 | implemented | `backend/src/audit/`、`backend/tests/audit_logs.rs`、`backend/tests/audit_atomicity.rs` |
| V4.3.1 | 管理功能使用管理权限 | implemented | `backend/src/routes/admin*.rs`（reason+recent-auth+If-Match）、`backend/tests/admin_routes.rs` |

## V5 验证、净化、编码（ASVS 5.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V5.1.1 | 输入验证白名单 | implemented | `backend/src/content/markdown/`（allowlist 清洗）、`backend/tests/markdown_xss.rs` |
| V5.1.3 | 结构化数据 schema 校验 | implemented | `backend/src/content/posts/`（closed schema）、`backend/tests/posts_schema.rs` |
| V5.2.x | 输出编码 | implemented | Markdown 渲染+清洗、`scripts/check-html-sinks.rb` 静态强制 |
| V5.3.3 | 反序列化深度/大小限制 | implemented | `backend/src/middleware/problem.rs`、`backend/tests/edge.rs` |
| V5.5.x | SQL 注入防护（参数化） | implemented | 全仓 `sqlx::query` 参数化；`check-tx-io.rb` 边界 |

## V6 加密存储（ASVS 6.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V6.1.2 | 敏感数据静态加密 | implemented | 密码 Argon2id、token/Session/OIDC code 哈希存储、TOTP secret 加密（`backend/src/auth/`） |
| V6.3.1 | 密钥管理 | partial | OIDC RS256 私钥加密存储+轮换（`backend/src/oidc/`、`ops/backup/oidc-keys.md`）；KMS 集成不在 v1 |

## V7 错误处理（ASVS 7.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V7.1.1 | 统一错误响应（RFC 9457 problem+json） | implemented | `backend/src/error.rs`、`backend/tests/http.rs`、`backend/tests/edge.rs` |
| V7.1.2 | 错误不泄漏内部细节 | implemented | `AppError::sanitize`、`backend/tests/token_hygiene.rs`、`backend/tests/readyz.rs` |

## V8 数据保护（ASVS 8.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V8.1.1 | 最小数据收集 | implemented | `docs/RETENTION-PRIVACY.md` |
| V8.1.5 | 隐藏内容防泄漏 | implemented | `security/leak-sweep.md`（API/SSR/DOM/hydration/搜索/RSS/SEO/通知/日志/AI/缓存/附件全渠道） |
| V8.3.1 | 数据导出 | implemented | `backend/src/routes/users.rs`（export）、`backend/tests/account_deletion.rs` |
| V8.3.4 | 注销匿名化 | implemented | `backend/tests/account_deletion.rs#anonymize_user_preserves_discussion_and_disconnects_identity`、`backend/tests/deletion_lifecycle.rs` |
| V8.3.5 | 30 天删除延迟期 | implemented | `backend/tests/deletion_lifecycle.rs#due_execution_anonymizes_and_preserves_audit` |
| V8.3.6 | 法律保留覆盖删除 | implemented | `backend/tests/deletion_lifecycle.rs`（legal_hold defer）、`docs/RETENTION-PRIVACY.md` §1 |

## V9 通信（ASVS 9.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V9.1.2 | 全站 TLS | implemented | `deploy/Caddyfile.template`（HSTS）、`backend/src/middleware/security_headers.rs` |
| V9.2.1 | 传输层安全配置 | implemented | `deploy/Caddyfile.template`、`docs/OPERATIONS.md` |
| V9.3.1 | 安全响应头 | implemented | `backend/src/middleware/security_headers.rs`（CSP/HSTS/nosniff/Referrer-Policy/COOP/Permissions-Policy） |

## V11 业务逻辑（ASVS 11.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V11.1.1 | 业务逻辑完整性 | implemented | `backend/tests/economy/step_injection.rs`、`backend/tests/faults.rs`、`backend/tests/economy/ledger.rs` |
| V11.1.4 | 价格/库存/配额篡改防护 | implemented | `backend/tests/shop/core.rs`、`backend/tests/marketplace/checkout.rs#checkout_derives_amount_and_recipient_from_server_snapshot`、`backend/tests/marketplace/checkout.rs#banned_user_and_price_tamper_are_rejected` |
| V11.1.6 | 竞态/并发一致性 | implemented | `backend/tests/transaction_concurrency.rs`、`backend/tests/economy/ledger.rs#concurrent_double_debit_only_one_succeeds` |
| V11.2.x | 滥用防护（限流） | implemented | `backend/src/antibot/`、`backend/tests/antibot.rs` |

## V12 文件与资源（ASVS 12.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V12.1.1 | 上传类型白名单 | implemented | `backend/src/storage/upload.rs`（magic MIME + 扩展名）、`backend/tests/storage/upload.rs` |
| V12.1.2 | 恶意文件/压缩炸弹/图片炸弹 | implemented | `backend/tests/storage/upload.rs#complete_quarantines_on_virus_scan_mock`、`backend/tests/storage/upload.rs#scan_for_safety_rejects_svg_polyglot_and_mime_spoofing`、`backend/src/storage/upload.rs`（像素/尺寸限制） |
| V12.3.1 | 文件上传存储隔离/随机 key | implemented | `backend/src/storage/adapter.rs`（不可猜 key、路径穿越/符号链接阻断）、`backend/tests/storage/adapter.rs` |
| V12.3.2 | 附件下载 Content-Disposition | implemented | `backend/src/routes/download.rs`、`backend/tests/download/http.rs` |

## V13 API（ASVS 13.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V13.1.1 | REST 验证：方法/路径/JSON schema | implemented | `openapi/openapi.yaml`（193 ops）、`backend/tests/http.rs`、`scripts/check-openapi.rb` |
| V13.1.3 | OpenAPI 与实际实现一致 | implemented | `scripts/sync-operation-coverage.rb --check`、`scripts/check-openapi.rb`、`scripts/check-route-coverage.rb` |
| V13.1.4 | 请求 Content-Type 验证 | implemented | `backend/src/middleware/`、`backend/tests/edge.rs` |
| V13.2.1 | API 认证 | implemented | `backend/tests/session_bearer_idempotency.rs`、`backend/tests/session_login.rs` |
| V13.2.2 | API 授权 | implemented | `backend/tests/authz_enforce.rs`、`backend/tests/authz_object.rs` |
| V13.3.1 | 幂等性 | implemented | `backend/tests/idempotency.rs`、`backend/src/idempotency/` |
| V13.3.3 | 敏感操作 CSRF + 来源校验 | implemented | `backend/tests/session_csrf*.rs`、`backend/src/middleware/host_origin.rs` |

## V14 配置（ASVS 14.x）

| 控制 | 要求 | 状态 | 证据 |
|---|---|---|---|
| V14.1.1 | 硬编码密钥无 | implemented | `make check-secrets`、`ops/security/scan.sh`（secret 扫描） |
| V14.1.3 | 配置脱敏 | implemented | `backend/src/config/`（S3/SMTP/OIDC secret 脱敏）、`backend/tests/admin_routes.rs#admin_dtos_never_leak_credentials_or_private_body` |
| V14.2.1 | 依赖漏洞扫描 | implemented | `ops/security/scan.sh`（cargo audit/deny 若可用）+ `security/scan-report.md` |
| V14.2.2 | 锁文件提交 | implemented | `backend/Cargo.lock`、`frontend/package-lock.json` |
| V14.2.4 | SBOM | partial | `ops/security/scan.sh` 生成记录；真实 SBOM 附件随发布 |
| V14.4.1 | 独立最小权限账号 | implemented | `deploy/systemd/*`、`docs/OPERATIONS.md` |

## 排除项（excluded / external）

| 控制 | 理由 |
|---|---|
| V1.3.x 威胁建模（正式 STRIDE 文档） | v1 采用本文档 + docs/SECURITY.md 简化威胁模型；正式威胁建模列入 v1.1 |
| V2.4.x 联邦身份（OIDC client 端） | BBLBB 是 OIDC Provider（V12/V13 层已覆盖 provider 侧）；client 端集成由 M11 专项 |
| V3.6.1 原生应用会话 | v1 仅 Web；原生客户端为 v1.1 |
| V6.2.x 硬件密钥（HSM/TEE） | 依赖外部硬件基础设施；v1 使用加密存储 + systemd credentials |
| V10 恶意代码（防毒/沙箱） | 无代码型插件在线执行路径（v1 配置型插件）；附件病毒扫描通过外部扫描集成（默认关闭，`upload.rs` mock 可插拔） |
| V9.1.3 内部 DNS 专用证书 | 生产环境部署后外部验证（M15-PACKAGE-08 `[!]` 关联） |
| V2.9.x 用户自注册后防枚举等 | 见 V2.1.7；部分枚举面由 `not_found`/统一 401 策略覆盖 |

## 验证命令

```sh
ruby scripts/check-code-fixtures.rb     # 稳定错误码四方一致
ruby scripts/check-state-machine-matrix.rb
ruby scripts/check-client-compat.rb
bash ops/security/scan.sh --report      # 依赖/secret/license/SBOM 扫描记录
cd backend && cargo test --all-features
```
