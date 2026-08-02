# BBLBB — 产品需求与实施边界

> 文档版本：v0.3
> 状态：可实施基线（架构与安全细节以配套文档为准）
> 产品定位：兼具博客发布体验的轻量社区论坛，并在后续版本作为 OpenID Connect Provider 为其他站点提供统一登录。

## 1. 产品目标

BBLBB 面向个人、小团队和中小型社区，核心目标是：

- 在 512MB 级服务器上以 SQLite 模式稳定运行。
- 同一套程序可选择 SQLite、MySQL 8 或 MariaDB 10.11 作为数据库。
- 提供文章、讨论、板块、标签、回复和管理后台。
- 提供用户、角色、板块级权限、审核和完整审计。
- 提供可审计的多币种积分与等级体系。
- 支持回复后可见、等级可见和付费解锁，且不会通过 API、缓存、搜索或 SEO 元数据泄漏隐藏内容。
- 主题和插件保持可扩展，但不以运行任意第三方代码为 v1 目标。
- v1.1 提供标准、可验证的 OIDC 登录服务，而不是自行发明 OAuth 协议。

## 2. 明确的非目标

v1 不包括：

- 即时聊天、私信和音视频通信。
- 微服务、Kafka、RabbitMQ、必须依赖 Redis 的架构。
- 运行时上传并执行任意 Svelte、JavaScript、Rust 或 WASM 代码。
- 多租户 SaaS；一个部署实例对应一个社区。
- OAuth implicit、password grant、device flow 等非必要授权模式。
- 原生移动端，但 API 保持可供未来客户端使用。

## 3. 技术与部署基线

| 层 | 选择 | 说明 |
|---|---|---|
| 入口 | Caddy | TLS、压缩、安全头、反向代理 |
| 前端 | SvelteKit + TypeScript + `adapter-node` | SSR、渐进增强、管理后台 |
| 后端 | Rust + axum + Tokio | 纯 JSON API、会话、权限、OIDC |
| 数据访问 | sqlx | SQLite / MySQL / MariaDB |
| 默认数据库 | SQLite（WAL） | 零额外数据库进程，适合小机器 |
| 可选数据库 | MySQL 8.0+、MariaDB 10.11+ | 面向更高并发或既有基础设施 |
| 后台任务 | Tokio worker + 数据库任务表/outbox | v1 不强制 Redis |
| 内容格式 | Markdown 源文 + 后端生成的已清洗 HTML | 禁止用户原始 HTML |
| API 描述 | OpenAPI | 前端类型由契约生成 |

512MB 目标仅适用于 SQLite 运行模式，且不在生产服务器上执行前端构建。若同机运行 MySQL/MariaDB，建议至少 1GB 内存。

## 4. 正式请求拓扑

```text
Internet
   │
   ▼
Caddy :443
   ├── /api/v1/*                         ──► Rust :8080
   ├── /.well-known/openid-configuration ──► Rust :8080
   ├── /oauth/*                          ──► Rust :8080
   ├── /healthz                          ──► Rust :8080
   └── 其他                              ──► SvelteKit :3000
                                                   │
                                                   └── SSR 时携带用户 Cookie
                                                       调用 Rust :8080
```

必须遵守：

1. Rust 后端始终验证 Session、Bearer Token、CSRF 和权限，不存在“内网免认证”。
2. 浏览器默认同源访问，因此生产环境默认不启用 CORS。
3. SvelteKit SSR 转发用户 Cookie、CSRF 信息和 `request_id`，但不能自行授予权限。
4. `/api/v1` 是稳定 API 前缀；OIDC 标准路径不放进该前缀。
5. OAuth 授权协议由 Rust 处理；登录和同意页面可以由 SvelteKit 渲染。

完整设计见 [`ARCHITECTURE.md`](ARCHITECTURE.md) 与 [`API.md`](API.md)。

## 5. 功能范围

### 5.1 账号与个人资料

- 邮箱、用户名和密码注册。
- 邮箱验证、重发验证邮件、找回和重置密码。
- 多设备 Session 查看和逐个撤销。
- 修改密码后撤销其他 Session。
- 昵称、头像、签名、自我介绍、时区和主题偏好。
- 用户状态：待验证、正常、受限、封禁、待删除、已删除。
- v1 可选 TOTP；Passkey/WebAuthn 预留到后续版本。

### 5.2 授权、角色与板块范围

