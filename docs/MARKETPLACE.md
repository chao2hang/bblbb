# BBLBB — 公开市场与原子交易接口

> 版本：v0.4
> 本文定义用户自建市场接入、物品购买、自动扣款、不可变入账和商户通知的安全边界。

## 1. 目标与非目标

目标：

- 允许用户注册自己的市场应用，并通过稳定的 `/api/v1/marketplace/*` 接口接入。
- 用户确认购买后，实时返回已经提交的成功或明确失败；不得先返回成功再异步扣款。
- 购买记录、账户扣款、账本流水、结账意图消费、审计和 Outbox 在一个数据库事务内原子提交。
- 网络重试、并发点击和商户重放不能造成重复扣款或重复交付。

非目标：

- 市场不能直接设置、增加、减少或读取用户完整余额。
- 市场不能提交任意可信价格、收款账户或货币；金额必须来自服务端登记的报价快照。
- Webhook 不是购买成功的事实来源；查询接口和 BBLBB 已提交的交易记录才是事实来源。
- v1 不提供信用透支、法币结算、链上资产、托管清算或跨站分布式事务。

“额度”在 v1 指站内可消费货币的可用余额，并可叠加后台配置的用户/市场单笔与日累计消费限额。默认不允许负余额。

## 2. 参与方与信任边界

- **用户**：拥有站内账户并对具体购买明确授权。
- **市场应用**：由用户创建的 Confidential Client；服务端保存 Secret 哈希，可配置精确 HTTPS 回调地址。
- **报价 Offer**：市场预先登记的物品、货币、整数最小单位金额、库存策略和版本。
- **结账意图 Checkout Intent**：BBLBB 根据有效报价生成的短效、不可变购买快照。
- **购买 Purchase**：成功提交后的事实记录，关联唯一结账意图和积分 operation。

OIDC 登录与市场扣款授权分离。普通 `openid/profile/email` scope 永远不能扣款。市场能力使用专用 scope：

- `marketplace.checkout.create`：为当前已登录用户创建结账意图。
- `marketplace.purchase`：在用户交互确认后消费一次结账意图。
- `marketplace.purchases.read`：读取本 Client 自己的购买状态。
- `marketplace.refund`：申请本 Client 自己交易的全额或受策略允许的部分退款。

高风险 scope 必须经过管理员批准和用户单独同意；不能由 Refresh Token 静默扩权。Public Client 不得获得扣款或退款 scope。

## 3. 注册与报价

市场应用必须登记：

- 所有者、显示名称、服务条款和隐私政策 URL。
- 精确 HTTPS redirect URI、Webhook URL 和允许的来源。
- Client 状态、批准 scope、单笔/日累计限额和风险等级。
- Webhook 签名密钥版本；密钥只在创建或轮换时显示一次。

报价由市场通过鉴权接口创建，但由 BBLBB 保存并分配 `offer_id` 和 `version`。结账仅接受 `offer_id + expected_version`，服务端读取可信金额；不接受请求方覆盖 `amount`、`currency_id` 或收款方。报价变价、禁用或库存策略变化后，旧版本不能创建新意图。

## 4. 安全结账流程

1. 市场服务端以 Confidential Client 身份提交 `offer_id`、商户订单号和幂等键。
2. BBLBB 校验 Client、scope、报价、限额和回调配置，创建 5 分钟内有效的 Checkout Intent；意图绑定 Client、用户、Offer 版本、货币、金额和商户订单号。
3. BBLBB 托管的同意页显示市场身份、物品名称、数量、准确金额、当前可用余额、扣款后余额和授权有效期。页面 POST 使用 Session + CSRF；高风险或超过阈值时要求近期重新认证/TOTP。
4. 用户确认后，Rust 在单个数据库事务中消费意图并执行购买。
5. 只有事务提交成功才返回 `succeeded`。响应包含稳定的 `purchase_id`、金额、货币、提交时间和可用于查询的状态，不返回完整账本或其他余额。
6. 同事务写入 `marketplace.purchase.succeeded` Outbox。worker 提交后向市场发送签名 Webhook；Webhook 延迟不改变购买结果。
7. 市场收到超时或未知结果时，必须使用同一幂等键重试或查询 `purchase_id`/商户订单号，不得创建新的随机请求来猜测结果。

