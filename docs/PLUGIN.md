# BBLBB — 插件与扩展规范

> 版本：v0.4（M13 已实现：v1 配置型插件 + 管理 API + capability 白名单）
> v1 插件是受控配置扩展，不执行上传代码。点赞、收藏、@通知可使用同一事件接口实现，但审计、授权、审核和积分账本始终是核心模块。

## 0. 实现状态（M13-PLUGIN）

- **已实现**：`plugins`/`plugin_call_metrics`/`plugin_data` 三库迁移
  （0057_theme.sql）；领域层 `backend/src/plugins/mod.rs`（manifest 解析、
  capability/event/settings-schema 白名单、危险 URL/代码内容扫描、
  policy_revision 乐观锁、调用摘要、plugin_data 配额）；管理路由
  `/api/v1/admin/plugins*`（列表/能力白名单/安装/设置/启停/卸载/指标，
  admin.manage + reason + recent-auth + 审计）。
- **v1 无在线代码执行路径**：`kind` 仅接受 `config`；代码型/WASM 插件是
  v2 研究项（§10），任何 `kind=code/wasm` 的包在安装阶段即被拒绝。
- **capability 白名单**（`KNOWN_CAPABILITIES`，9 项）**不含**权限/审核/账本
  裁决能力；插件永远不能改变裁决结果。
- **受控 Provider Adapter**：Direct/HLS/Xigua 随应用编译
  （`backend/src/video/provider.rs` ProviderRegistry），管理员只可启停/配置
  策略，不能注册新 adapter。
- **安全降级**：插件故障/超时/重复调用/旧版本结果以 `plugin_call_metrics`
  记录（ok/error/timeout/repeat/stale/skipped），fire-and-forget，**绝不
  阻塞核心论坛**；禁用插件不再消费新事件。
- **调用摘要审计**：`record_call` 非阻塞写入；指标脱敏（无 settings 正文/
  Secret）。

## 1. 扩展层级

### 1.1 核心模块

下列能力不可停用、替换或由第三方插件接管：

- 身份、Session、CSRF 和 OIDC。
- RBAC/ABAC 权限裁决。
- 审计日志。
- 审核和处罚状态机。
- 积分事务、账本和内容解锁。
- 数据迁移和备份。

### 1.2 v1 配置型插件

配置型插件由版本化 manifest 和声明式规则组成，可：

- 订阅允许的领域事件。
- 调用白名单动作。
- 存取自己的 `plugin_data` 命名空间。
- 显示已编译进前端的 UI 组件。

不能：

- 执行 JavaScript、Rust、Shell 或 WASM。
- 运行任意 SQL。
- 发起任意网络请求。
- 修改权限裁决结果。
- 读取密码、Session、OAuth Token、私有邮箱或其他插件数据。
- 阻止核心审计写入。

### 1.3 v2 WASM 插件（研究项）

只有在 capability model、签名、资源配额、升级和灾难恢复设计成熟后才实现。WASI 本身不自动等于安全沙箱。

## 2. 插件包

```text
plugin-package/
  plugin.json
  rules/
    post-published.json
  assets/
    icon.svg
```

示例：

```json
{
  "schema_version": 1,
  "id": "welcome-reward",
  "name": "新用户欢迎奖励",
  "version": "1.0.0",
  "supports": ">=1.0 <2.0",
  "kind": "config",
  "subscriptions": ["user.verified"],
  "capabilities": ["points.award", "notification.create"],
  "settings_schema": {
    "type": "object",
    "properties": {
      "amount": { "type": "integer", "minimum": 0, "maximum": 1000 }
    },
    "required": ["amount"],
    "additionalProperties": false
  }
}
```

规则：

- 插件 ID 使用小写 ASCII、数字和连字符。
- 解压时限制文件数、总大小、压缩比和路径。
- manifest 和 settings 必须通过 JSON Schema。
- capabilities 必须是 BBLBB 已知白名单子集。
- 安装不代表启用；启用前显示所需能力并要求管理员确认。

## 3. 领域事件

事件使用版本化 envelope：

```json
{
  "event_id": "uuid-v7",
  "event_type": "post.published.v1",
  "occurred_at": 1760000000000,
  "actor_id": "uuid",
  "aggregate": { "type": "post", "id": "uuid" },
  "payload": {}
}
```

v1 事件建议：

- `user.verified.v1`
- `user.login_succeeded.v1`
- `post.published.v1`
- `post.updated.v1`
- `comment.published.v1`
- `reaction.created.v1`
- `report.created.v1`
- `moderation.action_recorded.v1`
- `points.operation_completed.v1`
- `level.changed.v1`

