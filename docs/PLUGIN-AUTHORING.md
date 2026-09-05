# BBLBB — 插件编写指南与范例

> 版本：v0.1（对应 [`PLUGIN.md`](PLUGIN.md) v0.4；M13-PLUGIN 管理面已实现）
> 读者：为本站编写 v1 配置型插件的管理员与开发者。
> 事实来源：规范与安全边界以 [`PLUGIN.md`](PLUGIN.md) 为准，本文只做「怎么写」的教学展开；两者冲突时以 PLUGIN.md 为准。
> 实现状态速览：管理面（安装 / 设置 / 启停 / 卸载 / 调用摘要 / 审计）已上线；事件→动作的异步分发**执行面尚未接线**（§0.3）。本文描述的 manifest 契约在执行面落地后无需改动——今天就可以按本文编写、安装并配置插件。

## 0. 开始之前

### 0.1 v1 插件是什么

一段**受控的声明式 JSON 配置**（manifest），不是代码：

| 插件可以 | 插件不可以 |
|---|---|
| 订阅白名单内的领域事件（after-event，异步） | 执行 JavaScript、Rust、Shell 或 WASM（`kind` 只接受 `config`） |
| 声明白名单内的 capability（§2.3） | 发起任意网络请求（v1 无 `http_call`，防 SSRF / 数据外泄） |
| 拥有自己的 `settings`（封闭 schema 校验） | 运行任意 SQL、读写核心表 |
| 存取自己的 `plugin_data` 命名空间（配额内） | 读取密码、Session、OAuth Token、私有邮箱或其他插件的数据 |
| 通过管理 API 安装 / 启停 / 卸载（全程审计） | 修改权限、审核、积分账本的裁决结果 |
| | 阻塞核心论坛事务（插件故障只降级，不回滚帖子/积分） |

### 0.2 安装载体

v1 的安装载体是**单个 JSON 文档**（`POST /api/v1/admin/plugins` 的请求体，上限 256KB），
不是目录压缩包。[`PLUGIN.md`](PLUGIN.md) §2 的 `plugin.json + rules/ + assets/` 目录包
格式（含解压文件数 / 压缩比 / 路径检查）是规划中的能力，见 §7——**今天请不要依赖它**。

### 0.3 今天能用 / 暂不可用

| 能力 | 状态 | 说明 |
|---|---|---|
| 安装、设置、启停、卸载（`If-Match` 乐观锁 + reason + 审计） | ✅ 已实现 | `/api/v1/admin/plugins*`，见 §5 |
| manifest 校验（capability / 事件白名单、危险内容扫描、兼容范围、封闭 settings schema） | ✅ 已实现 | 安装与保存设置时逐项拒绝，见 §2 / §3 |
| 调用摘要（`plugin_call_metrics`，result 六标签） | ✅ 已实现 | 指标表与写入已具备，见 §6 |
| 事件 → 规则 → 白名单动作的异步执行 | ⏳ 执行面待接线 | 管理面数据已就绪；分发 worker 为规划项（§7.1） |
| `plugin_data` 读写 | ⏳ 待接线 | 领域函数（`put_plugin_data`，64 键 / 8KB 值配额）已实现并有测试，但尚无执行路径调用它 |
| `rules/` 目录包、版本升级迁移、卸载保留 30 天 | ⏳ 规划中 | §7 |

## 1. 五分钟：最小插件

下面是一个**合法的最小 manifest**（`capabilities` 与 `subscriptions` 可以为空数组，
`settings_schema` 必须存在且为封闭对象）：

```json
{
  "schema_version": 1,
  "kind": "config",
  "id": "hello-plugin",
  "name": "最小示例插件",
  "version": "1.0.0",
  "supports": ">=1.0 <2.0",
  "subscriptions": [],
  "capabilities": [],
  "settings_schema": {
    "type": "object",
    "additionalProperties": false
  }
}
```

### 安装路径 A：管理后台（无 JS 也可用）

打开 `/admin/plugins`，在安装表单里填：

| 表单字段 | 值 |
|---|---|
| 插件 ID | `hello-plugin` |
| 名称 | `最小示例插件` |
| capabilities | `[]` |
| subscriptions | `[]` |
| settings_schema | 上面 `settings_schema` 的 JSON 原文 |
| 操作原因（reason） | 必填，写入审计日志 |

表单会自动补上 `schema_version: 1`、`kind: "config"`、`version: "1.0.0"`、
`supports: ">=1.0 <2.0"`。安装成功后插件处于 **disabled（隔离态）**——安装不代表启用。

