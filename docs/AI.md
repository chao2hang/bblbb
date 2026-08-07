# BBLBB — 大模型能力与 AI Gateway

> 版本：v0.4
> 大模型用于辅助格式化、内容审计、SEO 和运营分析；Rust 后端保留最终权限、审核、发布和数据裁决权。

## 1. 能力范围

支持配置多个 OpenAI-compatible 或厂商原生适配器，用途按策略选择模型：

- 发帖/文章格式化：Markdown 修复、标题层级、列表/代码块格式，不改变事实和作者意图。
- 内容审计辅助：垃圾、广告、疑似违规、重复内容、风险类别和置信度建议，进入人工审核队列。
- SEO 优化：标题、摘要、canonical 建议、关键词、OpenGraph 描述和 JSON-LD 草稿。
- 标签和摘要建议：仅作为用户或管理员可编辑的草稿。
- 低风险批处理：对已公开内容生成搜索摘要或 SEO 草稿。

模型不能直接：发布/拒绝帖子、封禁用户、修改权限、改变付费价格、读取 Secret、访问隐藏内容并向不应可见的人输出、执行 SQL、调用任意 URL 或扣除积分。

## 2. AI Gateway 与配置

浏览器和插件不得直连模型供应商。所有请求经过 Rust AI Gateway：

```text
SvelteKit/后台 → Rust AI Gateway → Provider Adapter → 外部模型 API
```

后台 `/admin/ai` 可配置：

- Provider 名称、API Base URL、API 类型、模型名和用途路由。
- Secret：只写入受保护 Secret Store/加密配置，GET 只返回 `secret_configured`，不进入浏览器、localStorage、SSR payload、日志或导出。
- 超时、最大输入/输出 token、并发数、每用户/每日预算、失败重试和降级模型。
- 数据发送策略：`disabled/metadata_only/redacted/full_with_consent`；默认 `redacted`。
- 是否允许把用户内容发送给外部 Provider、保留期限、Provider 的训练/留存声明和区域。
- 各功能启停：`formatting/moderation/seo/tagging`。

Base URL 不是任意代理入口：生产必须 HTTPS、精确域名白名单、DNS 解析时阻断 loopback/私网/链路本地地址，限制响应大小、超时和重定向；禁止 SSRF 和任意工具调用。Provider 密钥支持轮换，旧密钥验证成功后再撤销。

## 3. 数据与隐私

- 发送前进行字段最小化和脱敏：移除邮箱、Session、Token、内部用户 ID、附件签名 URL、私密审核备注和不必要的 IP/设备信息。
- `full_with_consent` 必须在明确说明 Provider、用途、保留期和跨境风险后由用户单独同意；不能用通用注册同意代替。
- 隐藏正文默认不送外部模型。审核员可在权限范围内触发，并按审核留痕；模型结果不能扩大内容可见性。
- Prompt、模型返回和用户内容均视为不可信数据。模型输出必须当作纯文本/结构化 JSON 校验，禁止直接拼接 HTML、SQL、模板、URL 或系统指令。
- 记录最小 AI 审计：任务 ID、用途、provider/model、策略版本、输入摘要 hash、输出摘要 hash、耗时、token 用量、结果和人工采纳状态。默认不保存原文和完整 prompt/response。
- 用户可撤回 AI 辅助同意；撤回后停止新任务，已生成草稿按保留策略处理。

## 4. 发帖流程

### 用户主动格式化

1. 用户点击“AI 格式化/优化”，Rust 检查登录、CSRF、发帖权限、内容长度和 AI 策略。
2. 发送脱敏草稿，返回结构化候选：`title/content/summary/tags/changes`。
3. 前端以 diff 或明确的替换预览显示；用户必须手动采纳，不能静默覆盖输入。
4. 最终发布再次执行 Rust Markdown 清洗、内容规则、附件状态、权限和必要审核；AI 结果不是发布凭证。

### 发布前内容审计

- 可同步执行低延迟规则检查，但模型超时不得绕过核心规则；可返回 `pending_ai_review`。
- AI 只能提供 `risk_categories`、`score`、`evidence_spans` 和 `recommendation`，不能自动执行永久封禁或删除。
- 高风险动作由人工审核确认，记录规则版本、模型版本和人工决定。
- 通过后发帖事务与 Outbox 仍按原有核心流程执行。

### SEO

- 只对公开、已发布内容生成 SEO 草稿。
- 不向 OpenGraph、sitemap、搜索索引写入未人工/规则校验的模型输出。
- SEO 字段由作者或管理员采纳后写入帖子 revision，并重新进行长度、URL、XSS 和隐藏内容泄漏校验。

## 5. API

### 同意模型

同意按 `(user_id, provider_id, purpose, data_mode, disclosure_version)` 记录，不使用一个全局开关。`full_with_consent` 必须保存当时展示的 Provider、用途、保留期、训练使用、区域/跨境信息和文案 hash。撤回后禁止新任务；排队任务取消，运行中尽力取消且丢弃迟到输出，历史最小审计按保留矩阵处理。

### Task 与 Suggestion 响应

