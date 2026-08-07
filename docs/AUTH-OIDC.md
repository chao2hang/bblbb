# BBLBB — 本地认证与 OpenID Connect Provider

> 版本：v0.5
> 本文同时定义论坛本地 Session 和 v1.0 的 OIDC Provider。OAuth 2.0 负责授权；OpenID Connect 在其上提供身份登录。

## 1. 支持范围

v1.0 本地认证：

- 邮箱/用户名 + 密码。
- 邮箱验证、找回密码、多设备 Session。
- 可选 TOTP。

v1.0 OIDC：

- Authorization Code Flow。
- 所有 Client 强制 PKCE S256。
- Public 与 Confidential Client。
- `openid profile email` scope。
- RS256 ID Token。
- opaque Access Token。
- Refresh Token Rotation + reuse detection。
- Discovery、JWKS、UserInfo、Revocation、RP-Initiated Logout。

不支持 implicit、resource owner password、client credentials（除非未来有明确机器 API 场景）和 device flow。

## 2. 本地认证流程

### 注册

1. 规范化用户名和邮箱并检查唯一性。
2. 检查密码长度、泄漏密码和限流。
3. Argon2id 生成 PHC hash。
4. 创建 `pending` 用户和邮箱验证 token。
5. 同事务写 Outbox 邮件事件。
6. 验证成功后设为 `active`，再允许完整发帖权限。

### 登录

1. 按账号和 IP 执行限流。
2. 使用常量时间思路处理不存在用户，避免明显时间差。
3. 校验状态、密码和可选 TOTP。
4. 旋转 Session，写安全审计并重置失败计数。
5. 设置 `__Host-bblbb_session`。

### TOTP enrollment（M02-MFA-02）

1. 服务端生成 20 字节（160 bit）TOTP secret（RFC 6238，SHA-1，6 位，30 秒周期）。
2. `begin_enrollment` 撤销该用户既有 TOTP（重复启用 = 撤销旧 + 新建），
   将 secret 以 AES-256-GCM 加密后写入 `totp_credentials`（pending 行），
   返回二维码最小数据（otpauth URI + base32 secret）；secret 与 code 一律
   不落日志。
3. 用户提交 6 位 code → 时间窗口内（当前步 ±1）且未重放的 step 通过后
   原子启用（`confirmed_at` + `last_accepted_step`）。
4. `cancel_enrollment` 撤销未完成的 pending enrollment。

### 强制 TOTP enrollment（M02-MFA-05/06）

- **强制范围**：administrator / global moderator / board moderator / 自定义
  elevated 角色，以及持有任何 `sensitive`/`system` 风险权限的高风险账务账号
  （`points.adjust`、`marketplace.refund_admin`、`download_billing.manage`、
  `storage.manage`、`user.manage`/`role.manage`/`admin.manage` 等）——**必须**
  完成 TOTP enrollment。普通 member（全部 normal 权限）保持**可选**。
- **判定**：`aggregate_permissions` 在聚合角色/权限后检查
  `aggregation_requires_totp`——未完成 enrollment 的强制账号**降级为 member
  基线**（fail-closed）：聚合不返回 elevated 权限/角色，SessionUser.roles 与
  `/me` 投影只宣称 `member`，需要 elevated 权限的授权判定一律拒绝（403）。
- **实时生效**：判定实时依赖 `totp_credentials`（confirmed + 未撤销）状态，
  完成 enrollment 后同一会话立即恢复全部权限，无需重建 Session；停用/撤销
  TOTP 后立即降级。
- **TOTP 禁用窗口**：停用 MFA 后（`DELETE /auth/mfa`，单事务撤销 TOTP + 失效
  恢复码 + 安全通知）强制账号立即失去 elevated 权限，须重新 enrollment。

### 重置密码

- 一次性 token，数据库只存哈希，30 分钟过期。
- 返回统一响应，不泄漏邮箱是否存在。
- 成功后撤销其他 Session、可选撤销 OAuth Refresh Token family，并发送通知。

## 3. Session 生命周期

- Token 至少 256 bit 随机熵，使用 CSPRNG。
- 数据库保存 SHA-256/HMAC 哈希，不存原 token。
- Idle timeout 建议 14 天；absolute timeout 建议 60 天，可配置。
- `last_seen_at` 不必每请求写库，可按时间窗口节流更新。
- 登出、改密码、账号封禁、管理员撤销会设置 `revoked_at`。
- 用户角色改变时无需立刻重建 Session；每次授权读取当前权限或安全缓存版本。
- 管理员提升权限时旋转当前 Session，降低 fixation 风险。

