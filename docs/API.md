# BBLBB — HTTP API 契约

> 版本：v0.5（2026-09-05：登记 M17-GAPFIX 社交/经济/个人域端点与 M18 原型对齐扩展，见 §20/§21）
> 机器可读事实来源为 [`../openapi/openapi.yaml`](../openapi/openapi.yaml)；本文规定所有 API 必须遵守的跨端点规则，不能与 OpenAPI 产生未经批准的差异。稳定状态、权限和错误码分别见 `STATE-MACHINES.md`、`PERMISSION-MATRIX.md` 和 `ERROR-CODES.md`。

## 1. 路径与版本

- 业务 API 前缀：`/api/v1`。
- OIDC 标准端点使用 `/.well-known/*` 和 `/oauth/*`，不放入业务版本前缀。
- 健康检查：`/healthz`、内部 `/readyz`。
- `/api/v1/openapi.json` 输出与代码一致的 OpenAPI。
- v1 内只做向后兼容新增；删除、重命名或改变语义进入 `/api/v2`。

## 2. 表示格式

- 普通 JSON：`application/json; charset=utf-8`。
- 错误：`application/problem+json`，遵循 RFC 9457 思路。
- 时间：API 使用 RFC 3339 UTC 字符串；数据库内部使用 Unix 毫秒。
- ID：规范化 UUID 字符串。
- 金额：整数最小单位，不使用浮点数。
- 枚举值使用稳定小写 snake_case。
- 未授权敏感字段应完全省略，不返回 `null` 暗示存在。

## 3. 认证方式

### 浏览器会话

- 使用 `__Host-bblbb_session` Cookie。
- 写操作同时要求 `X-CSRF-Token`。
- API client 使用 `credentials: same-origin`。

### OIDC Access Token

- `Authorization: Bearer <opaque-token>`。
- Token scope 与用户权限都必须满足；scope 不能提升用户本身没有的权限。
- Bearer 请求不依赖 Cookie 时不要求 CSRF。

不允许在 URL query、日志或 Referer 中携带 Token。

## 4. 请求 ID

- Caddy 若没有请求 ID，由 Rust 生成 UUID v7。
- 请求头：`X-Request-ID`；只有来自可信代理且格式正确时才接受上游值。
- 响应始终返回 `X-Request-ID`。
- 错误、日志、审计和任务关联该 ID。

## 5. 错误格式

```json
{
  "type": "https://docs.bblbb.example/problems/version-conflict",
  "title": "资源已被修改",
  "status": 409,
  "code": "version_conflict",
  "detail": "请刷新后重新提交。",
  "instance": "/api/v1/posts/019...",
  "request_id": "019...",
  "errors": [
    { "field": "title", "code": "too_long", "message_key": "validation.title_too_long" }
  ]
}
```

要求：

- `code` 是稳定机器码；前端按 `message_key` 本地化。
- `detail` 不包含 SQL、栈、路径、密钥或资源隐藏信息。
- 验证错误使用 422；语法错误使用 400。
- 身份缺失/失效为 401，并适当返回 `WWW-Authenticate`。
- 已认证但无权为 403；为防资源枚举可按端点返回 404。
- 并发版本冲突和幂等参数冲突为 409。
- 限流为 429，并返回 `Retry-After`。
- 依赖临时不可用为 503。

## 6. 分页

大列表统一使用 cursor 分页，不以 offset 作为公共 API 主契约。

请求：

```text
GET /api/v1/boards/{id}/posts?limit=30&after=<opaque-cursor>&sort=latest
```

响应：

```json
{
  "items": [],
  "page": {
    "next_cursor": "opaque-or-null",
    "has_more": false
  }
}
```

规则：

- 默认 30，最大 100。
- Cursor 是签名/编码的不透明值，包含排序键和 ID，不允许客户端依赖内部格式。
- 排序必须有稳定 tie-breaker（通常 `id`）。
- 支持的排序和过滤字段由各端点枚举白名单定义。
- 管理员小型配置列表可显式使用 offset，但需在 OpenAPI 标记。

## 7. 幂等

以下操作必须接受 `Idempotency-Key`：

- 积分奖励、扣减、转账和调整。
- 付费内容解锁。
- OAuth Client secret 重置等高价值操作。
- 可能因网络重试重复执行的创建接口。

规则：

