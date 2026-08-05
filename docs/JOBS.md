# BBLBB — 后台任务与 Transactional Outbox

> 版本：v0.4
> v1 使用数据库任务表和同一 Rust 进程内的 Tokio worker，避免在小机器上强制部署消息队列。

## 1. 适用任务

- 邮箱验证、重置密码和通知邮件。
- 站内通知聚合。
- 搜索索引更新。
- 图片检测、重编码和缩略图。
- 定时发布。
- Session、OAuth code/token、验证码过期清理。
- 账号延迟删除。
- 计数器修复。
- 配置型插件 after-event。
- 市场购买/退款 Webhook 签名投递、重试和投递记录。
- 市场交易增量对账与账本一致性检查。
- 下载扣费授权与积分账本一致性检查、过期授权整理。
- 内部商城限时商品过期、Entitlement 过期卸下、活动签到/任务结算、榜单聚合和异常奖励反查。
- AI 格式化、内容审计建议、SEO/摘要草稿和 Provider 健康检查。
- 视频 URL 元数据解析、HLS 清单检查、视频引用刷新和来源状态检查。
- 数据导出和低优先级维护。

不进入异步任务的关键路径：权限判定、余额校验、内容 grant、市场购买/退款提交、Checkout Intent 消费、授权码消费和处罚生效。市场 Webhook 可以异步，但只有数据库已提交的 Outbox 事件才能投递。

## 2. Outbox 模式

业务事务中同时写：

```text
业务表变更 + audit_logs + outbox_events
```

事务提交后 worker 才处理事件。这样避免：

- 帖子已发布但通知事件丢失。
- 余额已增加但等级/通知永远没触发。
- 事务回滚却发出了邮件。

事件 envelope 见 `PLUGIN.md`。Outbox payload 必须最小化，不复制密码、Token、完整隐藏正文等敏感数据。

实现（M01-JOBS-02）：业务代码在事务内调用 `outbox::enqueue_in_tx(&mut tx, event_type, payload)`；事务提交事件才持久化，事务回滚事件同步消失。时间戳一律为 Unix 毫秒（M01-DB-08）。

## 3. Job 生命周期

```text
queued → running → succeeded
   └────→ retry_wait → running
   └────→ dead
running ──lease timeout──→ queued/retry_wait
```

字段见 `SCHEMA.md`：`available_at`、`locked_by`、`locked_until`、`attempts`、`max_attempts`、`last_error`。

- worker 通过租约领取任务。
- 任务开始前增加 attempts 或按明确语义记录。
- 进程崩溃后，租约过期可重新领取。
- 完成写 `completed_at`，失败按策略设置下次 `available_at`。
- 超过最大重试进入 dead-letter 状态并告警。

## 4. 数据库领取策略

### MySQL/MariaDB

- 事务中使用适用的行锁/`SKIP LOCKED` 能力；需分别验证最低版本行为。
- 批量领取数量小，快速提交租约，不在锁事务内执行任务。

### SQLite

- 使用 `BEGIN IMMEDIATE` 完成短暂领取和租约更新。
- 单机默认一个 worker coordinator，按 queue 限制并发。
- 不在写事务内发 SMTP、访问 S3 或处理图片。
- 配置 `busy_timeout`，监控锁等待。

### 领取/续租契约（M01-JOBS-04）

实现位于 `backend/src/jobs/worker.rs`，SQLite 与 MySQL/MariaDB 共用同一套语句：

- `claim_batch(pool, worker_id, queue, limit, lease_ms)` 批量领取：
  1. 先把本 queue 中 lease 已过期的 `running` 任务重新入队（`running → queued`，
     崩溃恢复；`available_at` 重置为当前时间，立即进入可领取集合）；
  2. 按 `available_at` 升序选出最多 `limit` 个可领取任务
     （`queued`/`retry_wait`、`available_at <= now`、无未过期锁）；
  3. 每个任务用 CAS UPDATE 抢占（WHERE 同时约束状态、`available_at` 与锁），
     只有 `rows_affected == 1` 才算领取成功；多 worker 并发不会重复领取。
- 领取成功即 `attempts + 1`（一次领取 = 一次执行尝试），写 `locked_by = worker_id`、
  `locked_until = now + lease_ms`、`status = running`。
- `renew_lease(pool, worker_id, job_id, lease_ms)` 只允许 owner 在 lease 未过期
  （`locked_until >= now`）时续租；owner 不符、任务已非 `running` 或 lease 已过期
  一律返回 `false`，worker 必须立即放弃该任务——它可能已被其他 worker 重领。

## 5. 幂等

- Job handler 必须按“至少一次执行”设计。
- **Outbox 事件去重（M01-JOBS-06）**：消费者对每个事件开启事务——
  `outbox::consume_in_tx(tx, event_id, consumer)` 写入
  `outbox_consumed(event_id, consumer)` 去重标记（唯一约束），返回 `true`
  才执行业务副作用，随后 `outbox::mark_sent_in_tx(tx, event_id)` 标记
  `sent`，整体提交。重复投递（崩溃重试/多消费者竞争）时唯一约束让
  `consume_in_tx` 返回 `false`，副作用不会重复提交。不同消费者各自去重。
  消费者崩溃则整事务回滚，标记与副作用一起消失，事件保持 `pending` 可重投。
