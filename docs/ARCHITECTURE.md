# BBLBB — 系统架构

> 版本：v0.3
> 本文定义进程边界、请求流、模块职责和演进路线。产品范围见 `REQUIREMENTS.md`。

## 1. 架构目标

- SQLite 模式可运行于 512MB 级服务器。
- 初期保持两个应用进程，不采用微服务。
- Rust 是身份、权限、内容可见性和事务的唯一业务裁决者。
- SvelteKit 提供 SSR 和交互，但不拥有独立业务数据库。
- 数据库、对象存储和任务实现均可通过受控接口替换。
- 对外 API 和 OIDC 协议优先稳定，内部模块可持续重构。

## 2. 生产拓扑

```text
Internet
  │ HTTPS
  ▼
Caddy :443
  ├─ /api/v1/*                          -> Rust 127.0.0.1:8080
  ├─ /.well-known/openid-configuration  -> Rust 127.0.0.1:8080
  ├─ /oauth/*                           -> Rust 127.0.0.1:8080
  ├─ /healthz                           -> Rust 127.0.0.1:8080
  └─ /*                                 -> SvelteKit 127.0.0.1:3000
                                               │ loopback HTTP
                                               └─ Rust 127.0.0.1:8080
                                                    │
                         ┌──────────────────────────┼───────────────────────┐
                         ▼                          ▼                       ▼
                    SQLite/MySQL             本地/S3 附件             SMTP/外部服务
```

- Rust 与 SvelteKit 都只监听 loopback 或容器内部网络。
- Caddy 是唯一公开入口和可信代理。
- Rust 只接受来自配置中可信代理的 forwarded headers。
- 后端始终验证用户凭据，不存在“来自 SvelteKit 即可信”的通道。

## 3. 请求流

### 3.1 匿名页面

1. 浏览器请求文章或板块页面。
2. Caddy 转给 SvelteKit。
3. SvelteKit SSR 调用 Rust 公共 API。
4. Rust 在查询层过滤不可见资源并返回公开投影。
5. SvelteKit 输出 HTML，并按内容是否个性化设置缓存头。

### 3.2 登录页面

1. SvelteKit 显示表单和 CSRF 所需信息。
2. 表单同源 POST 到 Rust `/api/v1/auth/login`，或由 action 透明代理。
3. Rust 验证凭据、限流和账号状态，创建 `user_sessions`。
4. Rust 返回 `__Host-bblbb_session` Cookie。
5. 若 action 代理，必须完整转发 `Set-Cookie`；推荐浏览器直接请求 Rust。

### 3.3 写操作

1. 浏览器发送 Session Cookie、CSRF header、`X-Request-ID`。
2. Rust 验证 Session、CSRF、动作权限和对象范围。
3. Domain service 执行业务规则。
4. Repository 在一个事务内写业务数据、审计和 Outbox。
5. 返回资源、版本和一致的错误格式。

### 3.4 后台任务

1. 业务事务写入 `outbox_events`。
2. 同一 Rust 进程中的 Tokio worker 领取事件。
3. 将事件转换为幂等 `jobs` 或直接执行轻量消费者。
4. 邮件、通知、搜索和插件失败独立重试，不回滚已提交业务事务。

## 4. 后端模块

```text
backend/src/
  app/             配置、依赖装配、启动和优雅停机
  config/          环境变量与配置校验
  domain/
    identity/      用户、凭据、Session
    authorization/ 权限策略
    forum/         板块、帖子、回复、标签
    moderation/    举报、处罚和审核
    economy/       货币、账本和等级
    content_access/隐藏内容和解锁
    oauth/         OIDC 领域模型
    storage/       附件生命周期
    notification/  通知领域
  application/     用例服务、事务边界、DTO 映射
  infra/
    database/      sqlx pool、迁移、仓储实现
    jobs/          Outbox 与任务 worker
    mail/          SMTP 适配器
    storage/       本地/S3 适配器
    crypto/        Token、哈希、密钥加密
  http/
    api/            /api/v1 handlers
    oidc/           标准协议端点
    middleware/     request ID、tracing、认证、CSRF、限流
    openapi/        utoipa schema
```

规则：

