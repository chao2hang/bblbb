# BBLBB backend

Rust + Axum 后端。当前已完成骨架、认证闭环和全量路由挂载；各业务领域按 OpenAPI 契约逐步实现，未完成的操作返回 `501 not_implemented` Problem。

## 运行

在 `backend/` 目录执行：

```sh
cp .env.example .env
cargo run
```

默认监听 `127.0.0.1:8080`。配置通过环境变量读取，变量使用 `BBLBB__` 前缀和双下划线分隔，例如 `BBLBB__BIND_ADDRESS`、`BBLBB__OPENAPI_PATH`。完整配置清单见 `.env.example`。

也可通过根目录 `make` 命令统一操作：`make check`、`make test`、`make build`、`make migrate`（SQLite 迁移到空库，需系统 `sqlite3` CLI，3.40+）。

## 基础端点

- `GET /healthz`：返回 `{"status":"ok","version":...}`。
- `GET /readyz`：返回数据库/存储目录就绪检查。
- `GET /api/v1/openapi.json`：读取 `BBLBB__OPENAPI_PATH` 指向的 OpenAPI YAML 文档，解析后以 JSON 格式返回。默认从仓库的 `openapi/openapi.yaml` 读取。
- 所有响应都会经过 `X-Request-ID` 边界：请求提供合法值时沿用，否则生成 UUID，并在响应中返回。

统一错误响应使用 `application/problem+json`，包含 `type`、`title`、`status`、`code`、`detail` 和 `request_id` 字段；错误边界可在各路由中复用 `AppError`。

## 已实现

- 认证闭环：`/api/v1/auth/csrf`、`register`、`verify-email`、`login`、`session`（登出）、`password-reset` 及其 confirm，基于数据库的密码哈希、Session 和令牌。
- 数据库：`sqlx` 连接池，支持 SQLite（自动建库、WAL）与 MySQL 8 / MariaDB 10.11；启动默认**不**自动应用迁移（M01-DB-06），显式开启 `BBLBB__AUTO_MIGRATE=true` 或传 `--migrate` 参数时应用 `BBLBB__MIGRATIONS_DIR` 下的待执行迁移（版本 + checksum 校验，失败即退出）。
- 路由挂载：boards、posts、comments、moderation、storage、economy、ai、video、oidc、marketplace、admin、feeds、search、themes 等领域模块已接入 Router，未实现的操作返回 `501 not_implemented` 占位。
- 中间件：请求 ID、安全响应头、请求体 10MB 上限、30s 超时、HTTP 追踪和 CSRF 防护（状态变更请求携带会话 Cookie 时必须提供合法的 `X-CSRF-Token`，否则返回 403 Problem）。

## 分层

- `src/config.rs`：环境变量配置结构和默认值。
- `src/middleware/`：请求 ID 等 HTTP 边界中间件。
- `src/routes/`：按资源划分的路由处理器。
- `src/error.rs`：统一 Problem JSON 错误响应。
- `src/db/`：连接池（SQLite/MySQL）与迁移执行。
- `src/auth/`：密码、Session、令牌。
- `src/app.rs`：状态和 Router 组装。

验证：`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（含 `tests/http.rs` 集成测试）。
