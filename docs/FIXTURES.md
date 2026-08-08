# BBLBB — 测试 Fixture 与可控性约定（M16-HARNESS-01）

> 约定：所有集成测试必须可控 Clock、随机 ID、外部依赖（邮件/S3/AI/Video）与
> 请求 Fixture；禁止依赖真实 SMTP/S3/Provider 网络。本文件是 Fixture 的单一事实来源。

## 1. 可控 Clock

- 时间统一经 `bblbb_backend::outbox::now_millis()`（可注入）；测试用固定 seed 时间。
- 依赖时间的边界（Sanction 半开边界、签到日界、会话过期、Intent TTL、JWT exp）
  一律用显式时间戳断言，不依赖真实时钟。
- 证据：`backend/tests/moderation/sanctions.rs#effective_sanctions_realtime_boundaries`、
  `backend/tests/economy/activity.rs#checkin_activity_day_follows_user_timezone_boundary`、
  `backend/tests/marketplace/checkout.rs#intent_expires_after_ttl`。

## 2. 随机 ID 与 UUIDv7

- 所有主键用 `uuid::Uuid::now_v7()`（时间有序，天然 keyset 排序）。
- 测试不硬编码 ID；需要引用时先插入再取回。
- 证据：`backend/tests/boards_pagination.rs`、`backend/tests/harness_contract.rs`。

## 3. 邮件 Fake

- 生产 SMTP 经 `EmailSender` trait 抽象；测试用 `RecordingSender`（记录调用参数、
  可注入失败脚本）。payload 只存 `user_id` 引用 + 安全模板参数，无完整邮箱/正文/token。
- 证据：`backend/src/email/service.rs`（`EmailSender`/`RecordingSender`）、
  `backend/tests/mail_payload_safety.rs`、`backend/tests/jobs_retry.rs`（退避/死信）。

## 4. S3 Fake

- 存储适配器错误分类（403/404/429/5xx/超时/DNS/TLS/部分上传）经
  `StorageError` 稳定码断言；`backend/tests/storage/adapter.rs` 用 mock 注入
  供应商错误验证分类/重试/dead（M16-STORAGE-FAULTS-01/02）。
- 真实 AWS S3/MinIO/R2 兼容矩阵为外部阻塞项（M16-STORAGE-FAULTS-01 `[!]`）。
- 证据：`backend/src/storage/error.rs`（`StorageError::code/is_retryable`）。

## 5. AI Fake

- 出站策略（HTTPS/host/端口/私网/重定向/大小）为纯函数
  `EgressPolicy`，直接单测（`backend/tests/ai/gateway.rs`）。
- Provider 调用经 `ProviderClient` 抽象，测试用 `MockProviderClient`
  （固定响应/固定错误，无真实网络）：`backend/tests/ai/tasks.rs`。
- 证据：`backend/tests/ai/gateway.rs`、`backend/tests/ai/tasks.rs#execute_retries_5xx_then_dead_after_max_attempts`。

## 6. Video Fake

- 出站探测经 `FetchClient` 抽象；测试用 `MockClient`（固定响应）。
- 证据：`backend/tests/video.rs`（`MockClient`）、`backend/src/video/egress.rs`。

## 7. 请求 Fixture 与身份助手

`backend/tests/common/mod.rs` 提供统一助手（M02-SESSION-08 起）：

- `enroll_totp(pool, user_id)`：为 elevated 账号完成 TOTP enrollment（强制启用降级规则）。
- `direct_session_cookie(pool, user_id)`：直接签发真实 Session Cookie（绕过 HTTP 登录）。
- `fetch_preauth(app)`：获取匿名预认证 CSRF 状态（`GET /api/v1/auth/csrf`）。

Persona 铸造（M14-E2E）：`frontend/tests/playwright/fixtures/seed-personas.mjs` 按
`user_sessions` 真实 schema 铸造 anonymous/unverified/cooldown/member/moderator/
admin/mute/banned 会话，驱动 Playwright desktop+mobile。

## 8. 三数据库 Fixture 与跨库 runner

- 同一 repository/HTTP 测试套件在 SQLite/MySQL 8/MariaDB 10.11 上运行：
  `.github/workflows/ci.yml`（`mysql-family-migrations` 矩阵 + 6 个 crossdb 测试
  二进制 `transaction_concurrency`/`session_crossdb`/`auth_crossdb`/
  `schema_fixture`/`search_store`/`search_fixture`，`BBLBB_TEST_MYSQL_URL`）。
- 本地全量测试默认 SQLite；MySQL/MariaDB 实机执行为外部阻塞项（M16-HARNESS-02 `[!]`）。

## 9. 错误码 Fixture

- 每个稳定 Problem code 至少一个后端 Fixture + 前端映射：
  `ruby scripts/check-code-fixtures.rb`（docs/ERROR-CODES.md ↔ OpenAPI ↔ backend ↔ frontend 四方一致）。
- 状态机合法/非法迁移矩阵：`reports/rc/state-machine-coverage.md` +
  `ruby scripts/check-state-machine-matrix.rb`。
