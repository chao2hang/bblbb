# BBLBB — 领域状态机与稳定枚举

> 基线：v0.4。数据库、Rust 枚举、OpenAPI、前端状态和测试 Fixture 必须一致。除明确列出的迁移外全部拒绝；管理操作也不能跳过审计和版本检查。

## 1. 通用规则

- 状态迁移使用事务和乐观版本；高争用经济对象同时使用数据库锁和唯一约束。
- 非幂等迁移要求 `Idempotency-Key`；修改已有资源要求 `If-Match` 或 `version`。
- 终止态不物理删除历史；修复使用显式恢复或补偿状态。
- 每次敏感迁移记录 actor、reason、request ID、前后状态和策略版本。

## 2. 用户与内容

### User

```text
pending_verification → active → restricted → active
                         ├──────→ banned → active
                         └──────→ pending_deletion → anonymized
```

- `pending_deletion` 在延迟期内可由本人撤销；`anonymized` 为终止态。
- 管理员不能通过直接更新状态绕过 sanction、注销和审计服务。
- 法律保留（`users.legal_hold_at` 非空，RETENTION-PRIVACY.md §1 最高优先级）：
  禁止发起注销请求；冷却期内被设置则到期执行 Job 跳过并写审计
  `user.deletion_deferred_legal_hold`，账户保持 `pending_delete`（M03-PROFILE-08）。

### Post

发布状态：

```text
draft → pending_review → published → hidden → published
  └──────────────→ deleted ←──────────────┘
pending_review → rejected → draft
```

> **M04-SCHEMA 同步说明**：`posts.status` 的 DB CHECK 值域当前为 0003 骨架集合
> （draft/published/hidden/deleted/locked——`locked` 为遗留值，新代码用
> `closed_at`）；`pending_review`/`rejected` 随 M04-POSTS 迁移扩展 DB CHECK
> （SQLite 改 CHECK 需重建表，待骨架列收口时一并落地，见 SCHEMA.md §posts）。
> `closed_at` 非空即锁帖（禁止新增回复）。

回复能力为独立属性：

```text
closed_at = null  ↔  closed_at = timestamp
```

- 协议枚举不再使用 `status=closed`；`closed_at` 只控制新增 comment。
- `hidden` 是审核动作；`deleted` 是作者或有权限管理员的软删除。
- `published` 不保证任何请求方可见，仍需板块和内容访问策略判定。

### Comment

```text
published → hidden → published
published/hidden → deleted
```

### Board（板块稳定枚举，迁移 0022/0024/0025）

```text
visibility:   public | members | restricted | hidden
posting_mode: normal | approval | readonly | closed
活跃:         is_active=1 AND deleted_at IS NULL
停用:         is_active=0
软删除:       deleted_at 非空（行保留）
```

- `visibility`/`posting_mode` 为稳定枚举，无内部状态迁移；CHECK 三库强制
  （sqlite 0022 内联；mysql/mariadb 0025 补齐）。
- `hidden` 板块不进入公开列表/搜索；`readonly`/`closed` 禁止新增帖子
  （服务层强制，M03-BOARDS-03）。
- `deleted_at` 非空仍保留行（软删除）；物理删除需无子板块且由服务层裁决
  （M03-BOARDS-01，SCHEMA-06 语义）。

### Authorization（授权稳定枚举，迁移 0021/0022/0023）

```text
permissions.risk_level: normal | sensitive | system
roles.is_system:        0 | 1
assignment.expires_at:  NULL（永久） | 时间戳（到期即未生效）
```

- `risk_level=system` 的权限与 `is_system=1` 的角色不可删除/改名（应用约束，
  M03-AUTHZ；数据库无触发器，SCHEMA-06 测试锁定）。
- assignment 生效当且仅当 `granted_at <= now` 且 `expires_at` 为空（永久）
  或 `expires_at > now`；未来授权与已到期均按未生效实时判定（M03-AUTHZ-03，
  `assignment_effective_at`），过期/未来行保留供审计/恢复（不删除）。

## 3. 审核与处罚

> 迁移 0041-0044（M05-SCHEMA）。`can_transition_to` 由模型层
> `backend/src/moderation/model.rs` 强制，DB CHECK 兜底非法字面值。

### Report / Moderation Case

举报（`reports.status`）与案件（`moderation_cases.status`）共用主链，唯一
区别：举报多一个 `withdrawn` 终态（举报人撤回；案件不含 withdrawn）。

```text
open ─→ triaged ─→ investigating ─→ resolved
  │         └────────────→ rejected
  ├──→ rejected                      resolved/rejected ─→ reopened ─→ 回流主链
  └──→ withdrawn（仅举报，终态）
```

- `open` 可直接进入 `investigating`（跳过 triaged）或直接 `resolved`/`rejected`。
- `reopened` 回流主链：可进入 `triaged` / `investigating` / `resolved` / `rejected`。
- 终态：`resolved` / `rejected` / `withdrawn`（`withdrawn` 无任何出口；重新处理走
  新举报，见去重窗口）。

举报去重窗口：`reports` 按 `(reporter_id, target_type, target_id, reason_code)`
归一化键 + 锚定窗口 `dedup_until` 判定窗口内重复；`UNIQUE(report_dedup_key,
dedup_until)` 三库统一强制（见 SCHEMA.md §8）。

### Appeal

```text
submitted ─→ reviewing ─→ upheld
                ├──→ partially_upheld
                ├──→ rejected
                └──→ withdrawn（终态）
submitted ─→ withdrawn（终态）
```

- `upheld` / `partially_upheld` / `rejected` / `withdrawn` 均为终态。
- 每处罚至多一条申诉（`UNIQUE(sanction_id)`）：被拒后不可重复申诉，
  只能等待新处罚开启新申诉。