- Key 由客户端生成，长度与字符集受限。
- 服务端以 `用户/客户端 + 端点作用域 + key` 唯一。
- 相同 key + 相同请求摘要返回第一次结果。
- 相同 key + 不同请求返回 409 `idempotency_conflict`。
- 保存期限至少覆盖最大合理重试窗口；积分操作永久保留业务幂等记录。

## 8. 乐观并发

帖子、回复、角色和设置等可编辑资源返回：

- JSON `version`。
- 可选弱 ETag：`W/"<id>:<version>"`。

更新要求 `If-Match` 或请求体 `version`。版本不一致返回 409/412，不能最后写入者静默覆盖。

## 9. 缓存与条件请求

- 带 Session、私有信息、隐藏内容和管理响应：`Cache-Control: private, no-store`。
- 完全公开文章：可使用 `ETag`、`Last-Modified` 和短期 public cache。
- 个性化响应使用 `Vary: Cookie`；Bearer 投影使用 `Vary: Authorization`。
- 不允许缓存 OIDC token、userinfo、Session 和 CSRF 响应。
- 304 响应不能导致权限改变后继续使用旧隐藏正文；受限资源一律 no-store。

## 10. 内容投影

帖子 API 明确区分：

- `body_html`：当前请求方可见的公开内容。
- `restricted`：策略摘要，例如 `{ "kind": "after_reply", "unlocked": false }`。
- `restricted_html`：仅当 Rust 已判定可见时才出现。

禁止：

- 未解锁时返回加密/编码正文给前端。
- 在 excerpt、通知、搜索高亮和 OpenGraph 中泄漏受限正文。
- 管理员投影与普通用户投影共享公共缓存。

## 11. 过滤、排序和搜索

- 查询参数使用明确枚举，例如 `status=published`、`sort=latest`。
- 未知参数默认返回 400，避免拼写错误静默失效。
- 动态 SQL 列和方向只从枚举映射，不直接拼接用户输入。
- 搜索 query 限长并限流；搜索结果重新经过内容可见性过滤。
- 管理端导出使用异步 job，避免大响应拖垮小机器。

## 12. 上传 API

推荐两阶段：

```text
POST /api/v1/attachments                 创建上传/本地流式上传
POST /api/v1/attachments/{id}/complete   完成 S3 直传并触发服务端校验
GET  /api/v1/attachments/{id}            元数据
GET  /api/v1/attachments/{id}/content    鉴权下载或短期重定向
GET    /api/v1/users/{user_id}/profile-cover  获取资料 Cover（按用户资料可见性鉴权）
POST   /api/v1/me/profile-cover               创建/完成本人资料 Cover 附件引用
DELETE /api/v1/me/profile-cover               移除本人资料 Cover 引用
DELETE /api/v1/attachments/{id}           删除未引用附件
GET  /api/v1/attachments/{id}/download-policy  当前下载价格与授权状态
POST /api/v1/attachments/{id}/download         鉴权、必要时原子扣费并签发临时 URL
GET  /api/v1/download-authorizations/{id}      查询本人下载授权
POST /api/v1/download-authorizations/{id}/sign-url 重新鉴权签发 URL，不重复扣费
```

- 创建响应返回当前等级的 `max_file_bytes`、`total_bytes`、`used_bytes` 和 `remaining_bytes`。
- 超过单附件限制返回 413 `attachment_too_large`；超过用户总容量返回 409 `attachment_quota_exceeded`。错误可返回安全的数值配额，但不能泄漏其他用户信息。
- S3 可返回短期预签名上传参数，但完成后必须服务端校验对象。
- 创建响应仅返回限定对象 key 的短期上传参数、必要请求头和过期时间，绝不返回 S3 Access Key 或 Secret。
- 客户端调用 `complete` 后，Rust 必须执行 `HEAD`，校验对象存在、大小和约定元数据，再由 worker 流式读取 magic、hash 并完成图片处理；客户端提交的 ETag、Content-Type 和大小都不可信。
- `complete` 必须幂等：重复提交返回当前附件状态，不重复创建对象或链接。
- 本地存储由 Rust 流式接收，限制总大小和超时。
- `pending` 文件不能绑定到已发布正文；校验失败进入 `quarantined` 或清理临时对象。
- 私有附件下载先由 Rust 执行业务鉴权，再流式返回或 302 到短期签名 URL；签名 URL 不进入日志、审计 metadata 或长期缓存。
- 每次下载和签名 URL 签发都必须重新鉴权。响应返回临时 URL 的 `expires_at`；到期后该 URL 失效，但附件对象保持 `ready`，客户端可通过稳定 content 端点重新获取。
- `complete` 再次校验用户当前等级、对象实际大小和总容量，防止预签名后降级、并发上传或伪造大小绕过配额。

