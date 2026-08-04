# BBLBB — 授权模型

> 版本：v0.4
> BBLBB 使用 RBAC 表达“谁通常能做什么”，再使用对象级策略表达作者、板块范围、内容状态和处罚。权限最终由 Rust 后端裁决。

## 1. 概念

- **Permission**：稳定动作名，例如 `post.edit_any`。
- **Role**：Permission 集合，例如 moderator。
- **Global assignment**：用户在全站拥有角色。
- **Board assignment**：用户只在某板块拥有角色。
- **Ownership policy**：作者可操作自己的对象，但受状态、时限和处罚限制。
- **Level benefit**：额度或低风险能力，不是管理员权限。
- **Sanction**：对用户施加的显式限制，优先于一般允许。

## 2. 权限命名

统一使用 `resource.action`，不用在字符串中拼接对象 ID：

```text
board.view_restricted
board.create
board.update
board.delete
post.create
post.edit_own
post.edit_any
post.delete_own
post.delete_any
post.moderate
comment.create
comment.edit_own
comment.delete_any
user.view_private
user.manage
role.manage
moderation.review
moderation.sanction
points.view_ledger
points.adjust
settings.manage
plugin.manage
theme.manage
oauth_client.manage
marketplace_client.manage
marketplace_offer.manage
marketplace_purchase.read_own
marketplace_refund.create
marketplace.manage
marketplace.refund_admin
marketplace_secret.rotate
marketplace_webhook.replay
storage.manage
download.read
download.read_own
download.create
download_billing.manage
attachment.upload
attachment.read
level.manage
ai.format
ai.seo
ai.moderation_request
ai.consent_own
ai.manage
ai.task.manage
video.embed
video.manage
```

对象范围来自 assignment 和请求资源，不放入 permission 名称。

## 3. 内置角色

### `member`

- 查看公开/会员内容。
- 在允许板块发帖和回复。
- 编辑自己的内容（受时间和审核状态限制）。
- 举报、收藏、反应。

### `moderator`

- 查看分配板块的审核队列。
- 隐藏、恢复、移动、关闭该范围内容。
- 施加板块级禁言。
- 查看处理案件所需的有限用户信息。

### `administrator`

- 全站配置、角色、用户、积分、插件、主题和 OIDC Client 管理。
- 仍受审计、CSRF、二次确认和不可变账本约束。
- 不能通过普通接口删除审计或改写账本。

内置角色不可删除，但权限集合可在安全边界内配置；必须始终至少有一个可恢复系统的管理员。

## 4. 作用域

- `user_roles` 只存全局 assignment。
- `board_role_assignments` 只存板块 assignment。
- 板块角色默认只对当前板块生效，不自动继承到子板块。
- 若未来支持继承，在板块配置中显式开启并在权限解释接口中显示来源。
- 全局角色自然覆盖所有板块，但对象状态和显式 sanction 仍参与判断。

## 5. 判定顺序

```text
1. 验证身份与账号状态
2. 检查有效 sanction / 站点维护模式
3. 检查对象是否存在及基础可见性
4. 收集全局角色权限
5. 收集当前板块 assignment 权限
6. 应用 ownership policy
7. 应用低风险 level benefits / 配额
8. 检查业务状态规则（锁帖、审核中、编辑窗口等）
9. 输出 allow/deny + reason code
```

原则：

- 显式处罚/安全限制优先于普通 allow。
- Administrator 不是数据库 superuser；涉及账本、审计和密钥仍走专用流程。
- 等级不能授予 `user.manage`、`role.manage`、`points.adjust` 等敏感权限。
- UI 可根据结果隐藏按钮，但 handler 必须重新判定。

## 6. 对象策略示例

### 编辑帖子

允许条件之一：

- 作者拥有 `post.edit_own`，帖子在编辑窗口内、未被锁定且不处于不可编辑审核状态。
- 当前板块 moderator 拥有 `post.edit_any`。
- 全局 administrator 拥有 `post.edit_any`。

