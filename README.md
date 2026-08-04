# BBLBB

BBLBB 是一个使用 **Rust + SvelteKit** 构建的轻量社区论坛，兼顾博客式内容发布与论坛讨论体验。

项目的 **v0.5 正式需求基线已于 2026-08-04 冻结**，仓库当前以可实施规格文档和高保真原型为主；统一文档状态和 v1.0 发布矩阵见 [`docs/DOCUMENT-STATUS.md`](docs/DOCUMENT-STATUS.md)，已确认决策见 [`docs/PRODUCT-DECISIONS.md`](docs/PRODUCT-DECISIONS.md)。

## 核心目标

- SQLite 默认运行，同时支持 MySQL 8 和 MariaDB 10.11
- 用户、Session、多角色和板块级权限管理
- 文章、讨论、板块、标签与楼层回复
- 举报、审核、处罚、申诉与不可变审计
- 积分、货币、等级与受限内容解锁
- 内部积分商城、昵称/头像装扮、签到任务与社区互动氛围
- 安全公开市场 API、原子购买扣款与不可变入账
- 受控大模型 Gateway，用于格式化、内容审计和 SEO 辅助
- 视频嵌入插件，支持常见视频 URL、HLS 和西瓜视频安全引用
- 数据型主题与配置型插件
- v1.0 目标包含默认关闭的 OpenID Connect Provider，通过专项门槛后为其他站点提供统一登录
- SQLite 模式面向 512MB 级小型服务器

## 技术架构

```text
Caddy
├── /api/v1/*                         → Rust / axum
├── /.well-known/openid-configuration → Rust / OIDC
├── /oauth/*                          → Rust / OIDC
└── 其他                              → SvelteKit SSR
```

- 后端：Rust、axum、Tokio、sqlx
- 前端：SvelteKit、TypeScript、adapter-node
- 数据库：SQLite / MySQL / MariaDB
- 后台任务：Tokio worker + Transactional Outbox
- 部署入口：Caddy

## 文档

从 [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md) 开始：

| 文档 | 内容 |
|---|---|
| [`PRODUCT-DECISIONS.md`](docs/PRODUCT-DECISIONS.md) | 产品所有者通过交互式问答确认的正式决策 |
| [`DOCUMENT-STATUS.md`](docs/DOCUMENT-STATUS.md) / [`CHANGELOG.md`](docs/CHANGELOG.md) | v0.5 文档基线、事实来源、发布矩阵和变更记录 |
| [`REQUIREMENTS.md`](docs/REQUIREMENTS.md) | 产品范围、v1.0 内部里程碑和冻结决策 |
| [`STATE-MACHINES.md`](docs/STATE-MACHINES.md) | 全局状态机与稳定枚举 |
| [`PERMISSION-MATRIX.md`](docs/PERMISSION-MATRIX.md) | Endpoint、Scope、权限、CSRF 和审计矩阵 |
| [`ERROR-CODES.md`](docs/ERROR-CODES.md) | 稳定 API Problem code 注册表 |
| [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) | 进程、请求链路与模块边界 |
| [`SCHEMA.md`](docs/SCHEMA.md) | 双数据库数据模型与事务规则 |
| [`openapi/openapi.yaml`](openapi/openapi.yaml) | 机器可读 API 契约，作为接口字段事实来源 |
| [`API.md`](docs/API.md) / [`API-CONTRACTS.md`](docs/API-CONTRACTS.md) | API 跨端点规则、资源 DTO、错误、分页、幂等与缓存 |
| [`AUTH-OIDC.md`](docs/AUTH-OIDC.md) | 本地认证、Session、CSRF 与 OIDC |
| [`AUTHORIZATION.md`](docs/AUTHORIZATION.md) | RBAC、板块角色和对象级权限 |
| [`INTERNAL-MARKETPLACE.md`](docs/INTERNAL-MARKETPLACE.md) | 内部积分商城、装扮、签到、活跃任务和社区反应 |
| [`MARKETPLACE.md`](docs/MARKETPLACE.md) / [`MARKETPLACE-ACCOUNTING.md`](docs/MARKETPLACE-ACCOUNTING.md) | 自建市场接入、双边账务、结算、退款和 Webhook |
| [`DOWNLOAD-BILLING.md`](docs/DOWNLOAD-BILLING.md) | 下载抵扣积分、授权复用与后台计费策略 |
| [`AI.md`](docs/AI.md) | 大模型 Gateway、脱敏、审核辅助、SEO 和任务策略 |
| [`VIDEO-PLUGIN.md`](docs/VIDEO-PLUGIN.md) | 视频 URL、HLS、西瓜视频插件与 SSRF/CSP 策略 |
| [`MODERATION.md`](docs/MODERATION.md) | 举报、审核、处罚与申诉 |
| [`SECURITY.md`](docs/SECURITY.md) | 威胁模型和安全基线 |
| [`CRAWLER-POLICY.md`](docs/CRAWLER-POLICY.md) | 搜索索引、AI 爬虫、页面投影和批量访问策略 |
| [`FRONTEND.md`](docs/FRONTEND.md) | SvelteKit、SSR、SEO 与可访问性 |
| [`THEME.md`](docs/THEME.md) | 数据型和可信代码型主题 |
| [`PROTOTYPE-IA.md`](docs/PROTOTYPE-IA.md) | 原型信息架构、路由与页面流程 |
| [`PROTOTYPE-UI.md`](docs/PROTOTYPE-UI.md) | 设计 Token 与组件系统规格 |
| [`PLUGIN.md`](docs/PLUGIN.md) | 配置型插件与未来 WASM 边界 |
| [`JOBS.md`](docs/JOBS.md) | 后台任务、Outbox 与重试 |
| [`STORAGE.md`](docs/STORAGE.md) | 本地/S3 附件和媒体处理 |
| [`OPERATIONS.md`](docs/OPERATIONS.md) | 部署、升级、备份与恢复 |
| [`CONFIGURATION.md`](docs/CONFIGURATION.md) | 配置、Secret 和运行时变更矩阵 |
| [`EVENT-CATALOG.md`](docs/EVENT-CATALOG.md) | 领域事件、Outbox 和审计目录 |
| [`RETENTION-PRIVACY.md`](docs/RETENTION-PRIVACY.md) | 数据保留、导出、注销和第三方隐私 |
| [`TERMINOLOGY.md`](docs/TERMINOLOGY.md) | 统一业务术语 |
| [`TESTING.md`](docs/TESTING.md) | 三数据库与安全验收矩阵 |

## 原型

[`prototype/`](prototype/) 是无需构建的静态高保真原型，直接用浏览器打开 `prototype/index.html` 即可查看，对应 `docs/PROTOTYPE-IA.md` 与 `docs/PROTOTYPE-UI.md` 两份规格。

- 纯 HTML + CSS + 原生 JavaScript，无构建步骤、无依赖安装。
- `js/mock.js` 提供全部演示数据，原型不访问真实后端。
- 图标通过 CDN 加载 Lucide，离线环境下图标不显示，功能不受影响。

原型仅用于设计验证，不是生产代码；正式前端按 `docs/FRONTEND.md` 以 SvelteKit 实现。

## 当前状态

下一阶段将创建 Rust 后端和 SvelteKit 前端骨架、三数据库 CI、初始迁移及 OpenAPI 契约。