### 12.1 下载抵扣积分 API

下载计费完整安全语义见 [`DOWNLOAD-BILLING.md`](DOWNLOAD-BILLING.md)。`POST /api/v1/attachments/{id}/download` 强制使用 `Idempotency-Key`，请求方不得提交价格、货币或用户字段。余额扣减、不可变积分流水、下载授权、审计和 Outbox 在同一事务中提交；仅在提交后签发短期 URL。重复使用有效授权只重新签发 URL，不重复扣费。管理员配置接口为 `GET/PATCH /api/v1/admin/download-billing/config`，附件价格覆盖为 `GET/PATCH /api/v1/admin/attachments/{id}/download-policy`。

### 12.2 管理端存储配置 API

```text
GET   /api/v1/admin/storage/config       获取脱敏配置与来源
PATCH /api/v1/admin/storage/config       更新允许在线修改的配置
POST  /api/v1/admin/storage/test         测试候选或当前配置
GET   /api/v1/admin/levels/{id}/attachment-quota
PATCH /api/v1/admin/levels/{id}/attachment-quota
```

- 仅具备系统存储管理权限的管理员可调用，并要求 Session、CSRF、近期重新认证和审计。
- GET 只返回 `secret_configured`、配置来源、后端类型和脱敏连接状态，不返回 Secret。
- PATCH 中空 Secret 表示保持原值；更换 Secret 只接受写入，不提供读取接口。由环境变量或 Workload Identity 管理的字段为只读，更新返回 409 `managed_configuration`。
- `test` 可测试尚未保存的候选配置，但响应只包含稳定错误码和脱敏诊断，不回显凭证、内部对象 key 或签名 URL；测试对象使用专用前缀并立即清理。
- `signed_url_ttl_seconds` 控制 S3 临时公开链接有效期；修改后只影响新签发 URL，不删除或修改附件对象。
- 等级附件配额 PATCH 接受 `max_file_bytes` 和 `total_bytes`，要求总容量不小于单附件上限；修改后立即影响新上传并写管理员审计。
- 变更存储后端只保存候选配置，不自动切换已有对象；正式切换必须满足 `OPERATIONS.md` 的迁移与校验流程。

## 13. 公开市场交易 API

市场 API 的完整安全语义见 [`MARKETPLACE.md`](MARKETPLACE.md)。所有写接口要求短期 Bearer Token、专用 scope、`Idempotency-Key` 和 `Cache-Control: no-store`。

```text
POST /api/v1/marketplace/checkout-intents              从已登记 Offer 创建短效结账意图
POST /api/v1/marketplace/checkout-intents/{id}/confirm 用户在 BBLBB 托管页面确认并原子购买
GET  /api/v1/marketplace/purchases/{id}                 查询本 Client 的购买结果
GET  /api/v1/marketplace/purchases?after=...            cursor 增量对账
POST /api/v1/marketplace/purchases/{id}/refund          创建补偿退款

POST  /api/v1/marketplace/offers                        登记报价
PATCH /api/v1/marketplace/offers/{id}                   换版或禁用报价
GET   /api/v1/marketplace/offers/{id}                   读取本 Client 报价

GET   /api/v1/admin/marketplace/clients                 管理市场 Client
PATCH /api/v1/admin/marketplace/clients/{id}            审批 scope、限额或紧急禁用
GET   /api/v1/admin/marketplace/transactions            只读交易与对账视图
POST  /api/v1/admin/marketplace/clients/{id}/rotate-webhook-secret
```

关键契约：