## 4. CSRF

- Cookie Session 的所有状态修改端点需要 synchronizer CSRF token 和 Origin 检查。
- `GET /api/v1/auth/csrf` 返回与当前 Session 绑定的短期 token；匿名登录/注册使用独立预认证 CSRF 状态。
- 登录请求同样进行 Origin 校验，防 login CSRF。
- OIDC 同意 POST 需要 Session + CSRF。
- Bearer-only API 不使用 Cookie 身份时无需 CSRF。

## 5. OIDC 标准端点与交互页面

```text
GET  /.well-known/openid-configuration
GET  /oauth/authorize
POST /oauth/token
GET  /oauth/userinfo
GET  /oauth/jwks.json
POST /oauth/revoke
GET|POST /oauth/logout
```

`/oauth/*` 全部由 Rust 处理。需要登录或同意时：

1. Rust 完整校验授权请求并创建短期、一次性的 `oauth_interactions` 记录。
2. Rust 将浏览器 303 到 SvelteKit `/auth/consent/{interaction_id}`（未登录时先进入登录页，并保留 interaction ID）。
3. SvelteKit 只展示 Rust 返回的已验证 Client/scope 摘要。
4. 同意/拒绝以 Session + CSRF POST 到业务 API `/api/v1/oauth/interactions/{id}/decision`。
5. Rust 消费 interaction，签发授权码，并 303 到最初已验证的 redirect URI。

SvelteKit 不接收、重建或自行验证原始 `redirect_uri`，避免形成第二套协议实现和开放重定向。Discovery 中的 URL 全部来自固定、验证过的 `PUBLIC_ORIGIN`，不根据不可信 Host 头动态生成。

## 6. Client 类型

### Public Client

- 浏览器、桌面或移动应用，不能安全保存 secret。
- `client_secret_hash` 为空。
- 必须 PKCE S256。

### Confidential Client

- 有可信服务端的站点。
- secret 仅创建/重置时显示一次，数据库保存安全 hash。
- token endpoint 支持约定的一种客户端认证方式；v1 建议 `client_secret_basic`。
- 同样强制 PKCE，形成纵深防御。

所有 Client：

- redirect URI 必须预注册，并按 OAuth/OIDC 的注册 URI 比较规则匹配；除明确的 localhost 开发规则外，不做前缀、通配或可能改变语义的自定义规范化。
- HTTPS 是生产默认；仅 native app 的 loopback redirect 可按规范例外。
- post logout redirect URI 单独预注册，不能复用任意授权 redirect。
- 禁止 fragment、通配符和开放重定向器。
- 状态为 disabled 时不能新授权或刷新，现有 token 按管理员策略撤销。

### 6.1 管理端 Client 管理（`/api/v1/admin/oauth-clients`）

- 列表/创建/查询/更新全部要求 `admin.manage` 权限，写操作额外要求
  `reason` 与近期认证（step-up，默认 5 分钟窗口）并写审计。
- 创建：`name`、`client_type`（`public`/`confidential`）、`redirect_uris`、
  `post_logout_uris`、`scopes`（仅 `openid/profile/email` 白名单）。
- URI 精确校验（§6）；`confidential` secret 只在创建/重置时返回一次，
  数据库恒存 SHA-256 hash。
- 更新为版本化（`If-Match` 乐观锁），可停用/启用（`status`）与重置 secret。
- 停用后 authorize/token/refresh 全部拒绝；历史 token 按管理员策略撤销。

## 7. 授权请求

最小请求：

```text
response_type=code
client_id=...
redirect_uri=...
scope=openid profile email
state=...
nonce=...
code_challenge=...
code_challenge_method=S256
```

Provider 校验：

- Client 与 redirect。
- `response_type=code`。
- scope 子集。
- PKCE 格式和 S256。
- OIDC 请求必须有 `openid`；v1 要求 nonce。
- 参数长度和重复参数歧义。

认证后通过短期 interaction 显示同意页：Client 名称、所有者、redirect 域名、申请 scope、是否以前同意。用户拒绝后由 Rust 返回标准 `access_denied`。

## 8. 授权码

- 高熵随机值，数据库只存 hash。
- 默认 5 分钟过期。
- 绑定 Client、用户、redirect URI、scope、nonce、PKCE challenge 和 auth_time。
- token endpoint 在事务中原子标记 `consumed_at`。
- 即使换 token 失败，也需明确是否允许重试；推荐单次消费事务内只在全部校验成功后消费和创建 token。
- 重放返回标准 `invalid_grant`，不泄漏更多信息。

## 9. Token

### ID Token