- Job 的 `deduplication_key` 唯一约束在入队层去重：同一业务副作用只创建
  一个 job（M01-JOBS-01），崩溃重跑不重复入队。
- 邮件可记录 message logical ID，避免重试重复发送；SMTP 最终仍可能出现极少数重复，模板应容忍。
- 积分、付费解锁等调用核心服务时传业务幂等键。
- 图片处理输出使用内容/参数哈希 key，重复执行覆盖同一目标或无副作用。

## 6. 重试

实现位于 `backend/src/jobs/retry.rs`（M01-JOBS-05）。

- 错误分类 `RetryClass`：
  - `Transient`（临时性：网络、SMTP 4xx、S3 超时、数据库暂不可用）→ 按退避重试。
  - `Permanent`（输入无效、模板缺失、附件格式不支持）→ 直接 dead-letter，不重试。
- 退避 `RetryPolicy::backoff_with_jitter`：
  第 N 次失败的等待 = `min(base * 2^(N-1), max_delay) + [0, jitter]`
  （`base_delay_ms`/`max_delay_ms`/`jitter_ms`，指数饱和不溢出）。
- 最大次数：`fail_job` 以行级 `max_attempts` 为准；`attempts >= max_attempts`
  仍失败 → `dead`。`attempts` 在领取时 +1（M01-JOBS-04）。
- `fail_job(pool, worker_id, job_id, error, class, policy)`：仅 owner 有效；
  返回 `Retry { next_available_at }` / `Dead` / `LostLease`（lease 失效或
  owner 不符，不做任何修改）。
- `complete_job`：owner 标记成功（`running → succeeded`，写 `completed_at`）。
- `replay_job`：人工重放 dead 任务（`dead → queued`，重置 attempts/last_error/
  completed_at/租约，立即可领取）。管理操作，调用方必须写审计（§11）。
- 错误文本必须是安全摘要：不写入邮件正文、Token、隐藏内容到 `last_error`。

其他建议：

- 临时网络、SMTP 4xx、S3 超时：重试。
- 输入无效、附件格式不支持、模板缺失：直接 dead 或低次数重试。
- 数据库暂时不可用：进程级退避，不快速刷日志。
- 每类任务定义最大尝试、超时和并发，不能只有全局值。

错误日志存安全摘要；不把邮件正文、Token 和隐藏内容写入 `last_error`。

## 7. 队列与优先级

建议队列：

| Queue | 任务 | 优先级 |
|---|---|---|
| `security` | 登录安全通知、Token 重用通知 | 高 |
| `marketplace_webhook` | 已提交购买/退款的签名通知 | 高 |
| `ai` | 格式化、审核建议、SEO 草稿和模型调用 | 中 |
| `video` | URL 解析、HLS 检查、元数据刷新 | 中 |
| `mail` | 验证、重置、普通通知 | 高/中 |
| `media` | 图片检查、缩略图 | 中 |
| `search` | 索引更新 | 中/低 |
| `plugins` | 配置型插件 | 低 |
| `maintenance` | 清理、计数修复、导出 | 低 |

小机器限制媒体和导出并发，防止内存峰值影响 HTTP。

## 8. 定时任务

- scheduler 周期性将到期工作插入 jobs。
- 使用数据库 lease 保证多实例下一个周期任务只有一个调度者。
- 定时发布的正确性以 `scheduled_at <= now` 查询为准，任务可重复扫描。
- 清理任务采用小批量和游标，避免长事务。
- 任务时间全部为 UTC；显示时转换时区。

## 9. Outbox 清理

- `published_at` 表示所有必要消费者已确认或已转换为独立 job。
- 已发布事件保留可配置窗口，便于审计和故障排查。
- 清理小批量执行。
- 插件后来启用不默认回放全部历史；回放需显式范围和管理员确认。

## 10. 可观测性

指标：

- 各 queue queued/running/dead 数量。
- 最老待处理任务年龄。
- 成功/失败/重试速率。
- 处理耗时分位数。
- SQLite busy/锁等待。
- Outbox 未发布数量和年龄。

每次执行日志包含 `job_id`、`kind`、`attempt`、`request_id/event_id`，不含敏感 payload。

告警建议：

- security/mail 队列最老任务超过 5 分钟。
- dead-letter 新增。
- Outbox 堆积持续增长。
- media 任务内存/超时异常。

## 11. 管理操作

后台支持：

- 查看聚合统计和安全错误摘要。
- 重试单个/筛选后的 dead job。
- 取消尚未运行的可取消任务。
- 暂停低优先级 queue。
- 不能在 UI 任意编辑 payload 后重试。
- 所有重试/取消写审计。

## 12. 优雅停机

1. readiness 置失败，停止接收新外部流量。
2. 停止领取新任务。
3. 等待运行中任务到配置期限。
4. 可取消任务安全释放；不可取消任务让 lease 到期后重试。
5. 关闭连接池。

## 13. 未来外部队列

只有出现明确瓶颈时才引入 Redis Streams/NATS/RabbitMQ。迁移条件：

- 数据库队列显著影响业务事务。
- 需要大量独立 worker 或跨服务消费。
- 已有成熟的消息备份、监控和运维能力。

即使迁移外部消息系统，Transactional Outbox 仍可保留以连接业务事务和发布过程。