- 创建意图只接受 `offer_id`、`expected_offer_version`、`merchant_order_id` 和安全展示所需数量；响应金额来自服务端 Offer 快照。
- 确认接口同时要求当前用户 Session + CSRF 和与该交互绑定的市场授权；Client 后台不能代替用户确认。若采用一次性 interaction exchange，OpenAPI 必须保持同等绑定语义。
- `201/200 succeeded` 仅在购买、意图消费、账户扣款、流水、审计和 Outbox 已同事务提交后返回。
- 余额不足使用 409 `insufficient_funds`；意图过期使用 409 `checkout_intent_expired`；已被其他请求消费且幂等键不同使用 409 `checkout_intent_consumed`；Offer 换版使用 409 `offer_version_changed`。
- 网络超时后调用方用原幂等键重试或查询购买；服务端不得把未知提交状态映射为新购买。
- 市场响应不返回用户完整余额或内部用户 ID；用户确认页可向当前用户显示扣款前后余额。
- 退款是引用原购买的 `reversal`，累计金额不得超过原交易；不能 UPDATE/DELETE 原流水。
- Webhook 由提交后的 Outbox 异步签名投递，可能重复、延迟和乱序；`purchase_id` 查询是对账事实来源。

## 14. 权限与字段隐私

- 端点 OpenAPI 描述所需权限。
- 列表和详情使用同一可见性策略。
- 用户公开 DTO 不包含 email、Session、处罚内部备注等字段。
- `/me` 与管理员 DTO 使用不同 schema，不用一个巨型 User 对象后再随意删字段。
- OAuth claim DTO 独立于论坛用户 DTO。

## 15. AI 辅助 API

完整安全语义见 [`AI.md`](AI.md)。

```text
GET  /api/v1/ai/capabilities
POST /api/v1/ai/drafts/{draft_id}/format
POST /api/v1/ai/posts/{post_id}/moderation-suggestion
POST /api/v1/ai/posts/{post_id}/seo-suggestion
GET  /api/v1/ai/tasks/{id}
POST /api/v1/ai/tasks/{id}/cancel
GET  /api/v1/ai/suggestions/{id}
POST /api/v1/ai/suggestions/{id}/accept
POST /api/v1/ai/consent
DELETE /api/v1/ai/consent
GET/PATCH /api/v1/admin/ai/config
POST /api/v1/admin/ai/providers/test
GET /api/v1/admin/ai/tasks
```

AI 结果只作为版本化 suggestion；采纳必须再次鉴权并使用版本/If-Match，不能直接覆盖用户新编辑。AI Provider 超时返回任务状态或安全错误，不放行核心审核策略。

## 16. 视频嵌入 API

视频插件完整安全语义见 [`VIDEO-PLUGIN.md`](VIDEO-PLUGIN.md)。

```text
POST /api/v1/video-embeds/resolve
POST /api/v1/video-embeds
PATCH /api/v1/video-embeds/{id}
POST /api/v1/video-embeds/{id}/refresh
DELETE /api/v1/video-embeds/{id}
GET /api/v1/video-embeds/{id}
GET   /api/v1/admin/video/policies
GET   /api/v1/admin/video/policies/{provider}
PATCH /api/v1/admin/video/policies/{provider}
POST /api/v1/admin/video/policies/test
```

解析、创建和刷新接口均由 Rust 执行来源白名单、SSRF/DNS 重绑定、HLS 深度/分片/字节预算、CSP 来源和帖子权限校验；请求不能提交任意 iframe HTML、可信 MIME、HLS Key 或平台签名 URL。

## 17. 限流响应

返回：

```text
RateLimit-Limit
RateLimit-Remaining
RateLimit-Reset
Retry-After
```

具体值可以按用户等级/处罚动态变化；响应不得暴露内部风控规则细节。

## 18. Endpoint 领域分组

v1 预计：

