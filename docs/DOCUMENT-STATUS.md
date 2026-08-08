# BBLBB — 文档基线、版本与发布矩阵

> 基线版本：v0.5
> 状态：Frozen；产品所有者于 2026-08-04 确认。进入实现后仍必须通过后端、安全、前端、测试和运维发布评审。
> 产品决策事实来源：[`PRODUCT-DECISIONS.md`](PRODUCT-DECISIONS.md)。
> 事实来源优先级：本文件与 `REQUIREMENTS.md` → `openapi/openapi.yaml`（接口）→ `SCHEMA.md`/迁移（数据）→ 专项领域文档 → 原型文档。

## 1. 文档状态

| 档案 | 状态 | 负责领域 | 说明 |
|---|---|---|---|
| `PRODUCT-DECISIONS.md` | Owner confirmed | 产品 | 2026-08-04 交互式问答确认的产品决策 | 
| `REQUIREMENTS.md` | Frozen v0.5 | 产品 | 范围、非目标、v1.0 内部里程碑和全局决策 |
| `ARCHITECTURE.md` | Frozen candidate | 架构 | 进程边界和一致性 |
| `openapi/openapi.yaml` | Contract source | 后端 | API 字段、路径、请求/响应和机器可读安全扩展的事实来源 |
| `API.md` / `API-CONTRACTS.md` | Contract guide | 后端 | 跨端点规则与资源 DTO；必须与 OpenAPI 同步 |
| `SCHEMA.md` | Logical baseline | 后端/数据库 | 逻辑模型；DDL 必须同步 |
| `AUTH-OIDC.md` | Frozen candidate | 身份 | Session、CSRF、OIDC |
| `AUTHORIZATION.md` | Frozen candidate | 安全 | RBAC、对象权限和 Scope |
| `MARKETPLACE.md` | Frozen candidate | 交易 | 依赖 `MARKETPLACE-ACCOUNTING.md` |
| `DOWNLOAD-BILLING.md` | Frozen candidate | 经济/附件 | 策略解释见本文档和 Schema |
| `AI.md` | Frozen candidate | AI/隐私 | Provider、同意和建议 |
| `VIDEO-PLUGIN.md` | Frozen candidate | 媒体/安全 | 受控视频服务和 Provider |
| `SECURITY.md` | Frozen candidate | 安全 | 全局威胁模型和安全基线 |
| `MARKDOWN.md` | Implementation | 安全/后端/前端 | Markdown 渲染管线、策略版本与升级/缓存/回滚手册（M04-MARKDOWN） |
| `TESTING.md` | Release gate | 测试 | 自动化、恢复和上线门槛 |
| `OPERATIONS.md` | Release gate | 运维 | 部署、备份、告警和故障处理 |
| `FRONTEND.md` | Frozen candidate | 前端 | SvelteKit、SSR、SEO、a11y |
| `CRAWLER-POLICY.md` | Frozen candidate | 搜索/安全 | AI 爬虫、索引投影、批量访问和缓存边界 |
| `SEARCH.md` | Frozen candidate | 后端/搜索 | 搜索索引存储契约：文档模型、source/policy revision、跨库 FTS 策略 |
| `THEME.md` | Implementation | 主题/前端/安全 | 数据型主题封闭 Token schema、fallback、revision 一致性与管理 API（M13-THEME 已实现，0057_theme.sql） |
| `PLUGIN.md` | Implementation | 插件/安全 | v1 配置型插件 capability 白名单、无在线代码执行路径、调用摘要审计（M13-PLUGIN 已实现） |
| `PROTOTYPE-IA.md` / `PROTOTYPE-UI.md` | Reference | 产品/前端 | 原型路由、流程和视觉规范，不替代 API |

## 2. v0.5 v1.0 发布矩阵

所有已确认能力均属于 v1.0 目标；“内部里程碑”只表示实现顺序。可选能力仍可默认关闭，未通过专项门槛不得启用。

| 能力 | v1.0 内部里程碑 | 默认状态 | 依赖 | 未配置/故障行为 |
|---|---|---|---|---|
| 核心论坛、邮箱验证、审核、权限 | M1 | 开启 | 数据库、Session | 不可关闭 |
| 搜索、RSS/Atom、SEO 与防爬 | M1/M3 | 公开内容开启，AI 训练爬虫拒绝 | 索引、任务、隐私矩阵 | 核心阅读和发帖不受影响 |
| 本地/S3 附件 | M2 | 本地开启，S3 可选 | Storage Adapter | 保持当前后端，不丢失对象 |
| 积分、等级、受限内容 | M2 | 开启 | 不可变账本 | 核心规则继续生效 |
| 内部商城、装扮、自动签到与活跃 | M2 | 商城开启，活动按规则启用 | 账本、附件、任务 | 停止新购买/奖励，已有装扮安全展示 |
| 下载抵扣 | M2 | 关闭，管理员开启 | 附件、账本、授权 | 按管理员策略免费或拒绝新下载，历史授权不变 |
| 视频 Direct/HLS/Xigua | M2 | Provider 按策略开启 | Video Service、CSP | 降级为外链卡片，不阻塞发帖 |
| AI Gateway | M3 | 关闭，管理员开启 | Worker、Provider、逐次同意 | 普通发帖和人工审核继续 |
| OIDC Provider | M4 | 关闭，通过专项门槛后开启 | 密钥、Consent、Conformance | 本地登录不受影响 |
| Marketplace | M4 | 关闭，逐应用逐 Scope 审批 | OIDC、账本、Outbox | 不创建新 Intent，历史交易可查询 |
| 代码型/WASM 插件 | v2 研究项 | 关闭 | 独立沙箱和签名 | v1.0 不提供 |

## 3. 冻结规则

- 任何 API、状态、权限、账务或隐私语义变更，必须同时更新本文件、OpenAPI、Schema、Security、Testing 和对应专项文档。
- 文档中的“建议”仅表示默认值；涉及金额、权限、隐私、外部网络和状态迁移的内容必须改成明确规则后才能编码。
- 原型可以提前演示，但不得成为安全、价格、权限或可见性的事实来源。
- v0.5 的可选能力按 Feature Flag 隔离；未配置外部 Provider 或专项门槛未通过时，核心论坛必须保持可用。
- v1.0 范围与内部里程碑不得被实现团队自行改写为 v1.1/v1.2；范围变更必须重新取得产品所有者确认。
