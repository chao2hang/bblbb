# BBLBB — 产品需求与实施边界

> 文档版本：v0.5
> 状态：Frozen（产品所有者于 2026-08-04 确认；统一基线见 [`DOCUMENT-STATUS.md`](DOCUMENT-STATUS.md)；产品决策见 [`PRODUCT-DECISIONS.md`](PRODUCT-DECISIONS.md)）
> 产品定位：公开兴趣社区，兼具博客发布体验、论坛互动、积分经济和可配置扩展能力；v1.0 同时包含核心论坛与已确认的扩展能力，但按 Feature Flag 和专项发布门槛分批启用。

## 1. 产品目标

BBLBB 面向个人、小团队和中小型社区，核心目标是：

- 在 512MB 级服务器上以 SQLite 模式稳定运行。
- 同一套程序可选择 SQLite、MySQL 8 或 MariaDB 10.11 作为数据库。
- 提供文章、讨论、板块、标签、回复和管理后台。
- 提供用户、角色、板块级权限、审核和完整审计。
- 提供可审计的多币种积分与等级体系；B币不可充值、提现、转账或兑换现实价值。
- 支持公开、登录后、回复后、等级和付费解锁，且不会通过 API、缓存、搜索、RSS、SEO 或 AI 摘要泄漏隐藏内容。
- 对疑似批量爬取采用降速、429、挑战、临时封禁和人工复核的分级处置。
- 主题和插件保持可扩展，但不以运行任意第三方代码为 v1 目标。
- v1.0 目标包含标准、可验证的 OIDC Provider；默认关闭，完成专项 conformance、安全和密钥轮换门槛后才启用。

## 2. 明确的非目标

v1 不包括：

- 即时聊天、私信和实时音视频通信；但允许通过受控视频插件引用第三方视频页面/流媒体。
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

- 面向公众开放邮箱、用户名和密码注册。
- 强制邮箱验证、重发验证邮件、找回和重置密码；未验证用户只能登录浏览、修改账号和重发验证，不能发帖、回复、上传、交易或领取活动奖励。
- 邮箱验证后默认立即可用；后台可以启用新用户冷静期并配置时长和受限动作。
- 多设备 Session 查看和逐个撤销。
- 修改密码后撤销其他 Session。
- 昵称、头像、签名、自我介绍、时区和主题偏好。
- 用户状态：待验证、正常、受限、封禁、待删除、已删除。
- 普通用户可选 TOTP；administrator、moderator 和高风险账务操作账号强制 TOTP，未 enrollment 不得获得高权限会话。Passkey/WebAuthn 预留到后续版本。

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
- 匿名访客可访问允许公开的帖子/文章、搜索、用户主页/资料卡、图片/视频和 RSS/Atom。
- RSS/Atom、sitemap、动态 robots、Open Graph、JSON-LD；作者可让单篇公开内容退出搜索和 AI 摘要，管理员禁止策略优先。
- 搜索属于 v1.0；SQLite 使用 FTS5，MySQL/MariaDB 使用全文索引，并在结果层重新执行内容可见性过滤。

### 5.4 内容访问策略

隐藏内容独立存储，公开正文只包含占位符。v1.0 只支持以下策略，不支持指定用户可见：

- 公开可见。
- 登录后可见。
- 回复后可见。
- 达到最低等级可见；最低等级不得高于作者发布时当前等级。
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

### 5.6 积分、货币、内部商城和等级

- 管理员可定义多种货币，例如经验、金币和贡献。
- 内部积分商城使用 `coin` 购买昵称颜色、头像挂件、头像边框、徽章、主页/帖子装饰、互动反应包和活跃道具；商品、库存、价格、装备槽位和展示 Token 均由 Rust 核心服务裁决。
- 装扮可全局展示在用户资料、帖子作者行、回复、通知和榜单；用户可卸下、关闭动画或隐藏他人装饰。
- 登录用户每日首次有效页面访问自动完成签到，不要求人工点击；签到、任务、有效回复、被互动和社区活动可以产生受限奖励，每日上限、时区、去重、冷却和反刷规则不可绕过。
- 商城购买使用不可变账本、幂等和库存锁；不能购买权限、审核结果、隐藏内容访问权或管理员身份。
- 完整规则见 [`INTERNAL-MARKETPLACE.md`](INTERNAL-MARKETPLACE.md)。
- 每个用户每种货币一个账户。
- 所有余额变更必须在同一事务内更新账户并追加不可变流水。
- 支持规则奖励、消费、冻结、解冻、管理员调整和补偿交易。
- 创建或消费请求支持幂等键，防止重试导致重复入账。
- 经验建议采用累计值并用于等级；可消费货币不参与等级，避免消费导致降级。
- 管理员撤销操作通过反向补偿流水完成，不修改历史流水。

### 5.7 公开市场与原子交易

