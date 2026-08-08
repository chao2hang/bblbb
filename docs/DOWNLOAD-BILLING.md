# BBLBB — 下载抵扣积分接口

> 版本：v0.4
> 本文定义附件下载按次/授权扣除站内可消费积分的接口预留和安全边界。

## 1. 规则

- 下载扣费是**下载授权**，不是 S3 链接费用。S3 预签名 URL 过期不会再次扣费，也不会删除附件。
- 后端每次先鉴权，再判断用户是否已有有效下载授权；没有授权时，在同一事务内校验价格、锁定账户、扣款、追加不可变流水、创建授权并写审计/Outbox。
- 只有事务提交成功后才签发临时下载 URL。扣款失败、对象未就绪、权限不足或事务回滚时不返回可用 URL。
- 同一用户对同一附件在授权有效期内重复下载不重复扣费；授权过期后是否再次收费由后台策略决定，默认按新的授权周期收费。
- 站点所有者、附件所有者、管理员和后台配置的免费等级/角色可以免扣费，但仍必须经过后端鉴权和审计。
- 价格使用整数最小单位，默认货币为 `coin`；不得使用浮点数，不允许负价。免费为明确的 `0`，不是缺省绕过。

## 2. 后台可配置策略

后台 `/admin/download-billing` 配置：

- 总开关：启用/停用下载扣积分。
- 计费货币：只能选择已启用且不允许负余额的可消费货币。
- 默认下载价格：整数最小单位。
- 授权有效期：例如 24 小时；只影响下载授权，不影响附件对象或 S3 URL 生命周期。
- 计费范围：所有私有附件、仅指定板块/用途、或仅标记为付费的附件。
- 免费等级和免费角色；管理员可强制免费或强制收费，优先级必须在策略解释中显示。
- 单次扣费上限、用户每日下载扣费上限、单附件累计收入上限和站点紧急停用。
- 附件级覆盖价格：`free`、固定整数价格或继承全局默认；作者不能通过浏览器字段直接改变生效价格。

变更立即影响新下载授权，不撤销已经购买的授权，不改写历史流水。所有配置变更要求管理员权限、近期重新认证、原因和审计。

### 策略解析与优先级

服务端按以下固定顺序解析，命中终止模式后不再读取更低层价格：

1. 站点紧急停用：拒绝新付费授权；管理员必须显式选择“免费降级”或“全部拒绝”，默认免费降级。
2. 管理员附件级 `forced_free/forced_paid`。
3. 附件级 `free/fixed/inherit`。
4. 目标内容所属板块级 `free/fixed/inherit`。
5. 站点默认 `free/fixed`。
6. 在已解析价格上应用所有者、管理员、免费角色和免费等级规则；`forced_paid` 仅跳过角色/等级免费，不跳过附件权限。
7. 校验单次上限、用户每日扣费上限和附件累计收入上限。

一个附件引用多个内容时，请求必须携带当前访问上下文 `target_type + target_id`，服务端验证 `attachment_links` 后使用该目标板块；不能由客户端任意选择更便宜板块。直接访问无上下文附件使用附件策略后回退站点策略，不使用任意板块策略。

免费路径也创建 `download_authorization`，`charged_amount=0` 且 `point_operation_id=null`，以便统一重签、限流和审计。授权过期后是否重新收费取决于新授权时的当前策略，不沿用旧价格。

## 3. API

```text
GET   /api/v1/attachments/{id}/download-policy   查看当前请求方可见的价格/授权状态
POST  /api/v1/attachments/{id}/download          鉴权、必要时原子扣费并返回临时 URL
GET   /api/v1/download-authorizations/{id}        查询本人的下载授权状态
POST  /api/v1/download-authorizations/{id}/sign-url 重新鉴权并签发 URL，不重复扣费
GET   /api/v1/me/download-transactions            查询本人的下载扣费流水

GET   /api/v1/admin/download-billing/config      获取脱敏配置
PATCH /api/v1/admin/download-billing/config      修改全局策略
GET   /api/v1/admin/attachments/{id}/download-policy
PATCH /api/v1/admin/attachments/{id}/download-policy 设定 free/fixed/inherit
```