### 安装路径 B：API

```sh
# 0) 前置：管理员会话 Cookie + CSRF token（获取方式见 docs/API.md / AUTH-OIDC.md）
CSRF=$(curl -s -H "Cookie: __Host-bblbb_session=$SESSION" \
  http://127.0.0.1:8080/api/v1/auth/csrf | ruby -rjson -e 'puts JSON.parse(STDIN.read)["token"]')

# 1) 安装（请求体 = manifest + reason）
curl -X POST http://127.0.0.1:8080/api/v1/admin/plugins \
  -H "Cookie: __Host-bblbb_session=$SESSION" \
  -H "x-csrf-token: $CSRF" \
  -H "Content-Type: application/json" \
  -d '{
    "schema_version": 1,
    "kind": "config",
    "id": "hello-plugin",
    "name": "最小示例插件",
    "version": "1.0.0",
    "supports": ">=1.0 <2.0",
    "subscriptions": [],
    "capabilities": [],
    "settings_schema": { "type": "object", "additionalProperties": false },
    "reason": "插件编写指南最小示例"
  }'
```

返回 `201`，响应体含完整插件投影（`status: "disabled"`、`policy_revision: 1`）。
安装要求：`admin.manage` 权限、会话在 step-up 窗口内（窗口外需重新验证）、必填 `reason`。

## 2. Manifest 字段权威参考

### 2.1 字段表

| 字段 | 必填 | 约束 | 违反时 |
|---|---|---|---|
| `schema_version` | 是 | 必须为整数 `1` | `plugin_incompatible`（400） |
| `kind` | 是 | 只接受 `"config"`；`code` / `wasm` 在安装阶段即被拒绝（v2 研究项） | `plugin_invalid`（400） |
| `id` | 是 | 小写 ASCII 字母 / 数字 / 连字符，长度 1..=64；站内唯一 | `plugin_invalid`；重复安装 `plugin_conflict`（409） |
| `name` | 是 | 1..=120 字符 | `plugin_invalid` |
| `version` | 是 | semver：`MAJOR.MINOR` 或 `MAJOR.MINOR.PATCH`，纯数字 | `plugin_invalid` |
| `supports` | 是 | 版本范围，≤64 字符，必须接受核心版本 `1.0`（语法见 §2.2） | `plugin_incompatible` |
| `capabilities` | 是 | 字符串数组（可为空），必须是 §2.3 白名单子集 | `plugin_invalid` |
| `subscriptions` | 否 | 字符串数组（默认空），必须是 §2.4 白名单子集 | `plugin_invalid` |
| `settings_schema` | 是 | 封闭 JSON Schema 子集，规则见 §3 | `plugin_invalid` |
| `reason` | 路由层必填 | 管理操作原因，写审计日志；不是 manifest 语义的一部分 | 400 |

注意：

- **未知顶层键会被静默忽略**。在请求体里写 `rules`、`assets` 之类的键今天不会有任何效果
  （也不会报错）——执行面接线后才会赋予语义（§7.1）。
- 整个请求体序列化后不得超过 **256KB**。

### 2.2 `supports` 语法

支持 `>=`、`>`、`<=`、`<`、`==` 五种运算符，多个约束用空白分隔（AND 语义）。
当前核心版本固定按 `1.0` 判定，因此 `"supports": ">=1.0 <2.0"` 是标准写法。

### 2.3 capability 白名单（9 项）

| capability | 语义 | 限制（PLUGIN.md §4） |
|---|---|---|
| `notification.create` | 给事件关联用户创建通知 | 模板参数经 schema 校验 |
| `points.award` | 经核心账本发放积分 | 需 capability、站点限额与幂等键；仍走核心账本服务 |
| `plugin_data.put` | 写自身 `plugin_data` 命名空间 | 每插件 64 键、每值 8KB |
| `plugin_data.delete` | 删自身数据 | 同上 |
| `tag.attach` | 给事件帖子附加标签 | 仅限配置批准的标签 |
| `audit.note` | 追加插件执行说明 | 不能修改核心审计 |
| `video.resolve` / `video.render` / `video.metadata.refresh` | 调用核心 Video Service | 只能走随应用编译的 Provider Adapter（Direct/HLS/Xigua），不能获得通用网络/DB/Secret |

白名单**不含**任何权限 / 审核 / 账本裁决能力——插件永远不能改变裁决结果。
视频 Provider 的策略（启停、适配器选择）由管理员在 `/admin/video` 配置，
manifest 声明 `video.*` 只表示「允许调用核心 Video Service」。