- 用户可创建经管理员批准的市场应用，通过公开、版本化 API 接入自建市场。
- 市场以服务端登记的 Offer 和短效 Checkout Intent 发起购买；不得直接提交可信金额、收款方或修改用户余额。
- 普通 OIDC 登录 scope 不具备扣款能力；购买使用独立高风险 scope、用户明确同意、短效 Token 和强制幂等键。
- 购买记录、意图消费、库存条件更新、账户扣款、不可变流水、审计和 Outbox 必须在同一数据库事务内提交。
- 同步响应只在事务提交后返回成功；Webhook 由 Outbox 在提交后签名投递，延迟不改变购买事实。
- 退款通过引用原交易的补偿流水完成，禁止修改或删除历史交易；提供查询、增量对账、限额、风控和紧急禁用能力。

详见 [`MARKETPLACE.md`](MARKETPLACE.md)。

### 5.8 附件与媒体

- 本地磁盘和 S3 兼容对象存储适配器；目标兼容 AWS S3、MinIO、Cloudflare R2。
- 头像、封面和帖子附件，Bucket 默认私有。
- 附件对象持续保存，只有用户主动删除、业务保留策略或管理员清理才删除；S3 公开/预签名链接必须有可配置有效期，链接过期不删除附件，可重新鉴权签发。
- 用户等级可在后台配置单附件最大字节数和附件总容量；上传时与站点、用途、板块及处罚限制共同计算，取最严格值。
- 文件大小、MIME、扩展名、用户配额与板块规则校验。
- S3 凭证只由后端读取，支持预签名上传/下载但不能绕过服务端 HEAD、hash、病毒/图片处理和权限校验。
- 后台提供 `/admin/storage` 配置与连接测试；切换后端必须有迁移、校验和回滚说明。
- 图片重新解码、缩略图、SVG 限制、下载权限和孤儿清理。
- 通过受控视频插件支持 MP4/WebM/OGV、HLS `.m3u8` 和西瓜视频公开页面引用；不抓取、转存或绕过第三方鉴权。

详见 [`STORAGE.md`](STORAGE.md) 和 [`DOWNLOAD-BILLING.md`](DOWNLOAD-BILLING.md)。

### 5.9 大模型辅助能力

- 通过后端 AI Gateway 接入多个可配置 Provider，用于发帖格式化、内容审计建议、摘要/标签和 SEO 草稿。
- Provider URL、Secret、模型、用途路由、超时、并发、预算、脱敏和用户同意均可在后台配置；浏览器不直连 Provider。
- 模型只能输出建议或风险信号，不能直接发布、删除、封禁、改权限、改价格或扣积分；最终操作仍由 Rust、用户或管理员确认。
- 用户原文默认脱敏后发送；隐藏内容、私密审核备注和 Secret 不发送；需要完整内容时必须单独同意并可撤回。
- AI 网络调用异步化、可取消、可重试、可熔断；模型不可用不能绕过核心审核，也不能阻塞普通发帖。
- AI 输出保存为版本化 suggestion，采纳时要求版本校验、再次鉴权和 XSS/Markdown/SEO 校验。

详见 [`AI.md`](AI.md)。

### 5.10 通知与后台任务

- 回复、引用、@提及、审核、等级变化和 OIDC 安全通知。
- 站内通知与邮件通知。
- 邮件、搜索索引、缩略图、定时发布、过期数据清理通过数据库任务队列执行。
- 关键业务事件通过 Transactional Outbox 保证最终送达。

详见 [`JOBS.md`](JOBS.md)。

### 5.11 主题

v1 支持两类主题能力：

1. **数据型主题**：CSS Token、Logo、字号和密度，可运行时安全切换。
2. **可信代码型主题**：构建时编译进 SvelteKit，可运行时在已编译主题之间切换。

上传新的 Svelte 代码不能即时执行，安装代码型主题必须重新构建和部署前端。详见 [`THEME.md`](THEME.md)。

### 5.12 插件

- v1：配置型后端规则和已预编译的前端扩展组件。
- v1 不接受任意网络请求、任意 SQL 或运行时上传代码。
- 审计、权限、积分账本和审核属于核心能力，不可被插件替代或关闭。
- WASM 运行时属于 v2 研究范围，并以 capability、资源配额和签名为前提。

详见 [`PLUGIN.md`](PLUGIN.md)。

### 5.13 OpenID Connect Provider

v1.0 目标包含最小、安全的 OIDC 子集；该能力默认关闭，只有通过专项安全与协议一致性门槛后才可启用：

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

## 7. v1.0 内部实施里程碑

以下里程碑全部属于 v1.0 范围。里程碑顺序用于控制工程风险，不表示拆分为 v1.1 或 v1.2；未达到专项门槛的可选能力必须保持 Feature Flag 关闭。

### 里程碑 0：需求冻结与工程契约

- 完成架构、API、授权、数据库、会话、审核、存储、隐私和运维规格。
- 建立并冻结 `openapi/openapi.yaml`、迁移校验和 SQLite/MySQL/MariaDB CI。
- 明确部署地区后完成法律、隐私和运营责任评审。

### 里程碑 1：可上线的核心论坛

- 用户、邮箱验证、Session、找回密码、TOTP 和新用户冷静期。
- RBAC、全局/板块角色、对象级授权。
- 板块、文章、讨论、回复、标签、编辑历史和内容可见性。
- 搜索、RSS/Atom、sitemap、动态 robots 和作者退出索引。
- 审核、举报、处罚、申诉、审计、管理后台和三数据库安装/备份流程。