`POST /download` 必须使用 `Idempotency-Key`。请求只接受 `attachment_id`、可选 `expected_policy_version` 和客户端请求 ID，不能接受 `amount`、`currency`、`user_id` 或 `owner_id`。

成功响应示例：

```json
{
  "authorization_id": "019...",
  "attachment_id": "019...",
  "charged": { "currency": "coin", "amount": 12 },
  "download_url": "https://s3.example/...signed...",
  "url_expires_at": "2026-08-03T12:05:00Z",
  "authorization_expires_at": "2026-08-04T12:00:00Z",
  "reused_authorization": false
}
```

- `download_url` 只属于当前短时响应，不能进入日志、Referer、缓存或长期数据库字段。
- 已有授权重复调用返回新的临时 URL，`charged.amount` 为 `0`，并标记 `reused_authorization=true`。
- 余额不足返回 409 `insufficient_funds`；计费停用/免费策略返回 `charged.amount=0`；附件未 ready 返回 409；无权访问返回 403/404。
- 客户端网络超时必须使用原幂等键重试或查询授权，不能随机新建请求。

## 4. 原子事务

MySQL/MariaDB 固定锁定顺序：幂等记录 → 附件/策略版本 → 既有授权 → 用户货币账户。使用 `SELECT ... FOR UPDATE`。

SQLite 使用 `BEGIN IMMEDIATE`，并验证账户版本更新 `rows_affected == 1`。

扣费路径：

```text
校验 Idempotency-Key 和请求摘要
→ 重新鉴权附件关联内容、用户状态、策略和对象 ready 状态
→ 查询/锁定有效 download_authorization
→ 若没有有效授权，锁账户并校验余额、每日限额和策略版本
→ 创建 point_operation(kind=consume, source=download_authorization)
→ 更新 point_accounts
→ 插入 point_transaction 和 download_authorization
→ 写 audit_logs + outbox_events
→ 提交
→ 重新鉴权后签发短期 S3 URL
```

签发 URL 不在数据库事务中执行。签发失败时下载授权和扣费仍是已提交事实：响应返回 HTTP 503、Problem code `download_url_unavailable`，并在安全扩展字段返回 `authorization_id`、`authorization_expires_at`、`charged` 和 `reused_authorization`，但绝不返回不完整 URL。客户端用原幂等键重试 `POST /download` 或查询授权后调用 `POST /api/v1/download-authorizations/{id}/sign-url`；两条路径都只重新鉴权和签名，不得再次扣款。v1 下载为同步完成，不存在异步 202 状态（`download_authorization_pending` 已从稳定码注册表移除）。不采用“拿到 URL 才算扣费成功”的第二套状态机。

## 5. 防滥用与隐私

- 价格和免收费判定只由 Rust 后端完成；前端提示不具备授权意义。
- 同一 Idempotency-Key 不同请求摘要返回 409；并发请求不能创建多个授权或扣款。
- 下载接口按用户、IP、附件和失败率限流；Range 请求共享同一授权，不按字节重复扣费。
- 预签名 URL 只允许访问指定对象 key，Bucket 默认私有；禁止把签名 URL 作为公开静态链接。
- 日志不记录 Token、完整 URL、积分余额、附件隐藏内容或完整文件名；审计记录脱敏的价格策略版本、扣费结果和 request ID。
- 管理员停用计费只阻止新扣费，不能删除历史授权或流水；退款必须通过补偿 operation。

## 6. 测试门槛

必须测试余额不足、重复点击、并发下载、幂等冲突、授权过期、策略换版、用户降级/封禁、S3 URL 到期、URL 签发失败、数据库每一步故障回滚，以及 SQLite/MySQL/MariaDB 账户锁竞争。

验证：

```text
初始余额 + Σ(point_transactions.delta_balance) = 当前余额
每个成功扣费授权最多一个 consume operation
每次重复授权签发 URL 不新增扣费流水
```