- 利益冲突：审查者（`appeal_decisions.reviewer_id`）不得是申诉人本人；
  声明冲突必须填写理由。

### Sanction

`kind` 固定为：

```text
warning | rate_limit | mute | board_mute | ban
```

`board_mute` 必须带 `board_id`；其他 kind 拒绝携带板块范围（CHECK 强制）。

状态：

```text
scheduled ─→ active ─→ expired        （时间自动推进）
active ─→ revoked                      （撤销，只追加 sanction_reversals）
scheduled ─→ revoked                   （生效前撤销）
```

- `scheduled → active`：`now >= starts_at` 时推进；
  `active → expired`：`ends_at` 非空且 `now >= ends_at`（半开边界，恰等于即到期）。
- `ends_at` 可空 = 永久（warning/ban 常为永久），永不到期。
- 撤销只追加：撤销动作写 `sanction_reversals`（每处罚至多一条），
  `sanctions.revoked_at/revoked_by/revoke_reason` 为当前态镜像；
  `revoked` 状态必须携带 `revoked_at` 与 `revoked_by`（CHECK 强制）。
- 生效判定 `Sanction::is_active_at(now)`：status=active 且 `starts_at <= now < ends_at`。

## 4. 附件与下载授权

### Attachment

```text
pending → processing → ready
pending/processing → quarantined → processing | deleted
ready → quarantined | deleted
```

- 临时 URL 到期不改变 Attachment 状态。

### Download Authorization

```text
active → expired
active → revoked
```

- URL 签发失败不改变 `active`；客户端可使用原幂等键或授权 ID重新签发。
- 免费下载也创建 authorization，以统一权限、限流和审计；`point_operation_id=null`、`charged_amount=0`。

## 5. Marketplace

### Checkout Intent

```text
created → awaiting_confirmation → consumed
created/awaiting_confirmation → expired | cancelled
```

- 一个 Intent 最多产生一个成功 Purchase；`consumed`、`expired`、`cancelled` 为终止态。

### Purchase

```text
succeeded → partially_refunded → refunded
succeeded/partially_refunded → disputed → succeeded | partially_refunded | refunded
```

- 不提供“回滚成功购买”状态；退款通过补偿账本实现。

### Refund

```text
requested → succeeded
requested → rejected | failed
failed → requested
```

- 重试必须保持同一退款业务 ID 和幂等作用域。

## 6. AI

### AI Task

```text
queued → running → succeeded
queued/running → cancelled
queued/running → failed → queued
failed → dead
```

- 撤回必要同意时：`queued` 任务取消；`running` 尽力取消并丢弃迟到输出；已成功 Suggestion 保留审计但不可继续处理未授权数据。

### AI Suggestion

```text
pending → accepted | rejected | stale
```

- 目标 revision 改变时转为 `stale`；不能覆盖新版本。

## 7. Video

### Video Embed

```text
pending → ready
pending/ready → blocked | error
error → pending
blocked → pending | removed
ready/error → removed
```

- Provider 被停用或策略收紧时，历史引用先降级外链，再异步重检；不得继续加载不符合新 CSP 的 iframe。

## 8. 内部商城与活跃

### Shop Product

```text
draft → pending_review → published → disabled → retired
pending_review → draft | rejected
```

### Shop Order

```text
created → succeeded → partially_refunded → refunded
created → rejected | expired
```

### Entitlement

```text
owned → equipped → owned
owned → expired | revoked | consumed
equipped → expired | revoked
```

- 装备不产生账务；购买成功才扣费。
- `revoked` 只能由退款、管理员补偿或安全撤销产生。

### Activity Claim

```text
eligible → claimed
eligible → rejected | expired
claimed → reversed
```

- 同一去重键只能成功一次；撤销奖励使用反向账本 operation。

## 9. Job、Outbox 与投递

### Job

```text
queued → running → succeeded
running → retry_wait → running
running → dead
running → queued            (lease 超时重新入队)
queued → cancelled
queued → dead
retry_wait → cancelled
retry_wait → dead
dead → queued               (人工重放，管理员审计操作)
```

- `queued`/`running`/`retry_wait`/`succeeded`/`cancelled`/`dead` 六个状态
  （M01-JOBS-03）；`succeeded`/`cancelled` 是终态无出边。
- 只能取消尚未运行的任务（`queued`/`retry_wait` → `cancelled`）；
  `running` 不直接取消，靠 lease 超时安全释放或按需人工干预。
- `dead → queued` 是人工重放边（M01-JOBS-05）：仅管理员在审计下把
  dead-letter 任务重新入队（重置 attempts/last_error），普通执行路径
  不会经过它。
- 非法迁移由 `backend/src/jobs/mod.rs` 的 `JobStatus::allowed_transition`
  拒绝，测试覆盖终态无出边、重放边与非法路径。

### Outbox Event

状态值来自迁移 CHECK 约束：`pending` / `processing` / `sent` / `failed`。

```text
pending → sent        (与业务副作用 + outbox_consumed 去重标记同一事务提交)
pending → pending     (失败重试，按 next_attempt_at 退避)
pending → failed      (达到 max_attempts)
```

- 消费者在业务事务内先写 `outbox_consumed(event_id, consumer)` 去重标记：
  唯一约束保证"至少一次投递"不重复产生业务副作用（M01-JOBS-06）。
  同一消费者对同一事件只领取一次；不同消费者各自独立去重。
- 消费者崩溃：整个事务回滚，去重标记与业务副作用一起消失，事件保持
  `pending`，重投时重新执行。

### Webhook Delivery

```text
pending → delivering → delivered
pending/delivering → retry_wait → pending
retry_wait/delivering → dead
```

- 手动重放创建新的 delivery attempt，但保持原 `event_id`。