### 里程碑 2：社区经济、附件与媒体

- 本地/S3 附件、共享容量、缩略图和短效签名 URL。
- 积分、货币、等级、内部商城、装扮、自动签到和活跃奖励。
- 回复/等级/付费可见和下载抵扣；下载抵扣默认关闭。
- 核心 Video Service 与 Direct/HLS/西瓜 Provider；不安全来源降级为外链。
- 通知、邮件、任务队列和 Transactional Outbox。

### 里程碑 3：AI 辅助与防爬

- 受控 AI Gateway、逐次明确同意、Provider 预算和版本化 Suggestion；默认关闭。
- 高风险内容暂不公开、AI 仅提供建议、人工审核完成最终裁决。
- AI 训练爬虫默认拒绝、页面级索引控制、行为检测和分级处置。

### 里程碑 4：统一登录与第三方市场

- OIDC Provider 最小安全子集、客户端管理、同意记录、密钥轮换和 conformance 测试。
- Marketplace 逐应用、逐 Scope 审批；采用托管双边站内账本、短效 Checkout Intent 和补偿退款。
- OIDC 与 Marketplace 默认关闭，只有通过专项安全、协议和账务门槛后才可启用。

### v2 研究项：受限代码扩展

- 签名的代码型主题分发。
- Capability-based WASM 插件沙箱。
- 插件市场与兼容性策略。

## 8. 文档索引

| 文档 | 作用 |
|---|---|
| [`DOCUMENT-STATUS.md`](DOCUMENT-STATUS.md) / [`CHANGELOG.md`](CHANGELOG.md) | 文档版本、事实来源、功能发布矩阵和变更记录 |
| [`TERMINOLOGY.md`](TERMINOLOGY.md) | 跨产品、API、Schema 和 UI 的统一术语 |
| [`STATE-MACHINES.md`](STATE-MACHINES.md) | 领域状态、合法迁移与终止态 |
| [`ERROR-CODES.md`](ERROR-CODES.md) | 稳定 Problem code 注册表 |
| [`PERMISSION-MATRIX.md`](PERMISSION-MATRIX.md) | Endpoint、Scope、Permission、CSRF 和审计 |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | 进程、请求链路、模块与信任边界 |
| [`SCHEMA.md`](SCHEMA.md) | 双数据库约定和逻辑数据模型 |
| [`openapi/openapi.yaml`](../openapi/openapi.yaml) | 机器可读 API 契约和字段事实来源 |
| [`API.md`](API.md) / [`API-CONTRACTS.md`](API-CONTRACTS.md) | API 跨端点规则、资源 DTO、错误、分页、幂等与缓存 |
| [`AUTHORIZATION.md`](AUTHORIZATION.md) | RBAC/ABAC 与板块作用域 |
| [`AUTH-OIDC.md`](AUTH-OIDC.md) | Session 与 OIDC Provider |
| [`MODERATION.md`](MODERATION.md) | 举报、处罚和审核状态机 |
| [`SECURITY.md`](SECURITY.md) | 威胁模型与安全基线 |
| [`CRAWLER-POLICY.md`](CRAWLER-POLICY.md) | 搜索索引、AI 爬虫、页面投影和批量访问处置 |
| [`FRONTEND.md`](FRONTEND.md) | SvelteKit 结构、SSR、SEO 和可访问性 |
| [`THEME.md`](THEME.md) | 数据型和构建时主题协议 |
| [`PLUGIN.md`](PLUGIN.md) | 配置型插件与未来 WASM 边界 |
| [`JOBS.md`](JOBS.md) | 后台任务、Outbox 和重试 |
| [`STORAGE.md`](STORAGE.md) | 文件与对象存储 |
| [`OPERATIONS.md`](OPERATIONS.md) | 配置、部署、升级、备份和恢复 |
| [`INTERNAL-MARKETPLACE.md`](INTERNAL-MARKETPLACE.md) | 内部积分商城、装扮、签到、活跃任务和反应 |
| [`MARKETPLACE.md`](MARKETPLACE.md) / [`MARKETPLACE-ACCOUNTING.md`](MARKETPLACE-ACCOUNTING.md) | 市场协议、账务、结算和身份时序 |
| [`DOWNLOAD-BILLING.md`](DOWNLOAD-BILLING.md) | 下载授权、策略优先级和原子扣费 |
| [`AI.md`](AI.md) | AI Gateway、同意、Task 和 Suggestion |
| [`VIDEO-PLUGIN.md`](VIDEO-PLUGIN.md) | 核心 Video Service、Provider 和播放边界 |
| [`CONFIGURATION.md`](CONFIGURATION.md) | 配置、Secret 与运行时变更矩阵 |
| [`EVENT-CATALOG.md`](EVENT-CATALOG.md) | Outbox 领域事件和审计目录 |
| [`RETENTION-PRIVACY.md`](RETENTION-PRIVACY.md) | 数据保留、导出、注销和第三方隐私 |
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
