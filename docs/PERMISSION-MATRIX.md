# BBLBB — Endpoint、Scope 与权限矩阵

> 基线：v0.4。Scope 只限制 OAuth Token 能代表 Client 请求什么，不能提升用户自身 RBAC/Object 权限。Session 写请求均要求 CSRF；Bearer-only 请求不使用 Cookie 时不要求 CSRF。

## 1. 身份和标记

- `S`：登录 Session。
- `B`：短期 Bearer Token。
- `O`：对象级作者/所有者/板块范围判断。
- `R`：近期重新认证；高风险管理员可要求 TOTP。
- 所有 `admin.*` 写操作都记录 reason、actor、request ID、前后策略版本。

## 2. 核心论坛与附件

| 动作/Endpoint 组 | 身份 | OAuth Scope | Permission | 对象/额外规则 | CSRF | 审计 |
|---|---|---|---|---|---|---|
| 读取公开帖子/板块 | 匿名/S/B | `content.read` | `post.read` | 板块、状态、访问策略 | 写时否 | 仅敏感访问 |
| 创建帖子/评论 | S/B | `content.write` | `post.create` / `comment.create` | sanction、板块、限流 | S 是 | 业务事件 |
| 编辑/删除本人内容 | S/B | `content.write` | `post.edit_own` / `comment.edit_own` | O + version | S 是 | 删除/恢复记录 |
| 管理他人内容 | S | — | `post.moderate` | 板块范围 + reason | 是 | 必须 |
| 创建/完成附件 | S/B | `attachment.write` | `attachment.upload` | 所有者、配额、MIME | S 是 | 安全结果 |
| 读取附件/签发 URL | S/B | `attachment.read` | `attachment.read` | 引用内容可见性 | 否 | 受限下载 |
| 管理存储配置 | S | — | `storage.manage` | R + Secret 不回显 | 是 | 必须 |
| 修改等级附件配额 | S | — | `level.manage` | R + policy version | 是 | 必须 |

## 3. 下载计费和经济

| 动作 | 身份 | OAuth Scope | Permission | 额外规则 | CSRF | 审计 |
|---|---|---|---|---|---|---|
| 查询本人下载策略/授权 | S/B | `download.read` | `download.read` | 仅本人、附件可见 | S 写时是 | 最小 |
| 创建下载授权 | S/B | `download.create` | `download.create` | Idempotency-Key、后端定价 | S 是 | 扣费/免费授权 |
| 查询本人下载流水 | S/B | `download.read` | `download.read_own` | 不返回他人余额 | 否 | 否 |
| 管理下载计费策略 | S | — | `download_billing.manage` | R + reason + version | 是 | 必须 |
| 管理附件价格覆盖 | S | — | `download_billing.manage` | 附件/板块范围 | 是 | 必须 |
| 管理员积分调整 | S | — | `points.adjust` | R + reason + 双重确认 | 是 | 必须 |

## 4. Marketplace

| 动作 | 身份 | OAuth Scope | Permission | 额外规则 | CSRF | 审计 |
|---|---|---|---|---|---|---|
| 创建/修改自己的 Offer | B | `marketplace.offer.write` | `marketplace_offer.manage_own` | Client owner + 已批准 | 否 | 必须 |
| 创建 Checkout Intent | B | `marketplace.checkout.create` | `marketplace_purchase.create` | user-bound Token | 否 | 意图摘要 |
| 托管确认购买 | S | 绑定 interaction | `marketplace_purchase.confirm_own` | Session user=Intent user；CSRF | 是 | 必须 |
| 查询本 Client Purchase | B | `marketplace.purchases.read` | `marketplace_purchase.read_own` | Client 隔离 | 否 | 批量查询限流 |
| 请求退款 | B | `marketplace.refund` | `marketplace_refund.create_own` | Client service credential + 原 Purchase | 否 | 必须 |
| 管理/审批 Client | S | — | `marketplace.manage` | R + reason | 是 | 必须 |
| 管理员强制退款 | S | — | `marketplace.refund_admin` | R + reason + 限额 | 是 | 必须 |
| 轮换 Webhook Secret | S/B | 管理端不用 Scope | `marketplace_secret.rotate` | Owner 或管理员、R | S 是 | 必须 |
| 重放 Webhook | S/B | `marketplace.webhook.manage` | `marketplace_webhook.replay` | 保持 event_id | S 是 | 必须 |

