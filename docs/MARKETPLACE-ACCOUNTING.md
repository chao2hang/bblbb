# BBLBB — Marketplace 账务、身份与结算决策

> 基线：v0.4。本文冻结 `MARKETPLACE.md` 中不能留给实现人员决定的资金去向、凭证主体和退款规则。

## 1. v1.0 账务模型

v1.0 采用**平台托管双边站内账本**：

- 买方使用可消费货币账户付款。
- 每个已批准 Marketplace Client 绑定一个不可由普通用户直接操作的 `merchant_account`。
- Purchase 同事务从买方可用余额扣除，并向商户待结算余额记账；货币不会静默销毁。
- 平台费默认 `0`，可按 Client 策略配置整数基点 `fee_bps`；费率和平台账户进入 Checkout Intent 不可变快照。
- `buyer debit = merchant pending credit + platform fee credit`，三者必须在同一 `point_operation` 组内保持恒等。
- 不允许 Client 在请求中提交收款账户、货币、单价、费率或总额。

## 2. 商户账户

`marketplace_merchant_accounts`：

- `id`、`client_id`、`owner_user_id`、`currency_id`、`available_balance`、`pending_balance`、`frozen_balance`、`version`、状态和时间字段。
- `(client_id, currency_id)` 唯一；Client 审批时由核心服务创建。
- Client 转让所有者不改变账户 ID和历史 Purchase；转让要求原所有者和管理员审批、近期认证并写审计。
- v1.0 不提供提现到法币、链上资产或站外支付；merchant balance 仅为站内可追踪价值。若以后支持提现，必须另立合规和清算版本。

## 3. 购买入账

购买锁顺序：

```text
idempotency operation
→ checkout intent
→ offer/stock
→ buyer point account
→ merchant account
→ platform fee account（fee > 0 时）
```

事务内：

1. 条件消费 Intent 与库存。
2. 创建 Purchase。
3. 买方 `delta_balance=-total_amount`。
4. 商户 `delta_pending=total_amount-platform_fee`。
5. 平台费账户 `delta_balance=platform_fee`。
6. 写不可变 transactions、审计和 Outbox。
7. 提交后返回成功；Webhook 异步发送。

默认结算等待期为 7 天。到期 Job 将商户 `pending_balance` 转为 `available_balance`；争议、Client 冻结或退款会阻止对应金额结算。结算仍使用不可变 operation，不直接改历史流水。

## 4. Checkout 身份绑定

创建 Checkout Intent 必须使用**用户授权码流程签发的短期 user-bound Access Token**：

1. 用户在市场站点选择 Offer。
2. 市场将用户引导至 BBLBB Authorization Endpoint，请求 `marketplace.checkout.create`。
3. BBLBB Session 用户明确同意后，市场后端以 Authorization Code + PKCE 换取 user-bound Token；Token 的 `sub` 为 pairwise subject，`aud` 绑定 Client。
4. 市场后端使用该 Token 创建 Intent；用户 ID仅从 Token 解析，不接受请求体 `user_id`。
5. 响应返回高熵 `interaction_id` 和 BBLBB 托管确认 URL；Intent 本身不放入 URL。
6. 用户打开托管页；BBLBB 校验 Session 用户、interaction、Client、Intent 用户和有效期一致。
7. 确认动作使用 Session + CSRF；市场后台不能代替用户确认。
8. 完成后 BBLBB 只重定向到预注册回调，并附带一次性结果码；市场后端通过购买查询确认最终状态。

稳定错误：Session 与 Token 用户不一致使用 `checkout_user_mismatch`（403）；interaction 失效使用 `checkout_interaction_invalid`（409）。

## 5. Client 服务凭证

为履行退款和对账，v1.0 增加仅限 Confidential Client 的 `private_key_jwt` 或轮换 Client Secret 服务认证：

- 服务 Token 只允许 `marketplace.offer.write`、`marketplace.purchases.read`、`marketplace.refund`、`marketplace.webhook.manage`。
- 服务 Token 没有用户 `sub`，不能创建 Checkout Intent、确认购买、读取用户余额或调整积分。
- 这不是通用 OAuth Client Credentials：Token Endpoint 仅为已批准 Marketplace Confidential Client 提供固定 audience 和固定白名单 Scope。

## 6. 退款

- 市场使用服务 Token 请求自己 Purchase 的退款；不要求买家在线，也不受买家撤销 Checkout Consent 影响。
- 管理员可用 `marketplace.refund_admin` 强制退款，必须近期认证、填写原因并受单次/每日限额。
- 退款先使用商户该 Purchase 未结算的 pending 余额；已结算时从 merchant available 扣除。
- 余额不足不允许账户变负：退款进入 `requested` 并冻结 Client 新销售，由管理员补足/冲正后重试。
- 平台费默认按退款比例返还；原 Checkout 快照可配置 `fee_refundable=false`，必须在用户确认页展示。
- 累计退款不得超过 Purchase 原金额；部分退款按整数最小单位，最后一次处理舍入差额。
- 退款同事务创建 Refund、买方 credit、商户/platform debit、不可变 reversal operation、审计和 Outbox。

## 7. 风险与停用

- Client 停用：停止新 Token、Offer、Intent 和 Purchase，但允许只读对账及管理员批准的退款。
- Client 冻结：待结算和可用商户余额转入 frozen，不删除交易。
- 用户封禁：停止新购买；已有退款仍可入账到被冻结的用户余额，并不可消费，避免损害其追偿权。
- 任何人工账务修复只能追加补偿 operation；禁止直接编辑 Purchase 或 point transaction。

## 8. 恒等式与验收

每个 Purchase/Refund operation 必须满足：

```text
Σ(delta_balance + delta_pending + delta_frozen) = 0
```

若系统存在显式铸币/销毁 operation，必须使用独立 kind 和授权，不能复用 Purchase。三数据库并发测试必须覆盖库存、Intent、买方、商户、平台费、部分退款、余额不足退款和死锁重试。