```text
/api/v1/auth/*
/api/v1/me/*
/api/v1/users/*
/api/v1/boards/*
/api/v1/posts/*
/api/v1/comments/*
/api/v1/tags/*
/api/v1/tags/{slug}/posts
/api/v1/attachments/*
/api/v1/notifications/*
/api/v1/marketplace/offers/*
/api/v1/marketplace/checkout-intents/*
/api/v1/marketplace/purchases/*
/api/v1/admin/users/*
/api/v1/admin/roles/*
/api/v1/admin/moderation/*
/api/v1/admin/economy/*
/api/v1/admin/themes/*
/api/v1/admin/plugins/*
/api/v1/admin/settings/*
/api/v1/admin/storage/*
/api/v1/admin/download-billing/*
/api/v1/admin/ai/*
/api/v1/admin/video/*
/api/v1/ai/*
/api/v1/video-embeds/*
/api/v1/download-authorizations/*
/api/v1/shop/*
/api/v1/me/entitlements/*
/api/v1/activity/*
/api/v1/posts/*/reactions
/api/v1/comments/*/reactions
/api/v1/admin/oauth-clients/*
/api/v1/admin/marketplace/*
/api/v1/oauth/interactions/{id}
/api/v1/oauth/interactions/{id}/decision
/api/v1/users/{username}/follow
/api/v1/users/{username}/followers
/api/v1/users/{username}/following
/api/v1/boards/{slug}/follow
/api/v1/posts/{id}/favorite
/api/v1/me/favorites
/api/v1/me/following
/api/v1/conversations/*
/api/v1/achievements
/api/v1/me/achievements/{code}/equip
/api/v1/me/api-keys/*
/api/v1/me/password
/api/v1/me/sanctions
/api/v1/me/point-transactions
/api/v1/me/oauth-grants/*
/api/v1/posts/{id}/unlock
/api/v1/stats
/api/v1/rss
/api/v1/atom
/api/v1/site
```

端点详情由实现中的 OpenAPI 提供；文档和 schema 不一致时 CI 失败。

## 19. 兼容与废弃

- 新增可选响应字段属于兼容变更；客户端必须忽略未知字段。
- 新增必填请求字段、改变枚举语义或删除字段属于破坏性变更。
- 废弃字段先在 OpenAPI 标记并至少保留一个受支持小版本周期。
- 安全紧急变更可提前停止危险行为，但需发布迁移说明。
- 对外插件事件和 OIDC claim 分别维护独立版本，不与内部 Rust 类型绑定。

## 20. M18 原型对齐扩展端点与参数

在 2026-09-04 原型全站对比（`reports/mobile-compare/REPORT.md`）后，补充以下端点与参数：

- **标签聚合**：`GET /api/v1/tags/{slug}/posts`（公开；published + 未删除；created_at 游标分页；投影含 board 与作者摘要）。
- **板块帖子扩展**：`GET /api/v1/boards/{slug}/posts` 扩展 `sort=featured`（精华优先）、`sort=unanswered`（无回复过滤）与 `q`（标题/作者模糊搜索，LIKE + ESCAPE '!'）。
- **首页帖子流扩展**：`GET /api/v1/posts` 列表项投影增加 `like_count`（post_reactions 聚合）；扩展 `sort=following`（当前登录用户关注的作者，未登录返回空数组）。
- **内部管理运营端点登记**（documented non-contract，`scripts/check-route-coverage.rb` 的 `DOCUMENTED_NON_CONTRACT` 注册表）：站点统计 `GET /api/v1/admin/stats`、趋势 `GET /api/v1/admin/stats/trend`、BI 指标 `GET /api/v1/admin/bi/metrics`、审计读取 `GET /api/v1/admin/audit-logs`、系统设置 `GET/PATCH /api/v1/admin/settings`、帖子管理 `GET /api/v1/admin/posts` 与 `POST /api/v1/admin/posts/{id}/action`、通知广播与召回 `GET/POST /api/v1/admin/notifications/*`、通知模板 `GET /api/v1/admin/notifications/templates`、角色分配 `POST/DELETE /api/v1/admin/users/{id}/roles*`、成就管理 `GET/POST/PATCH/DELETE /api/v1/admin/achievements*` 与手工授予 `POST /api/v1/admin/achievements/{code}/grant`、积分流水与调整 `GET /api/v1/admin/points/ledger`、`POST /api/v1/admin/points/adjust`、等级规则 `GET /api/v1/admin/levels`、`PATCH /api/v1/admin/levels/{level}`、附件管理 `GET /api/v1/admin/attachments`、`DELETE /api/v1/admin/attachments/{id}`、下载交易 `GET /api/v1/admin/download-billing/transactions`、标签合并 `POST /api/v1/admin/tags/{id}/merge`。这些为运营管理接口（沿用 M12/M13 Marketplace/Plugin 先例），安全语义集中记录于 [`OPERATIONS.md §19.8`](OPERATIONS.md)；不进入冻结契约，也不得被公开客户端依赖。
- **Feed/SEO 投影端点**：`GET /api/v1/rss`、`GET /api/v1/atom`、`GET /api/v1/sitemap.xml` 与 `GET /robots.txt` 为公开只读投影，内容、缓存与 `X-Robots-Tag` 策略见 [`CRAWLER-POLICY.md §7`](CRAWLER-POLICY.md)。
- **站点公开信息端点**（documented non-contract，`scripts/check-route-coverage.rb` 的 `DOCUMENTED_NON_CONTRACT` 注册表）：`GET /api/v1/site`（匿名可读，迁移 0065）返回站点名称/描述、登录页与注册页文案（空串 = 前端内置通用文案兜底）及 `maintenance_mode` 公开标记，是全站文案统一（管理台「系统设置 → 站点文案」）的前台只读投影；SMTP、注册开关等运营字段不进该投影（留在 `GET /api/v1/admin/settings`）。响应 `private, no-store`；供第一方 SSR/前端使用，不作为冻结契约被第三方客户端依赖。