- Client service credential 仅用于 Offer、查询、退款和 Webhook 管理；不能创建绑定任意用户的 Checkout Intent，不能读取用户余额。
- Checkout Intent 必须使用由用户授权码流程签发的 user-bound Token 创建。

## 5. AI

| 动作 | 身份 | OAuth Scope | Permission | 额外规则 | CSRF | 审计 |
|---|---|---|---|---|---|---|
| 格式化本人草稿 | S/B | `ai.format` | `ai.format` | 草稿所有者、同意/数据策略 | S 是 | 最小 AI 审计 |
| 生成公开帖 SEO | S/B | `ai.seo` | `ai.seo` | 作者/编辑权限、公开 revision | S 是 | AI 审计 |
| 请求审核建议 | S | — | `ai.moderation_request` | 审核范围 | 是 | 必须 |
| 接受 Suggestion | S/B | 对应用途 Scope | 对应编辑/审核权限 | base_version | S 是 | 必须 |
| 管理同意 | S | — | `ai.consent_own` | 仅本人 | 是 | 同意证据 |
| 管理 Provider/Policy | S | — | `ai.manage` | R + reason | 是 | 必须 |
| 重试/取消管理任务 | S | — | `ai.task.manage` | 不能绕过同意 | 是 | 必须 |

## 6. Internal Shop 与活跃

| 动作 | 身份 | OAuth Scope | Permission | 额外规则 | CSRF | 审计 |
|---|---|---|---|---|---|---|
| 浏览商品 | 匿名/S | `shop.read` | `shop.read` | 仅 published 商品 | 否 | 否 |
| 购买商品 | S | — | `shop.purchase` | 后端价格/库存/余额，Idempotency-Key | 是 | 必须 |
| 查看/装备/卸下持有物 | S | — | `shop.entitlement.manage_own` | 只能本人、槽位互斥 | 是 | 装备记录 |
| 领取签到/活跃奖励 | S | — | `activity.claim_own` | 日限额、去重、风控 | 是 | 必须 |
| 使用互动反应 | S | — | `reaction.create` | 目标可见、频率和数量限制 | 是 | 可配置 |
| 管理商品与库存 | S | — | `shop.manage` | R + reason + 审核状态 | 是 | 必须 |
| 管理活跃规则 | S | — | `activity.manage` | R + version + 预览 | 是 | 必须 |
| 管理商城退款 | S | — | `shop.refund` | R + 补偿账本，不能改历史 | 是 | 必须 |

## 7. Video

| 动作 | 身份 | OAuth Scope | Permission | 额外规则 | CSRF | 审计 |
|---|---|---|---|---|---|---|
| Resolve 视频 URL | S/B | `video.write` | `video.embed` | 发帖资格、egress 策略 | S 是 | 安全结果 |
| 创建/修改/删除 Embed | S/B | `video.write` | `video.embed` | 目标内容编辑权限 | S 是 | 来源和策略版本 |
| 刷新本人 Embed | S/B | `video.write` | `video.embed` | 所有者/编辑权限、限流 | S 是 | 任务 |
| 管理 Provider Policy | S | — | `video.manage` | R + CSP/egress 校验 | 是 | 必须 |
| 执行策略测试 | S | — | `video.manage` | 固定安全探针 | 是 | 必须 |

## 7. 高风险要求

- `storage.manage`、`download_billing.manage`、`marketplace.manage`、`marketplace.refund_admin`、`ai.manage`、`video.manage`、OIDC 密钥操作必须近期重新认证。
- Secret 创建/轮换只显示一次；读取接口只返回 `secret_configured` 和轮换时间。
- 用户被封禁、Client 停用、Consent 撤销、Policy 关闭后，缓存中的旧 Scope 不能继续执行敏感操作；Handler 必须实时查关键状态。