- JWT，RS256，含 `kid`。
- 必需 claim：`iss`、`sub`、`aud`、`exp`、`iat`、`auth_time`；授权请求有 nonce 时返回 `nonce`。
- 多 audience 时按规范处理 `azp`。
- 默认有效期 5–10 分钟。
- 不在 ID Token 放论坛角色、余额、处罚和内部用户 ID。

### Access Token

- opaque，高熵随机值，数据库只存 hash。
- 建议 10 分钟有效。
- `/userinfo` 验证 token、Client、scope、过期、撤销和用户状态。
- 不把 access token 记录到日志。

### Refresh Token

- 默认 30 天，可配置绝对期限。
- 每次使用后签发新 token，旧 token 标记 `used_at`。
- 旧 token 再次出现视为泄漏，撤销整个 family 并通知用户/Client 所有者。
- scope 只能缩小，不能在刷新时扩大。

## 10. Scope 与 Claim

| Scope | Claim |
|---|---|
| `openid` | `sub` |
| `profile` | `preferred_username`、`name`、`picture`、`updated_at` |
| `email` | `email`、`email_verified` |

- `sub` 使用 Pairwise Subject；不直接输出 `users.id`。
- `email` 只在用户同意且 scope 存在时输出。
- `picture` 应为安全公开 URL，不输出私有附件签名 URL。
- 未来自定义 scope 需要独立评审；默认不输出角色和权限。
- 市场交易 scope 与身份 claim scope 分离。`openid/profile/email` 永远不能发起扣款；`marketplace.purchase`、`marketplace.refund` 等高风险 scope 仅授予经管理员批准的 Confidential Client，并按 [`MARKETPLACE.md`](MARKETPLACE.md) 单独同意、限额和审计。

## 11. 用户同意

- `oauth_consents` 保存用户对 Client 的 scope 集合。
- 新增 scope、Client 元数据/redirect 高风险变化或管理员强制时重新同意。
- 用户可在账号设置查看并撤销授权；撤销同时撤销该 Client 的 Refresh Token family。
- Confidential Client 所有者不能代替用户同意。

## 12. 签名密钥

状态：`active → retiring →（超过保留期后移除）`。

轮换：

1. 生成新密钥并加密保存私钥。
2. 在同一事务内先将现有 active key 标记 `retiring`（仍发布在 JWKS），
   再插入新 active key——先发布、再切换，期间新旧公钥同时可验证。
3. 旧 key 保留到所有签发 Token 过期及安全余量（默认 24h + 最长 Refresh
   Token 有效期）后由 `purge_expired_keys` 移除。
4. 轮换写 `key_audit_json` 与审计日志（actor/reason）。

- 私钥加密主密钥来自秘密文件/系统凭据，不和数据库备份放在同一未隔离位置。
- 备份必须包含加密私钥和解密主密钥的独立恢复方案（见 `OPERATIONS.md` §10）。
- 服务启动不得临时生成新 key 掩盖丢失；已存在密钥无法用主密钥解密时
  直接失败（readiness 失败，fail-closed）。

## 13. Logout 与 Revocation

- 本地登出撤销当前 Session。
- RP-Initiated Logout 验证 `id_token_hint`、允许的 post logout redirect URI 和 state。
- `/oauth/revoke` 支持撤销 access/refresh token，响应避免 token 枚举。
- 用户封禁时至少撤销 Session 和 Refresh Token；短 Access Token 等待过期或立即标记撤销。
- 用户可按 Client 撤销 consent 和 token。

## 14. 错误与缓存

- OIDC/OAuth 端点使用标准错误码，不套业务 API problem 格式破坏协议。
- 不把内部错误、账号状态和 Token 存在性放入 `error_description`。
- `/oauth/token`、`/oauth/userinfo`、interaction 和同意页面响应 `Cache-Control: no-store`。
- redirect 错误前必须先验证 redirect URI；无效 URI 时直接显示本地错误，不能重定向。

## 15. 安全事件

以下写安全审计并可通知用户：

- 新设备登录。
- 密码/TOTP 变化。
- OAuth Client 首次授权或新增 scope。
- Refresh Token 重用。
- Client secret 重置。
- 签名密钥轮换。
- 管理员撤销 Session/授权。

## 16. 验收

- 使用 OpenID Foundation conformance suite 的适用 Basic OP 配置。
- 覆盖 PKCE downgrade、redirect 变体、授权码重放、nonce、aud/iss、Refresh Token 重用和 key rotation。
- 测试 Public/Confidential Client。
- 对接至少两个独立测试 RP，不只测试 BBLBB 自己的前端。