### 2.4 订阅白名单（10 项）与事件源状态

| 事件 | 当前事件源状态 |
|---|---|
| `reaction.created.v1` | ✅ 已在生产发射（互动创建） |
| `points.operation_completed.v1` | ✅ 已在生产发射（账务提交） |
| `post.published.v1` | ⏳ 常量已注册，发帖流程的事件源待接线 |
| `user.verified.v1` / `user.login_succeeded.v1` / `post.updated.v1` / `comment.published.v1` / `report.created.v1` / `moderation.action_recorded.v1` / `level.changed.v1` | ⏳ 事件源待接入 |

> 事件目录的完整事实来源是 [`EVENT-CATALOG.md`](EVENT-CATALOG.md)（23 项注册事件、
> envelope 格式与 payload 最小投影规则）。插件白名单与领域事件注册表的对齐
> 属规划中的执行面工作；今天订阅 ⏳ 事件不会被校验拒绝，但在事件源接入前不会
> 触发任何调用。**要让插件在执行面落地后立即有真实行为，优先订阅 ✅ 两项。**

### 2.5 危险内容扫描（settings 字符串值）

保存设置时，**每个字符串值**都会扫描以下模式，命中即拒绝（`plugin_invalid`）：

`<script`、`<?php`、`<?xml`、`javascript:`、`vbscript:`、`data:text/html`、`eval(`、
`Function(`、`system(`、`exec(`、`shell_exec`、`require(`、`include(`、`import `、
`http://`、`https://`、`//`、`file:`、`ftp:`、`\\`、`{`、`}`、`;`

实用结论：**任何字符串设置里都不要出现 URL、花括号、分号**。需要引用外部资源时，
应通过核心服务（如 Video Provider 的站点配置）完成，而不是塞进插件设置。

## 3. `settings_schema` 支持的子集

v1 实现的是 JSON Schema 的**封闭子集**，超出即拒绝：

- 顶层只允许 4 个键：`type`、`properties`、`required`、`additionalProperties`；
  `type` 必须为 `"object"`；`additionalProperties` **必须写且必须为 `false`**
  （v1 封闭 settings，保存时逐键校验）；
- `properties` 里每个属性 schema 只允许 7 个键：
  `type`（必填，`string` / `integer` / `number` / `boolean`）、
  `minimum` / `maximum`（数值范围）、`minLength` / `maxLength`（字符串，按字节）、
  `enum`（非空、原始值数组）、`default`（仅信息性，不会自动填充）。

### 校验时机（重要）

| 时机 | 校验内容 |
|---|---|
| 安装（POST） | `settings_schema` 本身的结构（上面的封闭子集） |
| 保存设置（PATCH settings） | settings 值逐键对照 schema + 字符串值危险内容扫描；缺 `required` 键拒绝 |
| 启用（POST enable） | **不校验 settings**（只做状态迁移 + 乐观锁） |

因此推荐顺序是 **安装 → 配置 settings → 启用**。启用后仍可改设置
（`If-Match` 换新 revision 即可），但空配置 + required 键缺失的插件在执行面
落地后调用时会按失败降级记录（`plugin_call_metrics.result = error`），不要依赖它。

## 4. 完整范例

### 4.1 `reaction-thanks` — 互动感谢（`notification.create`）

```json
{
  "schema_version": 1,
  "kind": "config",
  "id": "reaction-thanks",
  "name": "互动感谢",
  "version": "1.0.0",
  "supports": ">=1.0 <2.0",
  "subscriptions": ["reaction.created.v1"],
  "capabilities": ["notification.create"],
  "settings_schema": {
    "type": "object",
    "properties": {
      "message": { "type": "string", "minLength": 1, "maxLength": 200 }
    },
    "additionalProperties": false
  }
}
```

对应的一份合法 settings（PATCH 时提交）：

```json
{ "message": "有人赞了你的帖子，去看看吧" }
```

语义：订阅 `reaction.created.v1`（✅ 已发射），事件关联用户收到一条通知。
`message` 是纯文本——注意 §2.5，不能包含 URL 或分号。

### 4.2 `reaction-points` — 互动积分奖励（`points.award` + 数值/布尔设置）

