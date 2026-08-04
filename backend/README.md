# BBLBB backend

Rust + Axum 的最小可运行后端骨架。当前只提供基础健康检查和 OpenAPI 文档读取，不包含真实认证或业务逻辑。

## 运行

在 `backend/` 目录执行：

```sh
cp .env.example .env
cargo run
```

默认监听 `127.0.0.1:3000`。配置通过环境变量读取，变量使用 `BBLBB__` 前缀和双下划线分隔，例如 `BBLBB__BIND_ADDRESS`、`BBLBB__OPENAPI_PATH`。

## 基础端点

- `GET /healthz`：返回 `{"status":"ok"}`。
- `GET /api/v1/openapi.json`：读取 `BBLBB__OPENAPI_PATH` 指向的 OpenAPI 文档并返回。默认从仓库的 `openapi/openapi.yaml` 读取，当前保持原始 YAML 内容。
- 所有响应都会经过 `X-Request-ID` 边界：请求提供合法值时沿用，否则生成 UUID，并在响应中返回。

统一错误响应使用 `application/problem+json`，包含 `type`、`title`、`status`、`code` 和 `detail` 字段；业务错误边界可在后续路由中复用 `AppError`。

## 分层

- `src/config.rs`：环境变量配置结构和默认值。
- `src/middleware/`：请求 ID 等 HTTP 边界中间件。
- `src/routes/`：按资源划分的路由处理器。
- `src/error.rs`：统一 Problem JSON 错误响应。
- `src/app.rs`：状态和 Router 组装。

验证：`cargo fmt --check`、`cargo check`、`cargo test`。
