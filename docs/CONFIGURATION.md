# BBLBB — 配置字段与运行时变更矩阵

> 基线：v0.4。环境变量/Secret Store 是部署事实来源；后台可编辑项必须映射到版本化数据库 policy。配置值不能改变代码安全上限。

| 配置域 | 关键字段 | Secret | 运行时变更 | 生效范围 |
|---|---|---:|---|---|
| Server | `PUBLIC_ORIGIN`、bind、trusted proxies | 否 | 重启 | 新请求/Session |
| Database | URL、pool、busy timeout | 可能 | 重启 | 新连接 |
| Session/OIDC | cookie、issuer、key ref、TTL | 是 | 密钥轮换流程 | 新 Token；旧 Token按撤销策略 |
| Storage | backend、endpoint、bucket、region、upload limit、signed URL TTL | 是 | 候选配置 + 测试 + 审批 | 新上传/新 URL |
| Attachment quota | level max file/total bytes | 否 | 是，需 version | 新上传；已有对象不删除 |
| Download billing | mode、prices、free rules、limits、TTL | 否 | 是，需 reason/version | 新授权；历史授权不变 |
| Marketplace | Client、scope、limit、fee、settlement delay、webhook | Secret | 审批/轮换流程 | 新 Token/Offer/Intent |
| AI | Provider URL、models、data mode、budget、timeout | Secret | 是，策略递增 version | 新任务；历史 suggestion 保留来源版本 |
| Video | Provider enable、hosts、CSP、HLS budgets、duration | 否 | 是，policy version | 新 resolve/render；历史引用重检 |
| Rate limit | user/IP/object/provider buckets | 否 | 是 | 新请求 |
| Mail | SMTP host、sender、templates | 是 | Secret 轮换/模板发布 | 新任务 |
| Feature flags | capability 默认开关、灰度规则 | 否 | 是，需审计 | 新请求/用户投影 |

## 1. 统一规则

- 配置读取返回 `configured`、来源类别、版本和更新时间，不返回 Secret、完整签名 URL、内部路径或 Provider 原始响应。
- 在线修改必须先校验 schema、范围、依赖和安全上限，再写 `config_revisions`/policy 版本并审计；高风险修改需要近期认证。
- 外部 Provider 未配置或不可用时，不得阻塞核心发帖、阅读、登录和已提交账务。
- 关闭功能停止新任务/新授权/新交易；不删除历史数据、不撤销已经提交账务、不让已发布内容突然泄漏。
- Feature Flag 只负责启停和灰度，不得绕过权限、CSRF、审计、账本或安全策略。
