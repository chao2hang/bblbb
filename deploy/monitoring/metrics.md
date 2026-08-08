# BBLBB 指标目录（M15-OBSERVE-04/05）

> 输出：`GET /metrics`（Prometheus 文本格式；仅 loopback/受控监控可访问，
> M15-PACKAGE-07）。实现：`backend/src/observability/metrics.rs` + `/metrics`
> 路由（`backend/src/routes/metrics.rs`）。指标名白名单
> `METRIC_HELP` 强制登记，未登记指标直接 panic（防拼写漂移）。

## M15-OBSERVE-04：基础设施指标

| 指标 | 类型 | 说明 | 实现 |
|---|---|---|---|
| `bblbb_http_requests_total` | counter | HTTP 请求总数 | TraceLayer on_response |
| `bblbb_http_errors_total` | counter | HTTP 5xx 响应数 | TraceLayer on_response |
| `bblbb_http_429_total` | counter | HTTP 429（限流）数 | TraceLayer on_response |
| `bblbb_http_request_duration_seconds` | summary | p50/p95/p99/sum/count（对数桶近似） | observe_latency_ms |
| `bblbb_db_pool_size` | gauge | 当前连接池连接数 | /metrics 抓取时计算 |
| `bblbb_db_pool_idle` | gauge | 空闲连接数 | /metrics 抓取时计算 |
| `bblbb_db_pool_max` | gauge | 最大连接数（配置） | /metrics 抓取时计算 |
| `bblbb_db_connect_failures_total` | counter | 连接池创建失败次数 | main.rs 启动路径 |
| `bblbb_sqlite_busy_total` | counter | SQLite busy/locked 指数退避次数 | db/busy.rs retry_on_busy |

## M15-OBSERVE-05：领域指标

| 指标 | 类型 | 触发点 |
|---|---|---|
| `bblbb_session_login_failures_total` | counter | auth/login.rs 密码错误 |
| `bblbb_session_lockouts_total` | counter | auth/login.rs 触发锁定 |
| `bblbb_csrf_rejections_total` | counter | middleware/csrf.rs（csrf_failed/origin_not_allowed） |
| `bblbb_totp_failures_total` | counter | auth/mfa.rs TOTP 校验失败/重放 |
| `bblbb_oidc_token_errors_total` | counter | routes/oidc.rs oauth_error_response |
| `bblbb_uploads_failed_total` | counter | routes/storage.rs 上传失败/quarantined |
| `bblbb_storage_errors_total` | counter | routes/storage.rs storage_error_response |
| `bblbb_ledger_errors_total` | counter | economy/ledger/service.rs apply_operation |
| `bblbb_jobs_dead_total` | counter | jobs/retry.rs fail_job → dead |
| `bblbb_jobs_queued` | gauge | /metrics 抓取时队列快照（default+mail） |
| `bblbb_jobs_running` | gauge | /metrics 抓取时队列快照 |
| `bblbb_jobs_dead` | gauge | /metrics 抓取时队列快照 |
| `bblbb_outbox_pending` | gauge | /metrics 抓取时 outbox_events pending |
| `bblbb_outbox_failed` | gauge | /metrics 抓取时 outbox_events failed |

## 抓取方式

```sh
# loopback 直连（Caddy 不代理 /metrics）
curl -fsS http://127.0.0.1:8080/metrics
# Prometheus scrape_configs:
#   - job_name: bblbb
#     scrape_interval: 30s
#     static_configs: [{ targets: ['127.0.0.1:8080'] }]
#     metrics_path: /metrics
```

## 慢查询观测（M15-OBSERVE-07）

- 慢查询日志只输出 `label` + `elapsed_ms` + `threshold_ms`（`db/pool.rs`
  `with_slow_query_log`），**不输出 SQL 文本**、不输出参数值；
- HTTP 慢请求由 `http_request` span 的 `latency`/`route` 字段承载，路由为
  URL path（去 query 参数，避免高基数 label）；
- 需要按查询定位时用 `label`（业务域语义名）+ `request_id` 关联，禁止把
  未脱敏 query/SQL 作为 label。