- 生成接口默认返回 HTTP 202，响应为 `{ task_id, status: "queued", poll_url, cancel_url, source_revision, policy_version }`。
- 仅当能力声明 `synchronous=true` 且在短预算内完成时可返回 HTTP 200，并返回同一 Task 投影和 `suggestion`；两种响应使用同一 schema union。
- 用户通过 `GET /api/v1/ai/tasks/{id}` 查询本人任务，通过 `POST /api/v1/ai/tasks/{id}/cancel` 取消尚未结束任务；管理员端点不能扩大任务内容可见性。
- Suggestion payload 按 `formatting/seo/tagging/moderation` 使用独立版本化 schema。审核 Suggestion 只对有目标审核权限者可见，作者默认只见公开审核结果而不见内部风险信号。
- `accept` 使用目标 `base_version`/`If-Match`；格式化、SEO、标签分别写入对应草稿或 revision，审核建议只创建人工审核动作草稿，不直接处罚。

```text
GET  /api/v1/ai/capabilities
POST /api/v1/ai/drafts/{draft_id}/format
POST /api/v1/ai/posts/{post_id}/moderation-suggestion
POST /api/v1/ai/posts/{post_id}/seo-suggestion
GET  /api/v1/ai/tasks/{id}
POST /api/v1/ai/tasks/{id}/cancel
GET  /api/v1/ai/suggestions/{id}
POST /api/v1/ai/suggestions/{id}/accept
POST /api/v1/ai/consent
DELETE /api/v1/ai/consent

GET   /api/v1/admin/ai/config
PATCH /api/v1/admin/ai/config
POST  /api/v1/admin/ai/providers/test
GET   /api/v1/admin/ai/tasks
POST  /api/v1/admin/ai/tasks/{id}/retry
POST  /api/v1/admin/ai/tasks/{id}/cancel
```

所有生成接口要求登录、CSRF（Session 请求）、用途权限、限流和幂等键；响应 `Cache-Control: private, no-store`。后台测试接口不接受用户正文，使用固定脱敏探针。错误只返回稳定错误码，不回显 Provider 响应、Secret、Prompt 或隐藏正文。

## 6. 任务与一致性

- 模型网络调用不放在帖子/审核/积分数据库写事务中。
- 用户主动格式化是可取消、可重试的 job；结果保存为草稿 suggestion，不直接改核心内容。
- `post.published` 后的 SEO/摘要/标签使用 Transactional Outbox 触发异步任务。
- 内容审核建议失败、超时或 Provider 不可用时，核心安全规则继续执行；高风险策略可选择进入人工审核，不能自动放行。
- Job 至少一次执行，使用 `(task_type, target_id, policy_version, idempotency_key)` 去重；旧模型结果不能覆盖新 revision。
- Provider 只返回结果，不能回调核心写操作；采纳 suggestion 时再次鉴权并用版本号/If-Match 防止覆盖新编辑。

## 7. 成本与可用性

- 每个 Provider、用途、用户和站点都有 token/金额预算；达到预算返回明确的 `ai_budget_exceeded`，不静默切换到未批准 Provider。
- 默认短超时、有限重试、指数退避和熔断；不因模型故障阻塞普通发帖。
- 监控成功率、延迟、token 用量、预算、脱敏失败、Provider 4xx/5xx、任务堆积和人工采纳率。
- 供应商不支持零数据留存或训练隔离时，后台必须明确警告；敏感内容策略可完全关闭外部发送。

## 8. 验收

必须覆盖 Prompt injection、模型输出 XSS/SQL/模板注入、隐藏内容泄漏、Secret 泄漏、SSRF/DNS 重绑定、越权读取、重复任务、旧 revision 覆盖、Provider 超时/重试/熔断、预算超限、用户撤回同意、人工审核不可绕过和无 JavaScript 发帖流程。

## 9. 实现状态（M09）

实现位置：`backend/src/ai/`（gateway/consent/tasks/suggestions）+ 
`backend/src/routes/ai.rs`、`backend/src/routes/admin.rs`（AI 管理）+ 
`migrations/*/0052_ai_gateway.sql` + `backend/tests/ai/`。

- **Gateway**：`EgressPolicy` 纯函数裁决（HTTPS/host allowlist/端口/私网
  阻断/重定向/超时/响应上限）+ `ProviderClient` trait（reqwest 生产实现，
  mock 测试）；`Redactor` 默认脱敏（Disabled/MetadataOnly 外发为空），邮箱
  全量剥离；`BudgetCounter` 预算/并发/熔断。
- **Consent**：`ai_consents` 逐次同意，(user,provider,purpose) 唯一；撤回后
  禁止新任务，`execute_task` 执行前重确认，撤回的任务置 `dead`。
- **Tasks**：`ai_tasks` 状态机 `queued → running → retry_wait → running → … →
  succeeded | cancelled | dead`；幂等入队（唯一键 + 唯一约束容忍）；至少一次
  消费（原子占位）；错误分类 5xx/429/超时 → retry，4xx → dead。
- **Suggestions**：`ai_suggestions` schema_version + base_revision 防旧覆盖新；
  采纳时重新鉴权 + If-Match 幂等；模型输出解析校验拒绝注入形态。
- **Feature Flag**：`FeatureName::Ai` 默认关闭；未启用时路由返回
  `feature_disabled`。Provider 仅在 `data_mode=full_with_consent` 时外发正文；
  Secret 不落库、API/SSR/日志/审计全脱敏。
- 测试：`backend/tests/ai/{gateway,tasks,suggestions}.rs`（27 例）+ 路由/管理
  集成覆盖于 `tests/` 既有 HTTP 测试；默认关闭场景不回归既有门禁。
