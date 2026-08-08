# BBLBB 告警定义（M15-OBSERVE-06）

> 告警输入为 `deploy/monitoring/metrics.md` 的指标；表达式为 PromQL。
> 每条告警：触发条件 → 抑制 → 升级 → 值班通知 → 恢复确认（演练见
> `alerts-drill.sh`）。值班矩阵见 `ops/runbooks/oncall.md`。

## 1. HTTP 与数据库

| 告警 | PromQL 条件 | 级别 |
|---|---|---|
| `bblbb_http_5xx_high` | `rate(bblbb_http_errors_total[5m]) / max(rate(bblbb_http_requests_total[5m]), 0.1) > 0.05` | P1 |
| `bblbb_http_429_surge` | `rate(bblbb_http_429_total[5m]) > 20` | P2 |
| `bblbb_http_latency_p95` | `bblbb_http_request_duration_seconds{quantile="0.95"} > 1.0`（按基线调整） | P2 |
| `bblbb_db_pool_exhausted` | `bblbb_db_pool_idle == 0 AND bblbb_db_pool_size == bblbb_db_pool_max` 持续 5m | P1 |
| `bblbb_db_connect_failures` | `rate(bblbb_db_connect_failures_total[10m]) > 0` | P0 |
| `bblbb_sqlite_busy` | `rate(bblbb_sqlite_busy_total[5m]) > 10` | P1 |

## 2. 任务 / Outbox / 队列

| 告警 | PromQL 条件 | 级别 |
|---|---|---|
| `bblbb_jobs_dead` | `rate(bblbb_jobs_dead_total[15m]) > 0`（无 dead 事件出现即死信） | P1 |
| `bblbb_jobs_oldest_pending` | 安全/邮件队列最老任务年龄 > 5 分钟（由 /jobs 快照 + 管理视图） | P1 |
| `bblbb_outbox_backlog` | `bblbb_outbox_pending > 100` 持续 30m | P2 |
| `bblbb_outbox_failed` | `rate(bblbb_outbox_failed[15m]) > 0` | P1 |
| `bblbb_queue_accumulating` | `delta(bblbb_jobs_queued[30m]) > 50` | P2 |
| `bblbb_jobs_running_high` | `bblbb_jobs_running > 50` 持续 30m（租约堆积/worker 卡死） | P1 |

## 2b. 身份与内容域

| 告警 | PromQL 条件 | 级别 |
|---|---|---|
| `bblbb_session_login_failures` | `rate(bblbb_session_login_failures_total[5m]) > 30` | P1 |
| `bblbb_session_lockouts` | `rate(bblbb_session_lockouts_total[15m]) > 5` | P1 |
| `bblbb_csrf_rejections` | `rate(bblbb_csrf_rejections_total[5m]) > 10` | P2 |
| `bblbb_totp_failures` | `rate(bblbb_totp_failures_total[5m]) > 10` | P2 |
| `bblbb_oidc_token_errors` | `rate(bblbb_oidc_token_errors_total[15m]) > 5` | P2 |
| `bblbb_ledger_errors` | `rate(bblbb_ledger_errors_total[15m]) > 0` | P0 |
| `bblbb_uploads_failed` | `rate(bblbb_uploads_failed_total[15m]) > 0` | P1 |

## 3. 备份 / 磁盘 / WAL

| 告警 | 条件 | 级别 |
|---|---|---|
| `bblbb_backup_failed` | `systemctl is-failed bblbb-backup` 或最新备份产物 >26h | P0 |
| `bblbb_backup_verify_failed` | 每周恢复演练非 PASSED | P0 |
| `bblbb_disk_free` | 磁盘可用 < 20%（警告）/ < 10%（P0） | P1/P0 |
| `bblbb_wal_growth` | WAL 大小 > 主库 2 倍且持续增长（cron/脚本检查） | P1 |

## 4. S3 / SMTP / Provider

| 告警 | 条件 | 级别 |
|---|---|---|
| `bblbb_s3_errors` | `rate(bblbb_storage_errors_total[5m]) > 0`（分类见 s3-errors.md） | P1 |
| `bblbb_s3_permanent` | dead-letter 中 S3 永久错误新增 | P1 |
| `bblbb_smtp_failed` | mail 队列 dead 新增或 SMTP 5xx 记录 | P1 |
| `bblbb_provider_5xx` | AI/Video/Marketplace Provider 5xx/超时率上升（日志聚合） | P2 |
| `bblbb_oidc_key_expiring` | active signing key 将过期 / 无法解密（verify-oidc-keys.sh） | P1 |

## 5. 抑制与升级（M15-OBSERVE-08）

- 抑制：维护窗口（周四 02:00-04:00 UTC）内抑制发布/迁移类告警；
  P0 数据风险类告警永不抑制；
- 升级：P2 → 15 分钟未处理升 P1；P1 → 30 分钟未处理升 P0；
  P0 立即通知值班 A + 运维负责人（oncall.md 升级路径）；
- 值班通知：PagerDuty/企业微信/webhook（部署环境配置）；
- 恢复确认：告警表达式回到阈值以下持续 15 分钟 + 对应 Runbook 的恢复确认步骤。

## 6. 演练（M15-OBSERVE-08）

`deploy/monitoring/alerts-drill.sh` 每月表推演练：断言每条告警定义存在、
指标名与 `METRIC_HELP` 白名单一致、表达式可加载（Prometheus rule YAML 语法
校验）、值班/升级/审批路径存在。演练输出记录到 `ops/monitoring/`。