```json
{
  "schema_version": 1,
  "kind": "config",
  "id": "reaction-points",
  "name": "互动积分奖励",
  "version": "1.2.0",
  "supports": ">=1.0 <2.0",
  "subscriptions": ["reaction.created.v1"],
  "capabilities": ["points.award", "notification.create"],
  "settings_schema": {
    "type": "object",
    "properties": {
      "amount": { "type": "integer", "minimum": 1, "maximum": 50 },
      "daily_cap": { "type": "integer", "minimum": 1, "maximum": 500, "default": 10 },
      "notify_user": { "type": "boolean", "default": true }
    },
    "required": ["amount"],
    "additionalProperties": false
  }
}
```

settings 示例：

```json
{ "amount": 5, "daily_cap": 20, "notify_user": true }
```

语义：收到互动事件后给事件关联用户发放积分。积分仍走**核心账本服务**
（幂等键 + 站点限额，PLUGIN.md §4），插件设置里的 `amount` / `daily_cap` 是插件
自己的策略参数，不改变账本裁决。

### 4.3 `points-digest` — 多订阅 + 枚举设置（`audit.note`）

```json
{
  "schema_version": 1,
  "kind": "config",
  "id": "points-digest",
  "name": "积分动态摘要",
  "version": "0.3.0",
  "supports": ">=1.0 <2.0",
  "subscriptions": ["points.operation_completed.v1", "reaction.created.v1"],
  "capabilities": ["notification.create", "audit.note"],
  "settings_schema": {
    "type": "object",
    "properties": {
      "verbosity": { "type": "string", "enum": ["quiet", "normal", "verbose"] },
      "min_delta": { "type": "number", "minimum": 0 }
    },
    "required": ["verbosity"],
    "additionalProperties": false
  }
}
```

settings 示例：

```json
{ "verbosity": "normal", "min_delta": 10 }
```

语义：同时订阅两个 ✅ 已发射事件；`verbosity` 用封闭枚举控制输出级别；
`audit.note` 只能**追加**插件执行说明，不能修改核心审计。

## 5. 安装、配置与启用（API 全流程）

前置：管理员账号（`admin.manage` 权限）、会话 Cookie `__Host-bblbb_session`、
CSRF token（`GET /api/v1/auth/csrf` 返回 `{ "token": ... }`，写请求放
`x-csrf-token` 头）、会话在 step-up 窗口内。

```sh
# 1) 安装（见 §1 路径 B）→ status=disabled, policy_revision=1

# 2) 配置 settings（If-Match 携带当前 policy_revision；成功后 revision+1）
curl -X PATCH http://127.0.0.1:8080/api/v1/admin/plugins/reaction-thanks/settings \
  -H "Cookie: __Host-bblbb_session=$SESSION" -H "x-csrf-token: $CSRF" \
  -H "If-Match: 1" -H "Content-Type: application/json" \
  -d '{ "settings": { "message": "有人赞了你的帖子，去看看吧" }, "reason": "初始化插件设置" }'

# 3) 启用（If-Match 换成最新 revision）
curl -X POST http://127.0.0.1:8080/api/v1/admin/plugins/reaction-thanks/enable \
  -H "Cookie: __Host-bblbb_session=$SESSION" -H "x-csrf-token: $CSRF" \
  -H "If-Match: 2" -H "Content-Type: application/json" \
  -d '{ "reason": "指南示例启用" }'

# 4) 查看调用摘要
curl -H "Cookie: __Host-bblbb_session=$SESSION" \
  "http://127.0.0.1:8080/api/v1/admin/plugins/reaction-thanks/metrics?limit=50"

# 5) 停用 / 卸载（卸载前必须先停用）
curl -X POST http://127.0.0.1:8080/api/v1/admin/plugins/reaction-thanks/disable \
  -H "Cookie: __Host-bblbb_session=$SESSION" -H "x-csrf-token: $CSRF" \
  -H "If-Match: 3" -H "Content-Type: application/json" -d '{ "reason": "下线示例" }'
curl -X DELETE http://127.0.0.1:8080/api/v1/admin/plugins/reaction-thanks \
  -H "Cookie: __Host-bblbb_session=$SESSION" -H "x-csrf-token: $CSRF" \
  -H "Content-Type: application/json" -d '{ "reason": "清理示例" }'
```

规则速查：

- **乐观锁**：settings / enable / disable 都要求 `If-Match: <当前 policy_revision>`，
  revision 不匹配返回 `409 plugin_conflict`（并附当前值，重读列表后重试）；
  每次成功变更 revision +1。
- **状态机**：`disabled → enabled → disabled`；表结构还保留了 `error` 态
  （执行面规划使用），当前管理操作只接受 `enabled` / `disabled`。