### before 与 after

- v1 第三方配置插件只接收 **after-event**，不能阻塞核心事务。
- 必须同步校验的规则（权限、敏感词、余额、板块发帖策略）属于核心 domain policy，不做第三方 before-hook。
- 事件与业务数据通过 Outbox 同一事务写入，worker 异步处理插件规则。

## 4. 白名单动作

| 动作 | 限制 |
|---|---|
| `notification.create` | 只能给事件关联用户，模板参数经过 schema 校验 |
| `points.award` | 需要 capability、站点限额和幂等键；仍走核心账本服务 |
| `plugin_data.put` | 只能写自身命名空间，有配额 |
| `plugin_data.delete` | 只能删自身数据 |
| `tag.attach` | 仅允许配置批准的标签和帖子事件 |
| `audit.note` | 追加插件执行说明，不能修改核心审计 |

v1 不开放通用 `http_call`，避免 SSRF、数据外泄和不可控重试。未来若开放，只能通过独立 webhook/egress 子系统和域名白名单实现。大模型调用不属于插件能力，必须经过核心 AI Gateway、脱敏、预算和用户同意策略。

视频能力采用“核心 Video Service + 随应用编译的 Provider Adapter”：Adapter 可在 manifest 声明 `video.resolve`、`video.render`、`video.metadata.refresh`，但这些 capability 只能调用核心 Video Service，不能获得通用网络、数据库或 Secret。`direct/hls/xigua` 适配器随可信发布物安装，管理员只能启停和配置策略；v1 不允许上传新的视频执行代码。西瓜适配器不得抓取或绕过平台鉴权。

## 5. 幂等、重试与失败

- 每次插件动作使用 `(plugin_id, event_id, rule_id, action_index)` 生成幂等键。
- worker 采用指数退避和最大尝试次数。
- 一个插件失败不回滚已提交的帖子或积分核心事务。
- 永久失败进入 dead-letter 状态并在后台显示。
- 禁用插件后不再消费新事件；已锁定任务完成还是取消由 manifest 的停用策略决定，默认取消。
- 插件不得静默吞掉核心错误。

## 6. 数据隔离

插件数据使用 `plugin_data`：

- 每个插件独立命名空间。
- 设置和运行数据分开。
- 支持单插件容量、键数量和值大小配额。
- 插件卸载默认保留数据 30 天，管理员可立即清除。
- 导出和备份包含插件 manifest、settings 和 data。
- 插件不得往 `users`、`posts` 等核心表塞未声明 `extra` JSON。

## 7. 前端 UI 扩展

v1 前端扩展必须在构建时注册：

```ts
const pluginViews = import.meta.glob('/src/plugins/*/ui.ts', { eager: true });
```

- 数据库只控制已注册组件是否显示，不决定任意 import 路径。
- UI 组件声明固定 slot，例如 `post.actions`、`comment.actions`、`admin.navigation`。
- Props 是公开、安全投影，不包含私密邮箱、Token 或隐藏正文。
- 插件 UI 不能绕过 Rust API 权限。
- 安装新的 Svelte UI 扩展需要重新构建部署。

建议 v1 slot：

- `header.navigation.after`
- `sidebar.after`
- `post.card.actions`
- `post.detail.actions`
- `comment.actions`
- `compose.after`
- `user.profile.tabs`
- `admin.navigation.after`

## 8. 生命周期

```text
上传配置包
→ 解压与 schema 校验
→ 兼容性/能力检查
→ 安装（disabled）
→ 管理员配置
→ 启用
→ 异步执行
→ 停用
→ 升级或卸载
```

升级要求：

- manifest 使用语义版本。
- 配置 schema 变化必须有迁移函数；v1 仅支持内置迁移器声明的 JSON 转换。
- 升级失败保留旧版本配置和启用状态。
- 不允许降级到不能读取当前配置版本的插件。

## 9. 内置扩展

以下可作为核心功能之上的内置扩展，但使用正式领域表，而不是只依赖 `plugin_data`：

- 点赞/反应。
- 收藏。
- @提及通知。
- 欢迎奖励规则。

举报和审计不属于可停用插件。

## 10. WASM 研究门槛

在 v2 开工前必须先写设计并验证：

- 插件签名和可信发布者。
- capability-based host API，不开放原始数据库连接。
- fuel、epoch interruption、内存和输出限制。
- 无默认文件系统、环境变量和网络访问。
- 异步宿主调用和取消。
- 每插件并发和调用速率。
- 数据迁移、兼容和崩溃恢复。
- 供应链撤销和紧急 kill switch。

未满足上述条件时，不承诺 WASM 插件功能。
