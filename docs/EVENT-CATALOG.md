# BBLBB — 领域事件与审计目录

> 基线：v0.4。事件使用 `<domain>.<action>.v<major>`；Outbox 与业务事实同事务写入。事件可能重复、乱序和延迟，Consumer 必须按 `event_id` 幂等。

## 1. Envelope

```json
{
  "event_id": "uuid-v7",
  "event_type": "post.published.v1",
  "occurred_at": 0,
  "aggregate_type": "post",
  "aggregate_id": "uuid-v7",
  "aggregate_version": 1,
  "actor": { "type": "user", "id": "uuid-v7" },
  "correlation_id": "uuid-v7",
  "causation_id": "uuid-v7",
  "payload_version": 1,
  "payload": {}
}
```

Payload 默认只含 ID、状态和必要公开字段，不复制密码、Token、Secret、完整隐藏正文、AI Prompt、签名 URL 或 HLS Key。

## 2. 事件目录

| event_type | 同事务触发 | 主要 Consumer | PII/保留 |
|---|---|---|---|
| `user.registered.v1` | User 创建 | 邮件、审计 | 用户 ID；按账户保留 |
| `user.status_changed.v1` | 用户状态迁移 | Session 撤销、通知 | 最小原因码 |
| `post.published.v1` | Post 发布 | 搜索、通知、SEO/AI | 不含隐藏正文 |
| `post.visibility_changed.v1` | 隐藏/恢复/删除 | 索引清理、缓存失效 | 仅 ID/状态 |
| `comment.created.v1` | Comment 提交 | 通知、计数 | 清洗摘要可选 |
| `moderation.case_changed.v1` | 案件状态迁移（含指派/取消指派） | 通知、报表 | 不含内部备注（`moderation_notes` 不随事件外发） |
| `sanction.changed.v1` | 处罚创建/生效/到期/撤销 | Session/权限失效、通知 | kind/scope/期限；撤销引用 `sanction_reversals` id |
| `appeal.changed.v1` | 申诉提交/进入复核/决定/撤回 | 通知、报表 | 仅 appeal/sanction id 与状态，不含正文与内部 note |
| `attachment.ready.v1` | 完成验证 | 通知、引用 | MIME/大小，无 URL |
| `download.authorization_created.v1` | 免费或扣费授权 | 风控、统计 | 金额/策略版本 |
| `points.operation_completed.v1` | 账务提交 | 等级、通知、对账 | 整数 delta，无余额全量 |
| `marketplace.purchase_succeeded.v1` | Purchase 提交 | Webhook、结算 | Client/Offer/Purchase/金额 |
| `marketplace.refund_succeeded.v1` | Refund 提交 | Webhook、对账 | Refund/Purchase/金额 |
| `marketplace.settlement_due.v1` | 结算到期 | 结算 Worker | Merchant/Purchase |
| `ai.task_completed.v1` | Task 结束 | UI 通知、审计 | hash/用量/状态，无原文 |
| `video.embed_changed.v1` | Embed 状态改变 | 缓存、通知 | Provider/状态，无签名流 URL |
| `shop.order_succeeded.v1` | 内部商城扣费/持有创建 | 通知、统计 | 商品/订单/金额 |
| `shop.entitlement_changed.v1` | 装备、过期、撤销 | 用户投影、缓存 | 商品/槽位/状态 |
| `activity.claimed.v1` | 签到/任务奖励提交 | 通知、榜单 | 规则/金额/去重键 |
| `reaction.created.v1` | 帖子/评论互动 | 通知、计数 | 目标/反应类型 |
| `reaction.removed.v1` | 撤销互动 | 通知、计数 | 目标/反应类型 |
| `config.policy_changed.v1` | Policy 换版 | 缓存失效、重检 | 配置 diff 的脱敏摘要 |
| `auth.security_notification.v1` | 安全通知（新设备/密码/MFA 变化/Session 撤销/恢复码使用） | 邮件、站内通知 | 仅 kind/user_id，无设备/IP 原文 |

> M13-PLUGIN：v1 配置型插件订阅上述 `*.v1` 事件（白名单子集，见
> `backend/src/plugins/mod.rs::KNOWN_EVENTS`）；插件动作是 after-event 异步
> worker，绝不阻塞核心事务。插件调用摘要写入 `plugin_call_metrics`
> （`result` ∈ ok/error/timeout/repeat/stale/skipped），不是新的领域事件。
> 主题变更不产生事件：`themes.revision` 单调递增即缓存/SSR 失效信号。

> M3 板块/角色/标签的领域事件（`board.*`、`role.*` 等）尚未实现：事件只在
> 对应域的 Operation 落地（M03-BOARDS/M03-AUTHZ）时登记并注册到
> `backend/src/events.rs`，目录不收录未实现事件（`check-event-catalog.rb`
> 强制目录与注册表一致）。
>
> M05-SCHEMA 落地了治理与通知数据模型（0041-0045）；`appeal.changed.v1`
> 已随 M05-APPEALS 注册（M05-NOTIFY 消费既有领域事件生成站内/邮件通知，
> 不再单列 `notification.*` 事件）。

## 3. Webhook

Marketplace 外发 Webhook 从上述已提交事件生成，必须包含 `event_id`、类型、时间、Client、Purchase/Refund ID、状态、金额和货币，不包含内部用户 ID或余额。签名 Header、时间窗和测试向量由 OpenAPI 定义。

M12 实现（`backend/src/marketplace/webhooks.rs`）：

- 事件在业务事务提交后由 Outbox 登记 `webhook_deliveries`（`event_id` 即 Outbox 事件 ID），worker/手动重放异步投递；投递结果不改变已提交购买结果；
- 签名：`X-BBLBB-Signature = HMAC-SHA-256(secret, "{timestamp}.{event_id}.{body}")`（hex），密钥为该 Client 可轮换的 Webhook Secret（明文只显示一次，库中 AES-256-GCM 密文存储）；附带 `X-BBLBB-Webhook-Timestamp` 与 `X-BBLBB-Webhook-Event-Id`；
- 接收方必须校验 5 分钟时间窗并按 `event_id` 去重（`event_id_hash` 提供去重摘要）；重放保持原 `event_id`；
- 非 2xx 指数退避（30s·2^n，上限 10 分钟），超过 `max_attempts`（5）进入 `dead_letter` 保留手动重放；
- 发送前再次执行 SSRF 校验（私网/回环/链路本地/IPv6 拒绝）；`UnavailableWebhookClient` 在 egress 未配置时安全拒绝。

## 4. 审计

- 审计记录不是可重放领域事件；不得用 Event 删除或修改审计历史。
- 管理策略、Secret 轮换、退款、积分调整、内容管理、Provider 测试和手动重放必须写审计。
- 事件/审计 schema 变更增加版本，不静默改变既有字段语义。
