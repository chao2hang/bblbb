# BBLBB — HTTP API 契约

> 版本：v0.3
> OpenAPI 是接口事实来源；本文规定所有 API 必须遵守的跨端点规则。

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
DELETE /api/v1/attachments/{id}           删除未引用附件
```

- 创建请求必须包含或由服务端推导附件期限；响应返回权威 `expires_at`、当前等级的 `max_file_bytes`、`total_bytes`、`used_bytes` 和 `remaining_bytes`。
- 所有附件的 `expires_at` 必填且有限；客户端请求的期限超过站点、等级、用途或板块上限时返回 422 `attachment_ttl_exceeded`。
- 超过单附件限制返回 413 `attachment_too_large`；超过用户总容量返回 409 `attachment_quota_exceeded`。错误可返回安全的数值配额，但不能泄漏其他用户信息。
- S3 可返回短期预签名上传参数，但完成后必须服务端校验对象。
- 创建响应仅返回限定对象 key 的短期上传参数、必要请求头和过期时间，绝不返回 S3 Access Key 或 Secret。
- 客户端调用 `complete` 后，Rust 必须执行 `HEAD`，校验对象存在、大小和约定元数据，再由 worker 流式读取 magic、hash 并完成图片处理；客户端提交的 ETag、Content-Type 和大小都不可信。
- `complete` 必须幂等：重复提交返回当前附件状态，不重复创建对象或链接。
- 本地存储由 Rust 流式接收，限制总大小和超时。
- `pending` 文件不能绑定到已发布正文；校验失败进入 `quarantined` 或清理临时对象。
- 私有附件下载先由 Rust 执行业务鉴权，再流式返回或 302 到短期签名 URL；签名 URL 不进入日志、审计 metadata 或长期缓存。
- 下载和签名 URL 签发必须实时检查 `expires_at`；已过期返回 410 `attachment_expired`。签名 TTL 必须小于附件剩余有效期。
- `complete` 再次校验用户当前等级、对象实际大小和总容量，防止预签名后降级、并发上传或伪造大小绕过配额。

### 12.1 管理端存储配置 API

```text
GET   /api/v1/admin/storage/config       获取脱敏配置与来源
PATCH /api/v1/admin/storage/config       更新允许在线修改的配置
POST  /api/v1/admin/storage/test         测试候选或当前配置
```

- 仅具备系统存储管理权限的管理员可调用，并要求 Session、CSRF、近期重新认证和审计。
- GET 只返回 `secret_configured`、配置来源、后端类型和脱敏连接状态，不返回 Secret。
- PATCH 中空 Secret 表示保持原值；更换 Secret 只接受写入，不提供读取接口。由环境变量或 Workload Identity 管理的字段为只读，更新返回 409 `managed_configuration`。
- `test` 可测试尚未保存的候选配置，但响应只包含稳定错误码和脱敏诊断，不回显凭证、内部对象 key 或签名 URL；测试对象使用专用前缀并立即清理。
- 变更存储后端只保存候选配置，不自动切换已有对象；正式切换必须满足 `OPERATIONS.md` 的迁移与校验流程。

## 13. 权限与字段隐私

- 端点 OpenAPI 描述所需权限。
- 列表和详情使用同一可见性策略。
- 用户公开 DTO 不包含 email、Session、处罚内部备注等字段。
- `/me` 与管理员 DTO 使用不同 schema，不用一个巨型 User 对象后再随意删字段。
- OAuth claim DTO 独立于论坛用户 DTO。

## 14. 限流响应

返回：

```text
RateLimit-Limit
RateLimit-Remaining
RateLimit-Reset
Retry-After
```

具体值可以按用户等级/处罚动态变化；响应不得暴露内部风控规则细节。

## 15. Endpoint 领域分组

v1 预计：

```text
/api/v1/auth/*
/api/v1/me/*
/api/v1/users/*
/api/v1/boards/*
/api/v1/posts/*
/api/v1/comments/*
/api/v1/tags/*
/api/v1/attachments/*
/api/v1/notifications/*
/api/v1/admin/users/*
/api/v1/admin/roles/*
/api/v1/admin/moderation/*
/api/v1/admin/economy/*
/api/v1/admin/themes/*
/api/v1/admin/plugins/*
/api/v1/admin/settings/*
/api/v1/admin/storage/*
/api/v1/admin/oauth-clients/*
/api/v1/oauth/interactions/{id}
/api/v1/oauth/interactions/{id}/decision
```

端点详情由实现中的 OpenAPI 提供；文档和 schema 不一致时 CI 失败。

## 16. 兼容与废弃

- 新增可选响应字段属于兼容变更；客户端必须忽略未知字段。
- 新增必填请求字段、改变枚举语义或删除字段属于破坏性变更。
- 废弃字段先在 OpenAPI 标记并至少保留一个受支持小版本周期。
- 安全紧急变更可提前停止危险行为，但需发布迁移说明。
- 对外插件事件和 OIDC claim 分别维护独立版本，不与内部 Rust 类型绑定。