## 21. M17-GAPFIX 社交、经济与个人域端点

原型差距补齐（GAP-FIX）引入的社交/经济/个人域端点全部进入冻结契约（迁移 0060/0062），实现与测试证据见 [`../todo/OPENAPI-COVERAGE.md`](../todo/OPENAPI-COVERAGE.md)（M17-GAPFIX 工作包）。

### 21.1 关注与收藏（toggle 语义）

```text
POST   /api/v1/users/{username}/follow     关注用户 → 200 {following, followers}
DELETE /api/v1/users/{username}/follow     取关 → 200 {following, followers}
GET    /api/v1/users/{username}/followers   粉丝列表（keyset created_at DESC）
GET    /api/v1/users/{username}/following   关注列表（keyset created_at DESC）
POST   /api/v1/boards/{slug}/follow        关注板块 → 200 {following}
DELETE /api/v1/boards/{slug}/follow        取关板块 → 200 {following}
GET    /api/v1/me/following                本人关注汇总 {users[], boards[]}（各 limit 200）
POST   /api/v1/posts/{id}/favorite         收藏 → 200 {favorited, favorite_count}
DELETE /api/v1/posts/{id}/favorite         取消收藏 → 200 {favorited, favorite_count}
GET    /api/v1/me/favorites                本人收藏列表（keyset favorites.created_at DESC）
```

- 权限：全部 `authenticated` + Session CSRF；列表端点对未登录 401。
- **toggle 幂等不使用 `Idempotency-Key`**：`favorites(user_id, post_id)`、`user_follows`、`board_follows` 复合主键即天然幂等键——重复 POST/DELETE 返回当前态与计数，不产生重复行；body 的 `client_request_id` 按契约接收但不参与判定。
- 关注自己返回 422 `cannot_follow_self`；目标用户/板块/帖子不存在返回 404。
- 只允许收藏 `published`/`hidden` 且未删除的帖子（与评论创建路径同一可见性判定），不可收藏时 404，不泄露存在性。
- 新关注成功后对被关注者 best-effort 触发成就钩子（见 `backend/src/achievements/`），失败不影响关注结果。

### 21.2 私信（Conversations）

```text
GET  /api/v1/conversations                       本人会话列表（last_message_at DESC keyset）
POST /api/v1/conversations                        与指定用户开（或复用）会话 → 201 {id, other}
GET  /api/v1/conversations/{id}/messages          消息线程（created_at ASC，after = 上一页最后一条）
POST /api/v1/conversations/{id}/messages          发消息 → 201
POST /api/v1/conversations/{id}/read              标记已读（本人 last_read_at）→ 204
```

- 权限：`authenticated` + CSRF；**仅会话参与者可读写**，非参与者与不存在一律 404（不枚举会话 ID）。
- 与自己开会话 422；目标用户不存在 404。创建语义为 find-or-create：两人已有会话时返回既有 `id`。
- 发消息走 `client_request_id` + `idempotency_records` 幂等（scope `conversation.message`）：同 key + 相同请求摘要重放返回原消息；同 key 不同摘要返回 409 `idempotency_conflict`。
- 消息落库后给对方插入 `type='mention'`、`link='/messages'` 的通知（best-effort，不阻断发送）。
- 会话列表只含双人会话，`other` 为对方公开投影；不返回对方 email 等私有字段。

### 21.3 成就与徽章装备

