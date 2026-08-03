# BBLBB

BBLBB 是一个使用 **Rust + SvelteKit** 构建的轻量社区论坛，兼顾博客式内容发布与论坛讨论体验。

项目当前处于 **v0.3 设计冻结与工程准备阶段**，仓库暂以可实施规格文档为主。

## 核心目标

- SQLite 默认运行，同时支持 MySQL 8 和 MariaDB 10.11
- 用户、Session、多角色和板块级权限管理
- 文章、讨论、板块、标签与楼层回复
- 举报、审核、处罚、申诉与不可变审计
- 积分、货币、等级与受限内容解锁
- 数据型主题与配置型插件
- 后续作为 OpenID Connect Provider 为其他站点提供统一登录
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
| [`REQUIREMENTS.md`](docs/REQUIREMENTS.md) | 产品范围、阶段计划和冻结决策 |
| [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) | 进程、请求链路与模块边界 |
| [`SCHEMA.md`](docs/SCHEMA.md) | 双数据库数据模型与事务规则 |
| [`API.md`](docs/API.md) | API 版本、错误、分页、幂等与缓存 |
| [`AUTH-OIDC.md`](docs/AUTH-OIDC.md) | 本地认证、Session、CSRF 与 OIDC |
| [`AUTHORIZATION.md`](docs/AUTHORIZATION.md) | RBAC、板块角色和对象级权限 |
| [`MODERATION.md`](docs/MODERATION.md) | 举报、审核、处罚与申诉 |
| [`SECURITY.md`](docs/SECURITY.md) | 威胁模型和安全基线 |
| [`FRONTEND.md`](docs/FRONTEND.md) | SvelteKit、SSR、SEO 与可访问性 |
| [`THEME.md`](docs/THEME.md) | 数据型和可信代码型主题 |
| [`PROTOTYPE-IA.md`](docs/PROTOTYPE-IA.md) | 原型信息架构、路由与页面流程 |
| [`PROTOTYPE-UI.md`](docs/PROTOTYPE-UI.md) | 设计 Token 与组件系统规格 |
| [`PLUGIN.md`](docs/PLUGIN.md) | 配置型插件与未来 WASM 边界 |
| [`JOBS.md`](docs/JOBS.md) | 后台任务、Outbox 与重试 |
| [`STORAGE.md`](docs/STORAGE.md) | 本地/S3 附件和媒体处理 |
| [`OPERATIONS.md`](docs/OPERATIONS.md) | 部署、升级、备份与恢复 |
| [`TESTING.md`](docs/TESTING.md) | 三数据库与安全验收矩阵 |

## 原型

[`prototype/`](prototype/) 是无需构建的静态高保真原型，直接用浏览器打开 `prototype/index.html` 即可查看，对应 `docs/PROTOTYPE-IA.md` 与 `docs/PROTOTYPE-UI.md` 两份规格。

- 纯 HTML + CSS + 原生 JavaScript，无构建步骤、无依赖安装。
- `js/mock.js` 提供全部演示数据，原型不访问真实后端。
- 图标通过 CDN 加载 Lucide，离线环境下图标不显示，功能不受影响。

原型仅用于设计验证，不是生产代码；正式前端按 `docs/FRONTEND.md` 以 SvelteKit 实现。

## 当前状态

下一阶段将创建 Rust 后端和 SvelteKit 前端骨架、三数据库 CI、初始迁移及 OpenAPI 契约。