- 内置角色：`administrator`、`moderator`、`member`。
- 自定义角色及权限分配。
- 全局角色与板块角色分别建模。
- 资源级规则：作者、板块版主和全局管理员。
- 等级权益不能绕过明确拒绝的角色/板块权限。
- 权限命名统一采用 `resource.action`，例如 `post.edit_any`。

详见 [`AUTHORIZATION.md`](AUTHORIZATION.md)。

### 5.3 博客与论坛内容

- 帖子类型：`article`（文章）和 `discussion`（讨论）。
- 板块支持层级、排序、可见性和发帖规则。
- 标签与标签分组。
- Markdown 编辑、草稿、预览、定时发布和编辑历史。
- 回复、引用、主题内楼层号、置顶、精华、关闭、移动、合并和软删除。
- 文章字段：slug、摘要、封面、发布时间、canonical URL 和 SEO 元数据。
- RSS/Atom、sitemap、robots、Open Graph、JSON-LD。
- 搜索在 v1.1 提供；SQLite 使用 FTS5，MySQL/MariaDB 使用全文索引。

### 5.4 内容访问策略

隐藏内容独立存储，公开正文只包含占位符。支持：

- 回复后可见。
- 达到等级或积分门槛可见。
- 支付指定货币后永久解锁。
- 作者、获授权版主和管理员按审计规则越权查看。

后端负责最终裁决，未授权 API 响应不得包含隐藏正文。搜索、通知、RSS、OpenGraph 和公共缓存同样不得包含隐藏正文。

### 5.5 审核与社区治理

- 举报帖子、回复和用户。
- 审核队列、处理记录和备注。
- 警告、限流、禁言、板块禁言、临时或永久封禁。
- 帖子/回复隐藏、恢复、移动、合并、关闭。
- 用户申诉与管理员复核。
- 所有管理动作进入不可由插件关闭的核心审计日志。

详见 [`MODERATION.md`](MODERATION.md)。

### 5.6 积分、货币和等级

- 管理员可定义多种货币，例如经验、金币和贡献。
- 每个用户每种货币一个账户。
- 所有余额变更必须在同一事务内更新账户并追加不可变流水。
- 支持规则奖励、消费、冻结、解冻、管理员调整和补偿交易。
- 创建或消费请求支持幂等键，防止重试导致重复入账。
- 经验建议采用累计值并用于等级；可消费货币不参与等级，避免消费导致降级。
- 管理员撤销操作通过反向补偿流水完成，不修改历史流水。

### 5.7 附件与媒体

- 本地磁盘和 S3 兼容对象存储适配器。
- 头像、封面和帖子附件。
- 文件大小、MIME、扩展名、用户配额与板块规则校验。
- 图片重新解码、缩略图、SVG 限制、下载权限和孤儿清理。

详见 [`STORAGE.md`](STORAGE.md)。

### 5.8 通知与后台任务

- 回复、引用、@提及、审核、等级变化和 OIDC 安全通知。
- 站内通知与邮件通知。
- 邮件、搜索索引、缩略图、定时发布、过期数据清理通过数据库任务队列执行。
- 关键业务事件通过 Transactional Outbox 保证最终送达。

详见 [`JOBS.md`](JOBS.md)。

### 5.9 主题

v1 支持两类主题能力：

1. **数据型主题**：CSS Token、Logo、字号和密度，可运行时安全切换。
2. **可信代码型主题**：构建时编译进 SvelteKit，可运行时在已编译主题之间切换。

上传新的 Svelte 代码不能即时执行，安装代码型主题必须重新构建和部署前端。详见 [`THEME.md`](THEME.md)。

### 5.10 插件

- v1：配置型后端规则和已预编译的前端扩展组件。
- v1 不接受任意网络请求、任意 SQL 或运行时上传代码。
- 审计、权限、积分账本和审核属于核心能力，不可被插件替代或关闭。
- WASM 运行时属于 v2 研究范围，并以 capability、资源配额和签名为前提。

详见 [`PLUGIN.md`](PLUGIN.md)。

### 5.11 OpenID Connect Provider

v1.1 提供最小、安全的 OIDC 子集：

- Authorization Code Flow。
- 所有客户端强制 PKCE S256。
- Public 与 Confidential Client。
- `openid profile email` scope。
- ID Token（RS256）、opaque Access Token、Refresh Token Rotation。
- Discovery、JWKS、UserInfo、Revocation 和 RP-Initiated Logout。
- 授权同意、Pairwise Subject、签名密钥轮换和 Refresh Token 重用检测。

详见 [`AUTH-OIDC.md`](AUTH-OIDC.md)。

## 6. 非功能要求