Checkout Intent 是一次性的，不作为 Bearer 凭证暴露在 URL、Referer 或日志中。确认动作必须同时绑定当前 Session 用户、Client 和意图；任何字段不一致均拒绝。

## 5. 原子事务

账务、商户账户、结算、Checkout 用户绑定、服务凭证与退款资金来源以 [`MARKETPLACE-ACCOUNTING.md`](MARKETPLACE-ACCOUNTING.md) 为冻结事实来源。v1.0 采用平台托管双边站内账本：买方扣款、商户待结算入账和可选平台费同一事务，禁止将购买金额静默销毁或信任 Client 提交收款方。

MySQL/MariaDB：开始事务后先插入或读取唯一幂等 operation，再以 `SELECT ... FOR UPDATE` 按固定顺序锁定 Checkout Intent、报价/库存、买方货币账户、商户账户和可选平台费账户。

SQLite：使用 `BEGIN IMMEDIATE`，以唯一约束和账户 `version` 条件更新串行化写入。

共同步骤：

```text
1. 插入/读取 (client_id, endpoint, idempotency_key)；比较规范化请求摘要
2. 锁定并校验 Checkout Intent：未过期、未消费、用户/Client/Offer 版本一致
3. 读取可信报价并校验 Client、用户、处罚、单笔/日限额及库存
4. 锁定账户，计算 available = balance - frozen_balance，拒绝余额不足
5. 创建 purchase，并以条件更新消费 intent/库存
6. 更新 point_accounts，追加 point_operations 与 point_transactions
7. 写 audit_logs 和 outbox_events
8. 保存可重放的 operation 响应并提交
```

唯一约束至少包括 `(client_id, merchant_order_id)`、`checkout_intent_id` 的成功购买唯一性和账务幂等 scope/key。任一步失败必须全部回滚。锁等待超时或死锁返回可重试错误，不得返回模糊成功。

“实时”定义为同步响应只报告数据库已经提交的最终购买状态；目标延迟由实际压测给出 SLO。外部 Webhook 属于最终送达，不放进数据库事务，也不延迟购买提交。

## 6. 幂等、重放与请求认证

- 所有创建意图、购买和退款 POST 强制 `Idempotency-Key`。
- 相同 key 与相同请求摘要返回原 HTTP 语义和原资源；不同摘要返回 409。
- Confidential Client 使用短期 opaque Access Token；Client Secret 仅用于标准 token endpoint，业务请求不重复传 Secret。
- Token 绑定 Client、scope、到期时间和可选 audience；禁用 Client、撤销 consent 或用户封禁必须实时阻止新购买。
- 对高风险服务到服务接口可叠加 DPoP 或 mTLS，作为 v1.0 加固项；不能用自创签名替代 TLS/OAuth。
- Checkout Intent 具有高熵 ID、服务端到期时间和单次消费状态。客户端时间、价格、余额和 `succeeded` 标志都不可信。
- 按 Client、用户、IP、Offer 和失败率限流；异常金额、频率、设备或失败模式触发拒绝/二次验证，不静默降级安全检查。

## 7. 退款与撤销

- 已提交购买不 UPDATE/DELETE，不把原流水改成失败。
- 退款创建新的 `reversal` operation、退款流水和退款记录，并引用原 purchase/operation。
- 同一购买的累计退款不得超过原购买金额；并发退款必须锁原购买并由唯一约束/累计条件保证。
- 市场只能退款自己的交易。超时、超过退款窗口、争议交易或大额退款进入管理员复核。
- 退款提交后通过独立签名 Webhook 通知；Webhook 重试不得重复入账。

## 8. Webhook

- 只允许预登记 HTTPS URL；保存时和发送时执行 SSRF 防护，阻断私网、loopback、链路本地地址和 DNS 重绑定。
- 事件包含 `event_id`、`event_type`、`created_at`、`client_id`、`purchase_id`、状态和最小必要金额字段，不含 Access Token、用户邮箱或完整余额。
- 使用每个 Client 独立、可轮换的 HMAC-SHA-256 密钥，对原始请求体、时间戳和事件 ID 签名。
- 接收方必须校验 5 分钟时间窗并按 `event_id` 去重。BBLBB 对非 2xx 指数退避重试并提供投递记录/手动重放；重放保持原 `event_id`。
- Webhook 可能重复、乱序或延迟。市场必须通过购买查询接口对账。

## 9. 数据最小化与审计