- **卸载**：要求先 `disabled`；v1 **立即删除**配置与 `plugin_data`
  （规范中的「默认保留 30 天」是规划项，见 §7）。卸载后重装是全新插件（revision 从 1 重新开始）。
- **升级**：重复安装同 ID 返回 `409 plugin_conflict`；版本升级 / settings schema
  迁移未实现，今天的做法是「导出设置 → 卸载 → 重装 → 重新配置」。
- 审计：install / settings / enable / disable / uninstall 全部写审计日志（action +
  reason + 操作者）。

## 6. 验证与调试

### 6.1 调用摘要（`plugin_call_metrics`）

`GET /api/v1/admin/plugins/{id}/metrics?limit=N`（默认 100，钳制 1..500）：

| `result` | 含义 |
|---|---|
| `ok` | 动作成功 |
| `error` | 执行失败（如缺 required 设置） |
| `timeout` | 超过时限被取消 |
| `repeat` | 幂等键命中，跳过重复执行 |
| `stale` | 结果属于旧 policy revision，被丢弃 |
| `skipped` | 插件停用等原因未消费 |

指标是 fire-and-forget 追加，**绝不阻塞核心事务**；不含 settings 正文与 Secret。

### 6.2 常见错误码

| code | HTTP | 场景 |
|---|---|---|
| `plugin_invalid` | 400 | manifest / settings / 危险内容 / 未知 capability 或事件 / 状态值非法 |
| `plugin_incompatible` | 400 | `schema_version` 不支持、`supports` 不接受核心版本 |
| `plugin_conflict` | 409 | 重复安装、`If-Match` revision 不匹配 |
| `plugin_not_found` | 404 | ID 不存在 |

### 6.3 参考实现与测试

- 后端领域模块：`backend/src/plugins/mod.rs`（校验规则的事实来源）；
- 集成测试：`backend/tests/plugins.rs`（生命周期 / 越权 / 白名单 / 乐观锁 / 指标）、
  `backend/tests/admin_routes.rs`（HTTP 级行为）；
- 能力与服务接口的机器可读投影：`GET /api/v1/admin/plugins/capabilities`。

## 7. 规划中的编写能力（不要依赖）

以下内容是 [`PLUGIN.md`](PLUGIN.md) 已定义、**尚未实现**的能力。写进 manifest
今天不会有任何效果（未知键被忽略），但契约如下，执行面落地后即生效：

### 7.1 `rules/` 声明式规则

目录包形态（PLUGIN.md §2）：

```text
plugin-package/
  plugin.json        # 即本文的 manifest
  rules/
    post-published.json
  assets/
    icon.svg
```

规则文件示例（`rules/post-published.json`，声明式 when + actions）：

```json
{
  "rule_id": "welcome-points",
  "on": "post.published.v1",
  "when": { "post.author.posts_count": { "equals": 1 } },
  "actions": [
    { "action": "points.award", "settings_key": "amount" },
    { "action": "notification.create", "template": "first-post" }
  ]
}
```

配套语义（PLUGIN.md §3 / §5）：插件只收 **after-event**；每次动作用
`(plugin_id, event_id, rule_id, action_index)` 生成幂等键；worker 指数退避 +
最大尝试次数，永久失败进 dead-letter；一个插件失败不回滚核心事务。

### 7.2 其他规划项

- `plugin_data` 读写接入执行路径（`put_plugin_data` 已实现：每插件 64 键、
  每值 8KB、键 1..=128 且拒绝 `__` 前缀；`plugin_data.get` / `delete` 待实现）；
- 版本升级与 settings schema 迁移函数；卸载默认保留 30 天；
- 事件白名单与领域事件注册表（[`EVENT-CATALOG.md`](EVENT-CATALOG.md)）的统一对齐。

## 8. 安全边界速查

编写或评审插件时逐条对照（完整版见 [`PLUGIN.md`](PLUGIN.md) §1 / §4）：

1. 插件是配置数据，不是代码——`kind` 只接受 `config`，`code` / `wasm` 安装即拒。
2. 没有通用网络能力：不提供 `http_call`，避免 SSRF、数据外泄与不可控重试。
3. 权限、审核、积分账本裁决永远是核心模块，插件不能改变裁决结果。
4. 插件数据隔离：只读写自己的 `plugin_data` 命名空间与 settings。
5. 插件失败只降级（metrics 记录），绝不阻塞核心论坛。
6. 管理操作全程审计；安装、启用、设置变更都要求 reason 与乐观锁。