管理员/版主编辑他人内容时要求原因并写 revision + moderation action。

### 查看限制板块

- `public`：匿名可见。
- `members`：有效已登录用户可见。
- `restricted`：需要 `board.view_restricted` 的适用角色。
- `hidden`：列表中不出现；只有适用管理权限可访问。

### 市场购买与退款

- `marketplace.purchase` 是外部 Token scope，不等价于后台 Permission；同时要求 Client 已批准、用户已单独同意、Checkout Intent 绑定当前用户且实时策略允许。
- 市场所有者只能管理自己的 Offer、读取自己的 Purchase 和申请自己的退款；`marketplace.manage` 才能跨市场审计或紧急禁用。
- 市场 Client 不能获得 `points.adjust`，也不能直接调用通用积分扣款接口。购买只能经过市场交易 application service。
- 用户处罚、账户冻结、Client 禁用、余额/限额不足优先拒绝，即使 Token 和 consent 尚未过期。

### 查看受限正文

- 作者本人可见。
- 有适用 `post.moderate` 的审核员可审计查看。
- 用户具有未撤销 `content_access_grants`。
- 策略为等级门槛且用户达到要求。

即使有 URL，未授权响应也不含正文。

## 7. 等级权益

允许作为等级 benefit：

- 每日发帖/回复额度。
- 单附件最大字节数、附件总容量和附件数量。
- 签名长度。
- 可使用的普通表情/样式。
- 某些普通板块的发帖门槛。

禁止作为等级 benefit：

- 用户/角色管理。
- 查看私有邮箱。
- 审核、处罚和账本调整。
- 插件、主题、OIDC Client 或密钥管理。
- 绕过封禁或内容法律限制。

## 8. 处罚交互

- `mute`：禁止创建帖子/回复，不影响阅读和申诉。
- `board_mute`：只禁止指定板块写入。
- `rate_limit`：覆盖通常等级额度。
- `ban`：撤销 Session/Refresh Token 并禁止登录，保留申诉通道可由一次性安全链接实现。
- 处罚到期由查询实时判断，后台任务用于状态整理和通知，不能仅依赖任务准时运行。

## 9. 授权查询与缓存

- 对象列表必须把板块可见性和账号状态编入查询，避免大量结果回传后过滤。
- 角色权限可按 `authorization_revision` 进程内缓存。
- 角色、assignment、处罚变更增加 revision 并立即失效本实例缓存。
- v1 单实例不需 Redis；多实例必须增加分布式失效或短 TTL + 强制敏感操作查库。
- 管理员权限变更和封禁不能被长缓存延迟。

## 10. 管理与安全保护

- 首次安装通过一次性 bootstrap token 创建首个管理员，完成后永久禁用 bootstrap。
- 禁止删除/降级最后一个有效管理员。
- 角色修改显示受影响用户数量和权限差异。
- 敏感权限授予、管理员创建和最后管理员变更需要重新验证密码/TOTP。
- 所有 assignment 包含授权人、时间和可选到期时间。
- 临时版主自动过期，判定时实时检查 `expires_at`。

## 11. 权限解释

后台和 API 应提供安全的“为什么允许/拒绝”结果：

```json
{
  "allowed": false,
  "reason": "active_board_mute",
  "sources": ["role:member", "sanction:019..."]
}
```

普通用户只得到安全 reason code；完整来源只对授权管理员可见并写审计，避免泄漏隐藏角色或案件。

## 12. 测试要求

- 每个 permission 至少一个 allow 与 deny 测试。
- 覆盖匿名、member、板块 moderator、全局 moderator、administrator。
- 覆盖拥有者、锁定、审核、删除、隐藏板块、处罚和过期 assignment。
- 使用属性测试验证“增加普通等级不得获得敏感权限”。
- 列表与详情必须返回一致的可见集合。
- 管理端 UI 隐藏不能作为唯一测试；直接调用 API 仍必须拒绝。