- 市场只看自己的 Offer、Intent 和 Purchase；用户标识使用该 Client 下的 pairwise subject，不暴露内部 `user_id`。
- 余额查询默认不开放；结账同意页可向用户本人显示余额，市场响应仅给出 `insufficient_funds` 等最小结果。
- 审计记录 Client、用户、Offer、Purchase、request ID、结果码和脱敏网络风险信息，不记录 Token、Secret、完整 Webhook 签名或敏感商品描述。
- 市场所有者可查看安全事件、调用量、失败率、Webhook 投递与对账差异；不能查看其他市场数据。

## 10. 对账与故障处理

- 提供按 `purchase_id`、`merchant_order_id` 查询及 cursor 增量对账接口。
- 定时任务验证 purchase 金额与 point operation/transactions 一致，验证账户余额满足账本恒等式。
- 数据库不可用时返回 503 且不创建购买；worker/Webhook 故障不回滚已提交购买，但必须告警。
- Outbox 堆积、幂等冲突、余额不足激增、重复意图消费、退款超额尝试和 Webhook 长期失败均进入指标与告警。
- 管理员紧急禁用 Client 后停止创建意图、购买、退款和 Token 刷新；已提交交易保留并可对账。

## 11. 上线门槛

- SQLite、MySQL、MariaDB 均通过双花、锁竞争、失败注入和账本属性测试。
- 完成越权、IDOR、CSRF、Token/Intent 重放、价格篡改、Offer 换版、Client 禁用、退款竞态和 SSRF 测试。
- OpenAPI 提交并生成兼容性测试客户端。
- Webhook 签名测试向量、密钥轮换、重试和对账流程有公开接入文档。
- 在正式实现和上述测试通过前，原型中的市场功能只用于设计演示，不允许接真实资产或生产 Client。

## 12. M12 实现说明（v1.0）

实现对应不可变迁移 `0056_marketplace.sql`（SQLite/MySQL/MariaDB 三库等价），
领域层在 `backend/src/marketplace/`（clients/offers/checkout/refunds/webhooks/
reconcile/balance），路由在 `backend/src/routes/marketplace.rs` 与管理端
`backend/src/routes/admin.rs`，前端在 `frontend/src/routes/marketplace/` 与
`frontend/src/routes/admin/marketplace/`。

- **认证决策（M12 设计约束 #1）**：OIDC scope 白名单冻结为
  `openid/profile/email`（M11-CONSENT-06），不存在可用的 user-bound
  `marketplace.*` Access Token，因此 v1 的 Checkout Intent 创建使用 Session
  认证（AuthSession），请求体只接受 `client_id/offer_id/
  expected_offer_version/merchant_order_id/quantity`，金额/货币/收款方全部
  服务端派生；confirm 使用 Session + CSRF + intent/user/client 一致性校验
  （`checkout_user_mismatch` 403 / `checkout_interaction_invalid` 409）。
  服务操作（Offer 创建/更新、退款、服务端 Purchase 查询）使用
  `Authorization: Basic client_id:client_secret`（Confidential Client 秘密）
  或管理员（reason + recent-auth）；普通 OIDC scope 永远不能调用扣款接口。
- **账务恒等式**：买方扣款走不可变账本；商户与平台费使用合成账本账户
  （`merchant:{client_id}`、`platform:fees`，需真实 users 行以满足
  `point_accounts` 的 FK，密码哈希 `!` 无法登录），每次购买/退款同事务写
  多个账本 operation，`Σ(delta_balance + delta_pending + delta_frozen) = 0`
  由对账校验（docs/MARKETPLACE-ACCOUNTING.md §8）。
- **Webhook Secret 存储**：明文只在创建/轮换时返回一次；库中保存
  AES-256-GCM 密文（`marketplace_webhook_encryption_key` 主密钥，空则
  fail closed），签名时解密；不使用不可逆 hash（HMAC 需要明文密钥）。
- 管理端扩展接口（Client 注册、scope 审批、Offers、Webhook 投递与重放、
  对账运行、紧急停用、requested 退款重试）为内部管理端点，不在冻结的
  193-op OpenAPI 契约中；OpenAPI 已登记的市场/Admin marketplace 操作全部
  `verified`。
- v1.0 结算等待期为 7 天（`SETTLEMENT_DELAY_MS`）；`settle_pending` 由
  定时任务调用（pending→available 不改变总额，version 条件更新 + 审计）。