- Domain 不依赖 axum、sqlx、SMTP 或 S3。
- Handler 不直接执行 SQL。
- 事务边界由 application service 管理，而不是一个 repository 各自提交。
- OIDC HTTP 层可以依赖成熟协议组件，但持久化、用户同意和密钥由明确适配器实现。
- 核心审计不能由插件钩子替代。

## 5. 前端模块

SvelteKit 只维护：

- 页面路由与 SSR。
- 表单和渐进增强。
- API 类型化客户端。
- 本地展示状态。
- 主题 registry 和已预编译 UI 扩展。
- SEO、RSS、sitemap 和可访问性。

前端不得：

- 直接访问数据库。
- 根据隐藏按钮代替权限检查。
- 持有数据库、OIDC 私钥或 SMTP secret。
- 接收后端返回的隐藏正文后再用 CSS 隐藏。

## 6. 数据库适配

- 逻辑模型见 `SCHEMA.md`。
- `Database` enum 在启动时解析连接串，装配 SQLite 或 MySQL 系实现。
- MySQL 驱动同时服务 MySQL 与 MariaDB，但 CI 分别验证。
- 不要求每个 repository 写完全重复代码；可移植查询可共享，锁/UPSERT/搜索等差异使用策略实现。
- 连接池按数据库类型分别配置；SQLite 写连接数量保持保守。

## 7. 一致性边界

必须同事务：

- 帖子/回复 + 审计 + Outbox。
- 余额更新 + 操作 + 账本流水。
- 付费扣费 + 内容 grant。
- 授权码原子消费 + Token 创建。
- Refresh Token 轮换与旧 Token 标记使用。
- 处罚创建 + 审核动作 + 审计。

允许最终一致：

- 邮件。
- 站内通知展示。
- 搜索索引。
- 统计计数修复。
- 缩略图。
- 配置型插件 after-event。

## 8. 缓存

v1 不要求 Redis：

- 进程内短缓存：站点配置、权限定义、已启用主题/插件元数据。
- 更新后通过应用内失效；单实例不存在跨节点问题。
- Session 可短暂正向缓存，但撤销需立即可见；简单起见 v1 可每次查数据库。
- 不缓存隐藏正文投影。
- 多实例阶段才引入 Redis/pub-sub，并要求安全失效策略。

## 9. 故障模型

| 故障 | 行为 |
|---|---|
| 数据库不可用 | `/readyz` 失败；写请求 503；不伪造成功 |
| SvelteKit 不可用 | API/OIDC 仍可用；页面 502 |
| Worker 故障 | 核心写入仍成功；Outbox 堆积并告警 |
| SMTP 故障 | 邮件任务重试；用户看到待发送状态 |
| 对象存储故障 | 上传失败或保持 pending；帖子事务不引用未 ready 文件 |
| 插件失败 | 插件任务重试/死信；核心事务不回滚 |
| OIDC 私钥不可用 | Token 签发停止并 readiness 降级，不临时生成无持久密钥 |

## 10. 扩展路线

### 单机扩展

- SQLite WAL、短事务、静态资产由 Caddy 提供。
- Worker 并发受控，避免压垮 SQLite。
- 图片处理可限制并发或分进程。

### 多实例

在需要多实例时：

1. 迁移到 MySQL/MariaDB。
2. 附件迁移到 S3 兼容存储。
3. Session 保持数据库态或引入 Redis。
4. worker 使用数据库租约协调。
5. Caddy/负载均衡器分发请求。
6. 进程内缓存增加分布式失效。

模块化边界允许将 worker、搜索或媒体处理拆出，但用户、权限、论坛和积分在没有明确瓶颈前保持一个 Rust 服务。

## 11. 配置原则

- 配置层级：编译默认值 < 配置文件 < 环境变量/secret file < 启动参数。
- 启动时一次性验证必需配置、URL、目录权限和密钥。
- 不允许未知生产配置键静默忽略。
- 公开站点设置与服务器 secret 分离。
- 具体键和部署见 `OPERATIONS.md`。

## 12. 架构决策记录

后续重大决策写入 `docs/adr/NNNN-title.md`，至少包括：

- 背景与约束。
- 采用方案。
- 被拒绝方案。
- 安全、迁移和运维影响。
- 可逆性。

首批 ADR 建议：数据库主键、前后端拓扑、opaque Access Token、主题代码边界、数据库任务队列。