```text
GET    /api/v1/achievements                     公共成就目录（匿名可读；隐藏成就 description 脱敏）
GET    /api/v1/me/achievements                  本人成就视图（解锁状态/进度/装备位 + stats 汇总）
PUT    /api/v1/me/achievements/{code}/equip     装备徽章（仅已解锁；超过 3 槽 → 409）
DELETE /api/v1/me/achievements/{code}/equip     卸下徽章
```

- 目录 `public`；本人视图与装备操作 `authenticated` + CSRF。
- 装备位上限 3（`MAX_EQUIPPED_SLOTS`），超出返回 409；装备尚未解锁的成就同样返回 409。
- 管理侧目录/新建/更新（If-Match version）/删除与手工授予（`POST /api/v1/admin/achievements/{code}/grant`）为 documented non-contract 端点（§20）；`manual` 类成就唯一授予来源，复用统一 unlock 路径并发徽章通知。

### 21.4 个人 API 密钥

```text
GET    /api/v1/me/api-keys           本人密钥列表（仅 prefix，不含明文）
POST  /api/v1/me/api-keys            创建 → 201（明文 key 仅此一次返回）
DELETE /api/v1/me/api-keys/{id}      软删除（revoked_at 置位，立即失效）→ 204
```

- 权限：`authenticated` + CSRF。
- 明文格式 `bblbb_<32 位 base62>`（`OsRng` 熵，与 auth token 同源）；数据库只存 SHA-256，`prefix` = 明文前 12 字符供人工识别。
- scopes 白名单：`posts:read | drafts:write | notifications:read | me:read` 的子集。
- 创建走 `client_request_id` 幂等（scope `apikey.create`）；同 key 重放返回 409 并说明「密钥已创建，明文仅首次返回」——明文不可二次出示，也不产生重复行。
- 密钥仅用于机器对机器读取场景，不能替代 Session 完成写操作或提权。

### 21.5 个人域（账号与账务投影）

```text
POST   /api/v1/me/password                     修改密码（校验当前密码）
GET    /api/v1/me/sanctions                     本人处罚记录（sanctions 表投影）
GET    /api/v1/me/point-transactions            本人积分流水（keyset created_at DESC）
GET    /api/v1/me/oauth-grants                  本人 OAuth 授权列表
DELETE /api/v1/me/oauth-grants/{client_id}      撤销本人对某 Client 的授权
```

- 权限：`authenticated` + CSRF；全部仅返回本人数据，不暴露他人余额/邮箱/Session。
- 修改密码成功后吊销其他设备 Session（当前设备保留）；当前密码错误 401。
- 处罚投影来自 `sanctions` 表（用户被处罚记录），不是案件维度的 `moderation_actions` 审核日志；`expires_at` 映射 `ends_at`。
- OAuth 授权撤销只影响后续 token 签发，不追溯已签发 token 的剩余寿命（以 OIDC 撤销端点为准）。

### 21.6 付费内容解锁

```text
POST /api/v1/posts/{id}/unlock     解锁 paid 策略帖子
```

- 权限：`authenticated` + CSRF；定价权威来源为 `posts.price_coin`（金币计价），缺失时回退 `content_access_policies.amount`。
- 已解锁重复调用幂等返回 200，不重复扣费；余额不足返回 409 `insufficient_funds`。
- 扣费充足时在同一事务内完成：ledger `consume`（source_type=`post_unlock`，幂等 scope `post.unlock`）+ `content_access_grants` purchase grant + 通知作者；仅在提交后才返回解锁成功。
- 解锁后通过既有帖子投影（`restricted.unlocked=true`）读取正文；本端点不返回正文本身。

### 21.7 站点统计

```text
GET /api/v1/stats     公开站点统计（成员/帖子/板块等聚合计数）
```

- 权限：`public`；`Cache-Control: public, max-age=60`。
- 只返回聚合数字（成员、帖子、评论、板块、标签计数），不含任何用户级明细；计数口径与运营端点 `GET /api/v1/admin/stats`（non-contract）一致，但投影更少。

### 21.8 Feed 投影

`GET /api/v1/rss` 与 `GET /api/v1/atom` 为公开只读内容订阅投影，排序、缓存与防泄漏规则遵循 [`CRAWLER-POLICY.md §7.1`](CRAWLER-POLICY.md)（`published_at DESC, id DESC` 稳定排序；受限/未发布正文不出现在投影中）。

