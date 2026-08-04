# BBLBB — 统一术语表

> 基线：v0.4。所有 API、Schema、前端和测试使用以下术语；新文档不得为同一概念引入别名。

| 统一术语 | 不再作为协议术语使用 | 定义 |
|---|---|---|
| `post` | topic（仅可作为用户界面文案） | 可发布的内容根对象；`post_type` 区分 `article` 和 `discussion` |
| `comment` | reply（仅可作为 UI 文案） | 帖子下的回复对象 |
| `published` | visible | 已满足发布条件；请求方是否可见仍由授权判定 |
| `closed` | locked（除播放器/附件状态外） | 禁止新增回复；不等于内容删除或账号封禁 |
| `hidden` | deleted | 公开投影不可见，但对象和审计仍存在 |
| `sanction` | punishment（叙述可用） | 对账号或板块施加的正式限制记录 |
| `currency` | point、coin、额度混用 | 可计量账户单位；内置代码为 `exp`、`coin`、`contribution` |
| `coin` | B币（仅 UI 文案） | 默认可消费货币，整数最小单位 |
| `authorization` | grant、unlock 混用 | 允许一次或一段时间执行某动作的后端记录 |
| `download_authorization` | download grant | 附件下载授权；不等于 S3 URL |
| `checkout_intent` | order intent | 一次性、短效、绑定用户和 Client 的报价快照 |
| `purchase` | transaction（泛称） | 已提交的市场购买业务记录；账务记录另称 `point_operation` |
| `refund` | reversal（泛称） | 市场退款业务记录；账务冲正另称 `reversal operation` |
| `provider` | adapter/plugin 混用 | 外部服务或媒体来源的逻辑配置 |
| `adapter` | provider（实现语境） | 核心服务中实现某 Provider 协议的代码组件 |
| `plugin` | 任意外部代码 | 受 capability 限制的扩展；视频 Provider 不能获得通用网络权限 |
| `video_embed` | attachment | 第三方媒体引用；不拥有或保存第三方媒体对象 |
| `task` | job 混用 | 面向用户/管理员可查询的业务异步任务 |
| `job` | task（实现语境） | Worker 执行单元；可由 Outbox 触发 |
| `policy` | config/setting 混用 | 影响业务判定且需要版本化、审计的规则 |
| `setting` | policy（非规则配置） | 不直接参与业务授权的展示或运行配置 |

## 状态术语

- `closed` 只表示禁止回复；`locked_at` 是时间属性，不是第二套状态。
- `deleted` 表示软删除；`hidden` 表示暂时不进入公开投影；二者不可互换。
- “临时 URL 过期”只表示 URL 失效，不表示附件对象、下载授权或账本记录删除。
- “实时购买”只表示同步响应反映已提交数据库事实；Webhook 仍然异步最终送达。
- “可见”必须写成“当前请求方可见”，不能把发布状态当作授权结果。