| 维度 | 基线 |
|---|---|
| 安全 | OWASP ASVS 思路；具体要求见 `SECURITY.md` |
| 可用性 | 优雅停机；数据库故障时 readiness 失败 |
| 可观测性 | JSON 日志、request ID、基础指标、健康检查 |
| 性能 | 以基准测试结果为准，不预先承诺未经验证的 QPS |
| 数据一致性 | 积分、解锁、授权码消费等关键路径使用事务和幂等 |
| 可访问性 | 面向 WCAG 2.2 AA，支持键盘和屏幕阅读器 |
| API 稳定性 | `/api/v1` 内保持兼容，OpenAPI 是事实来源 |
| 数据库兼容 | SQLite、MySQL、MariaDB 均在 CI 执行迁移和集成测试 |
| 备份 | 数据库、附件和 OIDC 私钥纳入恢复演练 |
| 隐私 | 最小采集、导出、注销延迟删除和日志保留策略 |

## 7. 分阶段实施计划

### 阶段 0：设计冻结

- 完成架构、API、授权、数据库、会话、审核、存储和运维规格。
- 建立 OpenAPI、迁移校验和三数据库 CI。

### 阶段 1：可上线的核心论坛（v1.0）

- 用户、邮箱验证、Session、找回密码。
- RBAC、全局/板块角色。
- 板块、文章、讨论、回复、标签、编辑历史。
- 审核、举报、处罚、审计。
- 默认主题、管理后台、基础 SEO。
- SQLite/MySQL/MariaDB 安装与备份流程。

### 阶段 2：社区经济与媒体（v1.0.x）

- 附件、本地/S3、缩略图。
- 积分、货币、等级。
- 回复/等级/付费可见。
- 通知、邮件、任务队列和 Outbox。

### 阶段 3：统一登录（v1.1）

- OIDC Provider 最小安全子集。
- 客户端管理、同意记录、密钥轮换和 conformance 测试。

### 阶段 4：安全扩展（v1.2）

- 数据型主题。
- 配置型插件和已预编译 UI 扩展。
- 搜索、RSS 完善和导入导出。

### 阶段 5：受限代码扩展（v2，研究项）

- 签名的代码型主题分发。
- Capability-based WASM 插件沙箱。
- 插件市场与兼容性策略。

## 8. 文档索引

| 文档 | 作用 |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | 进程、请求链路、模块与信任边界 |
| [`SCHEMA.md`](SCHEMA.md) | 双数据库约定和逻辑数据模型 |
| [`API.md`](API.md) | API 版本、错误、分页、幂等与缓存 |
| [`AUTHORIZATION.md`](AUTHORIZATION.md) | RBAC/ABAC 与板块作用域 |
| [`AUTH-OIDC.md`](AUTH-OIDC.md) | Session 与 OIDC Provider |
| [`MODERATION.md`](MODERATION.md) | 举报、处罚和审核状态机 |
| [`SECURITY.md`](SECURITY.md) | 威胁模型与安全基线 |
| [`FRONTEND.md`](FRONTEND.md) | SvelteKit 结构、SSR、SEO 和可访问性 |
| [`THEME.md`](THEME.md) | 数据型和构建时主题协议 |
| [`PLUGIN.md`](PLUGIN.md) | 配置型插件与未来 WASM 边界 |
| [`JOBS.md`](JOBS.md) | 后台任务、Outbox 和重试 |
| [`STORAGE.md`](STORAGE.md) | 文件与对象存储 |
| [`OPERATIONS.md`](OPERATIONS.md) | 配置、部署、升级、备份和恢复 |
| [`TESTING.md`](TESTING.md) | 测试矩阵与验收标准 |

## 9. 已冻结的关键决策

- 后端只返回 JSON/协议响应，不渲染论坛 HTML。
- 生产环境前后端同源，默认不开放 CORS。
- 后端不存在内网免鉴权。
- 数据库时间统一为 Unix 毫秒 `BIGINT`。
- UUID v7 由应用生成；SQLite 用 `TEXT(36)` 语义，MySQL/MariaDB 用 `CHAR(36)`。
- 用户内容只接受 Markdown；后端生成并清洗 HTML。
- v1 不运行上传的前端代码或插件代码。
- SQLite 与 MySQL/MariaDB 是安装时选择，不承诺直接切换连接串完成在线迁移。
- OIDC Access Token 使用 opaque token；ID Token 使用 RS256 JWT。
- 审计、权限、积分账本和审核是核心模块，不是可关闭插件。
