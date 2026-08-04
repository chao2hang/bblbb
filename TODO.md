# BBLBB v1.0 正式上线长期开发 TODO

> 路线图版本：v1.0.0-rc.1
> 依据：已冻结的 [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md)、[`docs/PRODUCT-DECISIONS.md`](docs/PRODUCT-DECISIONS.md)、[`docs/DOCUMENT-STATUS.md`](docs/DOCUMENT-STATUS.md) 和 [`openapi/openapi.yaml`](openapi/openapi.yaml)。
> 目标：从当前 OpenAPI、Rust、SvelteKit、迁移和 CI 骨架，逐步达到 v1.0 正式上线标准。
> 当前状态：长期路线图已建立；业务实现尚未开始。

## 使用规则

- `[ ]` 未开始；`[~]` 进行中；`[x]` 已完成；`[!]` 阻塞或需要重新决策。
- `P0` 是上线阻断项；`P1` 是 v1.0 必须完成的能力；`P2` 是 v1.0 可选能力的专项门槛工作。
- 本文件是长期路线图，不替代当前对话的短期待办清单。
- 每个任务必须在完成时补充实现文件、测试证据和实际命令；不能只勾选代码存在。
- 可选能力即使属于 v1.0，也必须先通过专项门槛再打开 Feature Flag。
- 不得修改已经执行过的迁移；数据库结构变更必须新增不可变迁移。
- 不得提交真实 Secret、用户数据、生产 URL、备份文件或构建产物。

## 总体上线定义

v1.0 只有在以下条件全部满足后才可以发布：

- [ ] P0 所有安全、数据一致性、权限、恢复和隐私阻断项通过。
- [ ] SQLite、MySQL 8、MariaDB 10.11 均通过迁移、契约、集成和关键并发测试。
- [ ] 核心论坛在 AI、Video Provider、Download Billing、OIDC、Marketplace 未配置时仍可独立运行。
- [ ] OpenAPI、权限矩阵、错误码、状态机、事件目录、Schema 和实现无未经批准差异。
- [ ] 默认主题通过 Playwright、axe/WCAG 2.2 AA 和主要流程验收。
- [ ] SQLite 备份恢复、附件恢复和 OIDC 密钥恢复演练成功。
- [ ] 部署、回滚、迁移、故障、封禁、数据导出和注销流程有可执行 Runbook。
- [ ] 实际部署地区、运营主体、隐私政策、用户条款、内容处理和法律责任完成评审。
- [ ] RC 环境完成冒烟、性能、压力、安全、隐私、可访问性和人工验收。
- [ ] 发布前无 P0/P1 未关闭问题；P2 若默认关闭，必须有明确的关闭理由和恢复计划。

---

# M0：工程契约和可运行骨架

**目标：** 把当前骨架变成所有后续开发共同依赖的工程基线。

## M0.1 仓库与开发工具

- [ ] `M0.1.1` 确认 Rust、Node、npm、SQLite、MySQL 和 MariaDB 的 CI 版本矩阵。
- [ ] `M0.1.2` 统一开发、测试、预发布和生产环境变量命名；与 [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) 逐项核对。
- [ ] `M0.1.3` 增加根目录开发命令入口，避免必须记忆 `cd backend` 和 `cd frontend`。
- [ ] `M0.1.4` 增加 pre-commit/CI 检查：Rust fmt、Clippy、Svelte check、OpenAPI、SQL、Markdown 链接和 diff check。
- [ ] `M0.1.5` 规定 `target/`、`.svelte-kit/`、`node_modules/`、数据库、日志和生成文件不进入 Git。
- [ ] `M0.1.6` 增加本地开发说明：启动后端、启动前端、启动 SQLite、重置开发数据库和运行全部检查。

**验收：** 新开发者按 README 在干净环境中完成安装、启动和测试；CI 不依赖本机残留文件。

## M0.2 后端应用边界

- [ ] `M0.2.1` 将 axum 路由分为 `auth/users/content/moderation/storage/economy/ai/video/oidc/marketplace/admin` 模块。
- [ ] `M0.2.2` 增加统一 AppState：配置、数据库池、存储适配器、任务队列、审计和 Feature Flag。
- [ ] `M0.2.3` 完成 `X-Request-ID` 从入口、日志、Problem、审计到 Outbox 的贯通。
- [ ] `M0.2.4` 将 Problem JSON 补齐 `type/title/status/code/detail/instance/request_id/errors`，禁止输出 SQL、栈、Secret 和隐藏正文。
- [ ] `M0.2.5` 增加 `/healthz` 和受保护 `/readyz`；健康检查不依赖外部 Provider，ready 检查迁移、数据库和关键目录。
- [ ] `M0.2.6` 将 `/api/v1/openapi.json` 与 `openapi/openapi.yaml` 的发布方式固定，避免运行时返回与契约不同的文档。
- [ ] `M0.2.7` 增加全局请求体大小、超时、并发和 Content-Type 限制。

**依赖：** 无。  
**阻断：** M0.2 未完成不得开始高风险业务实现。

## M0.3 前端应用边界

- [ ] `M0.3.1` 建立 SvelteKit SSR layout、同源 API client、请求 ID 透传和 Session 读取边界。
- [ ] `M0.3.2` Rust 是认证、授权、可见性和账务唯一裁决方；SvelteKit 不自行授予权限。
- [ ] `M0.3.3` 建立表单错误、Problem code、`message_key` 和字段错误映射。
- [ ] `M0.3.4` 建立加载、空状态、错误、429、503、无权限和审核中状态组件。
- [ ] `M0.3.5` 配置 SSR/浏览器请求的缓存边界，禁止隐藏内容、Session 和管理响应进入公共缓存。
- [ ] `M0.3.6` 建立键盘导航、焦点、减少动效、屏幕阅读器和移动触屏基础规范。

## M0.4 OpenAPI 契约治理

- [ ] `M0.4.1` 为 172 个操作逐项核对 `operationId/tags/security/x-permission/x-csrf/responses`。
- [ ] `M0.4.2` 将所有稳定错误码与 [`docs/ERROR-CODES.md`](docs/ERROR-CODES.md) 建立自动一致性检查。
- [ ] `M0.4.3` 将状态机枚举与 OpenAPI schema、Rust enum、前端类型和 Fixture 建立检查。
- [ ] `M0.4.4` 为每个写接口标注 CSRF、幂等、If-Match、Cache-Control 和审计要求。
- [ ] `M0.4.5` 生成 TypeScript 类型或 API client；生成文件必须由契约生成，不手工分叉。
- [ ] `M0.4.6` 为 v1 API 建立兼容性规则：只允许新增兼容字段，删除/改语义进入 v2。

---

# M1：数据库、配置、任务和审计基础

**目标：** 先建立所有领域模块必须使用的安全基础设施。

## M1.1 数据库连接和迁移执行器

- [ ] `M1.1.1` 接入 sqlx，支持 SQLite、MySQL 8、MariaDB 10.11 的池配置。
- [ ] `M1.1.2` 每次连接设置 SQLite WAL、foreign keys、busy timeout 和时区约定。
- [ ] `M1.1.3` 实现迁移版本、checksum、应用记录、失败停止和显式迁移命令。
- [ ] `M1.1.4` 禁止生产启动自动执行未知迁移；ready 在迁移不匹配时失败。
- [ ] `M1.1.5` 为每个 SQL 迁移编写三数据库兼容 Fixture。
- [ ] `M1.1.6` 用 `BIGINT` Unix 毫秒、UUID v7 和数据库差异适配层统一时间与 ID。
- [ ] `M1.1.7` 测试 SQLite `BEGIN IMMEDIATE` 与 MySQL/MariaDB 行锁的关键路径等价性。

## M1.2 配置与 Feature Flag

- [ ] `M1.2.1` 实现环境变量、Secret ref、数据库 policy 和默认值加载。
- [ ] `M1.2.2` 未知配置键在生产模式报错或明确告警。
- [ ] `M1.2.3` Secret 只写不读；GET 只返回 `secret_configured`、来源类别、版本和更新时间。
- [ ] `M1.2.4` 实现 Feature Flag：默认值、灰度范围、紧急关闭、审计和生效时间。
- [ ] `M1.2.5` 验证 Feature Flag 不能绕过权限、CSRF、审计、账本或安全上限。
- [ ] `M1.2.6` 为 AI、Video Provider、Download Billing、OIDC、Marketplace 分别建立默认关闭配置。

## M1.3 审计、Outbox 和任务队列

- [ ] `M1.3.1` 建立不可关闭的 `audit_logs`，记录 actor、target、reason、request_id、before/after 和策略版本。
- [ ] `M1.3.2` 建立 Transactional Outbox，业务事务提交时同时写事件。
- [ ] `M1.3.3` 建立数据库任务表：queued/running/retry_wait/succeeded/cancelled/dead。
- [ ] `M1.3.4` 实现 worker 抢占、租约、指数退避、最大重试、dead 处理和人工重放。
- [ ] `M1.3.5` Outbox 投递至少一次；事件消费者必须按 `event_id` 幂等。
- [ ] `M1.3.6` 任务和 Outbox 不得在数据库事务中调用网络、图片处理或 Provider。
- [ ] `M1.3.7` 增加堆积、失败、重试和 dead 指标及管理查询。

---

# M2：账号、邮箱验证、Session 和安全

**目标：** 完成第一条可安全使用的身份链路。

## M2.1 注册和邮箱验证

- [ ] `M2.1.1` 实现用户名/邮箱规范化、唯一性和注册限流。
- [ ] `M2.1.2` 使用 Argon2id PHC hash；禁止明文密码、弱 hash 和日志泄漏。
- [ ] `M2.1.3` 创建 `pending_verification` 用户和一次性验证 Token；数据库只保存 Token hash。
- [ ] `M2.1.4` 注册同事务写邮箱 Outbox 事件。
- [ ] `M2.1.5` 实现验证 Token 过期、消费、重发限流和旧 Token 失效。
- [ ] `M2.1.6` 未验证用户只能登录浏览、修改账号和重发验证；禁止发帖、回复、上传、交易和奖励。
- [ ] `M2.1.7` 验证后默认立即可用；实现后台可配置的新用户冷静期和受限动作。
- [ ] `M2.1.8` 注册和重发响应不得泄漏邮箱是否已存在。

**验收：** 未验证用户所有禁止动作服务端均返回稳定错误；不能只依赖前端禁用按钮。

## M2.2 登录、Session 和 CSRF

- [ ] `M2.2.1` 实现常量时间登录失败处理和账号/IP 限流。
- [ ] `M2.2.2` 设置 `__Host-bblbb_session`：Secure、HttpOnly、SameSite=Lax、Path=/、无 Domain。
- [ ] `M2.2.3` Session Token 至少 256 bit 熵；数据库只存 hash。
- [ ] `M2.2.4` 实现 idle timeout、absolute timeout、登出、密码修改撤销和管理员撤销。
- [ ] `M2.2.5` 登录、权限提升和密码修改时旋转 Session。
- [ ] `M2.2.6` 实现 `GET /auth/csrf`、synchronizer token、Origin/Referer 校验和匿名预认证 CSRF。
- [ ] `M2.2.7` 所有 Cookie 写请求强制 CSRF；Bearer-only 且不使用 Cookie 的请求不要求 CSRF。
- [ ] `M2.2.8` 用户可查看并撤销自己的设备 Session。

## M2.3 密码、TOTP 和高风险认证

- [ ] `M2.3.1` 实现找回密码：统一响应、30 分钟一次性 Token、成功后撤销其他 Session。
- [ ] `M2.3.2` 实现普通用户可选 TOTP、恢复码 hash 和 enrollment。
- [ ] `M2.3.3` administrator、moderator 和高风险账务账号强制 2FA。
- [ ] `M2.3.4` 未完成 2FA enrollment 的高权限账号不得取得对应高权限 Session。
- [ ] `M2.3.5` 高风险操作要求近期重新认证，必要时再次要求 TOTP。
- [ ] `M2.3.6` 记录新设备、密码/TOTP 变化、Session 撤销和恢复码使用安全通知。

---

# M3：用户资料、权限和板块

## M3.1 用户资料

- [ ] `M3.1.1` 实现 `/me` 与公开用户 DTO 分离；公开 DTO 不包含邮箱、Session、IP、内部处罚和私有资产。
- [ ] `M3.1.2` 实现昵称、头像、简介、签名、时区、主题偏好和隐私设置。
- [ ] `M3.1.3` 实现用户主页、作者资料卡和 Profile Cover 投影。
- [ ] `M3.1.4` Cover 与头像、文章/帖子封面、正文图片和普通附件共享总容量。
- [ ] `M3.1.5` 用户注销后公开内容保留并匿名化作者；身份数据按保留策略清理。
- [ ] `M3.1.6` 删除内容、附件和 Cover 默认保留 30 天；法律保留例外必须有审计。

## M3.2 RBAC、对象授权和板块范围

- [ ] `M3.2.1` 实现 administrator/moderator/member 角色和自定义角色。
- [ ] `M3.2.2` 区分全局角色、板块角色和对象级作者权限。
- [ ] `M3.2.3` 实现 `resource.action` 权限命名和权限矩阵自动检查。
- [ ] `M3.2.4` 所有读写 Handler 同时做动作权限和对象范围判断。
- [ ] `M3.2.5` 管理员/版主越权查看隐藏内容必须写审计。
- [ ] `M3.2.6` 用户状态、处罚、冷静期和 Feature Flag 在服务端实时参与授权。

## M3.3 板块、标签和搜索基础

- [ ] `M3.3.1` 实现板块层级、排序、发帖规则、可见性和板块角色。
- [ ] `M3.3.2` 实现标签、标签分组、slug 唯一性和管理 CRUD。
- [ ] `M3.3.3` 实现 SQLite FTS5、MySQL/MariaDB 全文索引适配。
- [ ] `M3.3.4` 搜索结果重新执行内容可见性、删除、审核和索引退出过滤。
- [ ] `M3.3.5` 限制搜索长度、搜索窗口、分页深度和匿名频率。

---

# M4：核心内容、回复和可见性

## M4.1 Markdown 内容管线

- [ ] `M4.1.1` 只接受 Markdown，禁止原始 HTML/BBCode。
- [ ] `M4.1.2` Rust 生成并清洗 HTML：标签、属性、URL 协议、外链和 iframe 白名单。
- [ ] `M4.1.3` 保存 Markdown 源文、清洗后 HTML、渲染器版本和 revision。
- [ ] `M4.1.4` 前端只有专用安全 HTML 组件可以使用 `{@html}`。
- [ ] `M4.1.5` 增加 XSS、Markdown、链接、图片和隐藏正文泄漏测试。

## M4.2 帖子、文章、草稿和回复

- [ ] `M4.2.1` 实现 article/discussion、标题、slug、摘要、板块、标签和封面引用。
- [ ] `M4.2.2` 实现 draft、preview、published、scheduled_at 和编辑历史。
- [ ] `M4.2.3` 实现回复、引用、parent_id、楼层号、编辑和软删除。
- [ ] `M4.2.4` 实现置顶、精华、关闭、移动、合并和恢复。
- [ ] `M4.2.5` 所有更新使用 `If-Match`/version；冲突返回 409。
- [ ] `M4.2.6` 创建、编辑、定时发布和管理员代发都重新校验作者当前等级。

## M4.3 内容访问策略

- [ ] `M4.3.1` 只支持 public、logged_in、after_reply、level、paid。
- [ ] `M4.3.2` 明确不支持指定用户可见。
- [ ] `M4.3.3` `visibility_level` 不得高于作者当前等级，否则返回 `422 visibility_level_exceeds_author`。
- [ ] `M4.3.4` 未授权响应完全省略隐藏正文，不返回加密、编码、摘要或高亮版本。
- [ ] `M4.3.5` 搜索、通知、RSS、OpenGraph、JSON-LD、sitemap 和缓存均重新执行可见性过滤。
- [ ] `M4.3.6` 付费解锁和 grant 创建与积分扣款同一事务、幂等。

---

# M5：审核、举报、处罚、申诉和通知

## M5.1 先发布后风险审核

- [ ] `M5.1.1` 普通内容默认先发布。
- [ ] `M5.1.2` 高风险规则或 AI 风险命中后暂不公开并进入人工审核队列。
- [ ] `M5.1.3` AI 只能提供风险建议，不能直接封禁、删除或放行。
- [ ] `M5.1.4` 作者只能看到安全的审核状态，不得看到私密风险输入、举报人和内部备注。
- [ ] `M5.1.5` 新用户前 N 帖、链接数量、重复内容、敏感词和频率规则可配置。

## M5.2 举报、案件和处罚

- [ ] `M5.2.1` 实现帖子、回复、用户和附件举报。
- [ ] `M5.2.2` 举报原因、详情长度、去重和撤回规则服务端校验。
- [ ] `M5.2.3` 实现 moderation case：open/triaged/investigating/resolved/rejected/reopened。
- [ ] `M5.2.4` 实现 hide/restore/delete/move/lock/pin/feature/merge/edit_for_moderation。
- [ ] `M5.2.5` 实现 warning/rate_limit/mute/board_mute/ban，支持期限、撤销和审计。
- [ ] `M5.2.6` ban 实时撤销 Session 和 OIDC Refresh Token。
- [ ] `M5.2.7` 实现 appeal：submitted/reviewing/upheld/partially_upheld/rejected/withdrawn。
- [ ] `M5.2.8` 防止处理自己案件，版主只能处理板块范围。

## M5.3 通知和邮件

- [ ] `M5.3.1` 实现回复、引用、提及、审核、等级、OIDC 安全通知。
- [ ] `M5.3.2` 站内通知使用不可见摘要过滤。
- [ ] `M5.3.3` 邮件通过 Outbox/Job 投递，失败重试和 dead 可查询。
- [ ] `M5.3.4` 实现通知已读、偏好和去重。

---

# M6：附件、S3、媒体资源和下载抵扣

## M6.1 Storage Adapter

- [ ] `M6.1.1` 定义 local/S3 统一 Storage Adapter。
- [ ] `M6.1.2` S3 凭据仅由 Rust 读取；支持 AWS S3、MinIO、R2。
- [ ] `M6.1.3` 本地附件存储在 Web 根目录外，key 不可猜。
- [ ] `M6.1.4` 实现两阶段上传：create → presigned upload/stream → complete。
- [ ] `M6.1.5` complete 服务端执行 HEAD、大小、元数据、magic、hash、病毒和图片处理校验。
- [ ] `M6.1.6` 状态实现 pending/processing/ready/quarantined/deleted。
- [ ] `M6.1.7` 临时 URL TTL 可配置；URL 过期不删除对象、不释放容量。
- [ ] `M6.1.8` 删除和替换进入保留期；物理删除成功且配额结算后才释放容量。

## M6.2 配额、头像和 Cover

- [ ] `M6.2.1` 后台按等级配置单文件大小、总容量和可选每日上传量。
- [ ] `M6.2.2` 创建与 complete 两阶段都重新读取当前等级和总容量。
- [ ] `M6.2.3` Cover 上传、更换、移除、预览和降级背景完整可用。
- [ ] `M6.2.4` 共享 `quota_bytes_charged` 正确覆盖头像、Cover、封面、正文图片和附件。
- [ ] `M6.2.5` 图片重新解码、缩略图、像素上限和 SVG 默认拒绝。

## M6.3 下载抵扣

- [ ] `M6.3.1` 实现下载策略优先级：站点、板块、附件、等级、用户和 Feature Flag。
- [ ] `M6.3.2` 浏览器不能提交 user/owner/amount/currency；价格由后端计算。
- [ ] `M6.3.3` 免费下载也创建 authorization。
- [ ] `M6.3.4` 扣款、不可变流水、授权、审计和 Outbox 同事务。
- [ ] `M6.3.5` URL 签发失败返回 `download_url_unavailable`，不得重复扣费。
- [ ] `M6.3.6` 有效授权重签 URL 不重复扣费。
- [ ] `M6.3.7` 完成 SQLite/MySQL/MariaDB 并发竞争测试。
- [ ] `M6.3.8` 默认关闭 Download Billing，策略打开前通过专项门槛。

---

# M7：积分账本、等级和内部商城

## M7.1 不可变积分账本

- [ ] `M7.1.1` 建立 currency、point_accounts、point_operations、point_transactions。
- [ ] `M7.1.2` 所有余额变更在同一事务中更新账户并写流水。
- [ ] `M7.1.3` 所有奖励、消费、补偿和撤销支持幂等键。
- [ ] `M7.1.4` SQLite 使用 `BEGIN IMMEDIATE`；MySQL/MariaDB 固定锁顺序和行锁。
- [ ] `M7.1.5` 禁止充值、提现、现金兑换、普通用户转账和现实价值承诺。
- [ ] `M7.1.6` 管理员发放必须有原因、审计和可选双人复核。
- [ ] `M7.1.7` 奖励撤销使用反向补偿流水，不修改历史。

## M7.2 等级、签到和活跃

- [ ] `M7.2.1` 实现经验累计、等级计算和等级权益。
- [ ] `M7.2.2` 每日首次有效业务页面访问自动签到；禁止静态资源、预取、爬虫、匿名和失败请求触发奖励。
- [ ] `M7.2.3` 使用用户时区，缺省回退站点时区；实现日界线测试。
- [ ] `M7.2.4` 签到、优质内容、有效点赞、回复、互动和活动任务具有每日上限和反刷规则。
- [ ] `M7.2.5` 自赞、撤赞重赞、批量账号和对刷事件不产生点赞奖励。
- [ ] `M7.2.6` 活跃奖励可延迟确认、反向撤销和审计。

## M7.3 内部商城和装扮

- [ ] `M7.3.1` 实现商品、库存、价格版本、销售时间、等级门槛、限购和有效期。
- [ ] `M7.3.2` 永久和限时商品并存；商品 Token 为后端枚举，禁止 style/HTML/SVG/远程资源。
- [ ] `M7.3.3` 实现昵称颜色、头像挂件、边框、徽章、主页/帖子装饰、Reaction 包。
- [ ] `M7.3.4` 实现购买、订单、entitlement、装备槽和 PresentationProjection。
- [ ] `M7.3.5` 购买价格、库存、余额和等级由后端重新计算。
- [ ] `M7.3.6` 购买使用不可变账本和幂等；并发库存/余额不可超卖。
- [ ] `M7.3.7` 数字装扮默认不退款；重复扣款、权益未发放和平台异常必须补偿。
- [ ] `M7.3.8` Reaction 不能改变审核、排序、权限或现金价值。
- [ ] `M7.3.9` 管理后台可配置商城、商品、库存、活动规则和退款补偿。

---

# M8：搜索、SEO、RSS 和反爬

- [ ] `M8.1` 实现公开搜索、FTS5/全文索引适配和结果可见性过滤。
- [ ] `M8.2` 实现 RSS/Atom、sitemap、OpenGraph、JSON-LD 和 canonical URL。
- [ ] `M8.3` 实现动态 robots.txt 和 `X-Robots-Tag`/meta noindex。
- [ ] `M8.4` 默认拒绝 GPTBot、CCBot、Google-Extended、ClaudeBot 等 AI 训练爬虫。
- [ ] `M8.5` 普通搜索引擎只索引明确允许的公开内容。
- [ ] `M8.6` 作者支持逐帖退出搜索和 AI 摘要；管理员支持全站/板块强制关闭。
- [ ] `M8.7` 隐藏、登录、回复、等级、付费、审核、删除和封禁内容不进入任何公开投影。
- [ ] `M8.8` 实现按账号、可信代理 IP、IP 段、UA 一致性、并发、顺序扫描、分页和失败率的行为检测。
- [ ] `M8.9` 实现观察/降速 → 429 → 挑战 → 临时封禁 → 人工复核。
- [ ] `M8.10` 隔离搜索、RSS、sitemap 和公开文章限流桶；缓存必须按权限维度隔离。
- [ ] `M8.11` 按 [`docs/CRAWLER-POLICY.md`](docs/CRAWLER-POLICY.md) 完成全套投影和伪装爬虫测试。

---

# M9：AI Gateway

- [ ] `M9.1` 实现 Provider allowlist、HTTPS/SSRF/DNS rebinding 防护和 Secret 隔离。
- [ ] `M9.2` 实现用途、模型、超时、并发、token/金额预算和熔断配置。
- [ ] `M9.3` 浏览器不能直连 Provider；所有请求经过 Rust Gateway。
- [ ] `M9.4` 用户正文外发前每次明确确认，展示 Provider、用途、留存、训练、区域和数据模式。
- [ ] `M9.5` 默认脱敏；隐藏正文、私密审核备注和 Secret 不外发。
- [ ] `M9.6` 实现 consent 创建、撤回、版本、文案 hash 和审计证据。
- [ ] `M9.7` 实现 AI Task 状态机、取消、重试、死信和幂等。
- [ ] `M9.8` 实现 formatting/SEO/tagging/moderation 版本化 Suggestion。
- [ ] `M9.9` Suggestion 采纳必须重新鉴权、校验 base_version/If-Match 和 XSS/Markdown/SEO。
- [ ] `M9.10` AI 失败不能阻塞普通发帖和人工审核，不能自动处罚、删除或放行。
- [ ] `M9.11` AI 默认 Feature Flag 关闭；通过同意、撤回、预算、故障和迟到输出专项门槛后才能开启。

---

# M10：视频、HLS 和西瓜视频插件

- [ ] `M10.1` 定义核心 Video Service 和 Direct/HLS/Xigua Provider Adapter 接口。
- [ ] `M10.2` 支持 MP4、WebM、OGV、HLS `.m3u8` 和西瓜公开页面 URL。
- [ ] `M10.3` resolve 只返回短效 `resolution_id`，创建时不接受可信 MIME、iframe HTML、Key 或签名 URL。
- [ ] `M10.4` 实现来源白名单、SSRF、DNS 重绑定、私网阻断、超时、响应大小和重定向限制。
- [ ] `M10.5` 实现 HLS playlist 深度、分片数量、字节预算、Key 和跨域限制。
- [ ] `M10.6` 生成动态 CSP；Provider 不符合策略时降级官方外链。
- [ ] `M10.7` 实现 Embed pending/ready/blocked/error/removed 状态机和异步 refresh。
- [ ] `M10.8` 不抓取、转存、破解或绕过第三方鉴权。
- [ ] `M10.9` 通过 SSRF Corpus、HLS Corpus、CSP、浏览器直连/外链降级和版权阻断测试后再开启 Provider。

---

# M11：OIDC Provider

## M11.1 Client 和协议实现

- [ ] `M11.1.1` 实现 Authorization Code + PKCE S256；拒绝 implicit/password/device flow。
- [ ] `M11.1.2` 实现 Public/Confidential Client、精确 redirect URI 和 post logout redirect。
- [ ] `M11.1.3` 实现 discovery、authorize、token、userinfo、JWKS、revoke、logout。
- [ ] `M11.1.4` 实现 `openid/profile/email` scope 和 Pairwise Subject。
- [ ] `M11.1.5` 实现 RS256 ID Token、opaque Access Token、Refresh Token Rotation。
- [ ] `M11.1.6` 实现授权码一次性消费、nonce、state、aud/iss/exp/iat/auth_time 校验。
- [ ] `M11.1.7` Refresh Token reuse 撤销整个 family 并通知。
- [ ] `M11.1.8` OIDC 端点使用标准协议错误，不套业务 Problem JSON。

## M11.2 同意、密钥和安全

- [ ] `M11.2.1` 实现逐 Client/逐 Scope consent、撤销和安全通知。
- [ ] `M11.2.2` 实现 interaction 查询和 Session + CSRF decision。
- [ ] `M11.2.3` 私钥加密保存；JWKS 先发布新公钥再切换 active key。
- [ ] `M11.2.4` 旧 key 保留至 Token 过期加安全余量；ready 在 key 无法恢复时失败。
- [ ] `M11.2.5` OIDC 默认关闭；不影响本地登录和核心论坛。
- [ ] `M11.2.6` 通过 conformance profile、至少两个独立 RP、key rotation、Refresh reuse 和密钥恢复演练。

---

# M12：第三方 Marketplace 和原子账务

## M12.1 Client、Offer 和额度

- [ ] `M12.1.1` 仅允许管理员审核的 Confidential Client 接入。
- [ ] `M12.1.2` 交易对象限定为第三方应用服务额度，不开放任意用户物品市场。
- [ ] `M12.1.3` 实现精确 HTTPS redirect、条款、隐私 URL、Webhook URL 和 scope 审批。
- [ ] `M12.1.4` Offer 金额、货币、库存、版本和平台费由服务端登记。
- [ ] `M12.1.5` merchant balance 只能站内再次消费，不提现、不兑换现金、不转给普通用户。
- [ ] `M12.1.6` 普通 `openid/profile/email` scope 永远不具备扣款能力。

## M12.2 Checkout 和账务

- [ ] `M12.2.1` Checkout Intent 绑定 Client、user、Offer 版本、金额、货币、订单号和短 TTL。
- [ ] `M12.2.2` 使用 user-bound Token 创建 Intent；不接受请求体 user_id、金额、收款方或余额。
- [ ] `M12.2.3` 用户托管确认页显示市场、商品、数量、准确金额、余额变化和授权期限。
- [ ] `M12.2.4` 购买事务原子消费 Intent、锁库存、扣买方、入商户待结算余额、写流水、审计和 Outbox。
- [ ] `M12.2.5` SQLite `BEGIN IMMEDIATE` 与 MySQL/MariaDB 固定行锁顺序通过竞争测试。
- [ ] `M12.2.6` 同一幂等键重放返回原结果；不同请求摘要返回 409。
- [ ] `M12.2.7` Webhook 只由提交后 Outbox 投递；延迟、重复、乱序均可对账。

## M12.3 退款、对账和紧急停用

- [ ] `M12.3.1` 退款使用引用原交易的 reversal operation，不能修改历史流水。
- [ ] `M12.3.2` 累计退款不能超过原购买金额；并发退款锁定原 Purchase。
- [ ] `M12.3.3` Client 只能退款自己的交易；管理员退款要求近期认证、原因和限额。
- [ ] `M12.3.4` 实现增量对账、Purchase 查询、Webhook replay 和 Client 紧急禁用。
- [ ] `M12.3.5` 通过双边账本恒等式、并发库存/退款、user-bound checkout、Webhook 对账和紧急冻结门槛。

---

# M13：主题、插件和管理后台

- [ ] `M13.1` 实现数据型主题 Token、Logo、字号、密度和运行时安全切换。
- [ ] `M13.2` 主题不存在、不兼容或被停用时回退 default 并记录告警。
- [ ] `M13.3` 用户主题偏好页面和 SSR 使用同一 `theme_revision`。
- [ ] `M13.4` 管理员可上传受控数据包；资源走附件安全处理。
- [ ] `M13.5` 代码型主题只能构建时编译，禁止在线上传并立即执行 Svelte/JS/Rust/WASM。
- [ ] `M13.6` v1 插件只允许配置型后端规则和已预编译前端扩展。
- [ ] `M13.7` 审计、权限、账本和审核不可由插件关闭或替代。
- [ ] `M13.8` 管理后台覆盖用户、角色、板块、审核、存储、配额、商城、AI、视频、OIDC、Marketplace、下载和 Feature Flag。
- [ ] `M13.9` 每个高风险设置变更要求 reason、version、近期认证和审计。

---

# M14：前端完整实现和可访问性

- [ ] `M14.1` 将原型路由映射为 SvelteKit 路由，不把原型 mock/store 当作生产数据源。
- [ ] `M14.2` 实现公开匿名浏览、搜索、主页、资料 Hover Card、图片/视频和 RSS/Atom。
- [ ] `M14.3` 实现注册、验证、登录、Session、密码、TOTP 和找回流程。
- [ ] `M14.4` 实现编辑器、草稿、预览、审核状态、回复和可见性 UI。
- [ ] `M14.5` 实现附件上传进度、处理中、隔离、失败、配额和短效 URL 过期重签。
- [ ] `M14.6` 实现商城、衣柜、装备槽、限时权益、自动签到和 Reaction。
- [ ] `M14.7` 实现管理员后台及权限范围提示，禁止用前端隐藏代替授权。
- [ ] `M14.8` 所有页面通过键盘、焦点、屏幕阅读器、减少动效和移动端验收。
- [ ] `M14.9` Playwright 覆盖匿名、未验证、普通用户、版主、管理员和被处罚用户。
- [ ] `M14.10` axe/WCAG 2.2 AA 通过；隐藏内容不进入 DOM、SSR、预取或客户端状态。

---

# M15：生产运维、部署和恢复

## M15.1 单机部署

- [ ] `M15.1.1` 创建 Rust release、SvelteKit adapter-node build 和 Caddy 配置模板。
- [ ] `M15.1.2` systemd 管理 backend、frontend、worker；服务用户无 release 写权限。
- [ ] `M15.1.3` 配置 loopback、Caddy TLS、压缩、安全头、可信代理和请求体限制。
- [ ] `M15.1.4` 默认生产流程不在服务器上执行 npm install/build。
- [ ] `M15.1.5` 启动检查 origin、Cookie、数据库、目录、迁移和密钥。
- [ ] `M15.1.6` `/readyz` 只对本机/受控监控开放。

## M15.2 日志、指标和告警

- [ ] `M15.2.1` Rust/SvelteKit 输出结构化 JSON 日志并贯通 request ID。
- [ ] `M15.2.2` 禁止记录 Cookie、Authorization、OAuth code/token、密码、完整邮箱和隐藏正文。
- [ ] `M15.2.3` 建立 HTTP、DB pool、SQLite busy、Session 失败、Job/Outbox、上传、OAuth 和账务指标。
- [ ] `M15.2.4` 建立 dead job、Webhook、迁移、备份、存储和磁盘容量告警。
- [ ] `M15.2.5` 记录慢请求和慢查询，避免输出敏感参数。

## M15.3 备份、恢复和升级

- [ ] `M15.3.1` 实现 SQLite WAL 安全备份，不直接复制活跃数据库文件。
- [ ] `M15.3.2` 备份数据库、附件/S3 version、主题、配置、加密 OIDC 私钥和独立解密密钥恢复方案。
- [ ] `M15.3.3` 建立每日备份、每周恢复演练、异地加密和应用账号不可删除备份。
- [ ] `M15.3.4` 记录 RPO/RTO，并根据实际部署验证，不沿用未经验证的默认值。
- [ ] `M15.3.5` 发布前执行上一版本迁移、兼容回滚或明确不可回滚说明。
- [ ] `M15.3.6` 恢复后验证用户、账本、附件 hash、授权 grant、迁移版本和 OIDC 签名。

---

# M16：测试、性能和安全验收

## M16.1 单元、集成和契约

- [ ] `M16.1.1` 每个状态机合法/非法迁移均有测试。
- [ ] `M16.1.2` 每个 API 错误码有至少一个 Fixture 和客户端映射。
- [ ] `M16.1.3` OpenAPI 与 Handler 路由、权限、CSRF、幂等和响应 schema 自动核对。
- [ ] `M16.1.4` SQLite、MySQL 8、MariaDB 10.11 运行同一核心契约测试。
- [ ] `M16.1.5` 测试事务回滚、重复请求、并发锁、版本冲突和 Outbox 重放。

## M16.2 安全和隐私

- [ ] `M16.2.1` OWASP ASVS 基线检查完成。
- [ ] `M16.2.2` IDOR、权限升级、CSRF、Session fixation、Token 泄漏和缓存隔离测试通过。
- [ ] `M16.2.3` Markdown XSS、附件恶意文件、图片解压炸弹、SVG、SSRF、DNS rebinding 和 HLS 测试通过。
- [ ] `M16.2.4` 未验证用户、冷静期用户、被禁言/封禁用户的服务端权限测试通过。
- [ ] `M16.2.5` 隐藏内容不得通过 API、SSR、DOM、日志、搜索、RSS、SEO、OpenGraph、JSON-LD、AI 和缓存泄漏。
- [ ] `M16.2.6` 数据导出、注销、匿名化、30 天删除和法律保留测试通过。
- [ ] `M16.2.7` AI 外发同意、撤回、Provider、训练策略和迟到输出测试通过。

## M16.3 反爬和公开访问

- [ ] `M16.3.1` 验证 robots 只是声明层，Rust 授权和限流才是边界。
- [ ] `M16.3.2` 测试搜索引擎、AI 训练爬虫、伪造 UA、代理头和未知机器人。
- [ ] `M16.3.3` 测试观察/降速、429、挑战、临时封禁和人工复核状态机。
- [ ] `M16.3.4` 测试作者逐帖退出和管理员全站/板块关闭优先级。
- [ ] `M16.3.5` 测试 RSS、sitemap、缓存、OpenGraph 和 JSON-LD 的过滤一致性。

## M16.4 性能和容量

- [ ] `M16.4.1` 记录 CPU、RAM、数据库、数据量、并发和压测命令。
- [ ] `M16.4.2` 建立 SQLite 512MB 场景：10 万用户、100 万帖子/回复合成数据。
- [ ] `M16.4.3` 测试首页、文章、板块 SSR、登录、发帖、回复、积分并发和 worker 延迟。
- [ ] `M16.4.4` 记录公开文章、登录、发帖的 p95 SLO 和峰值 RSS。
- [ ] `M16.4.5` 验证无持续 SQLite busy、无无限增长队列和磁盘余量足够。

---

# M17：预发布、灰度和正式上线

## M17.1 Release Candidate

- [ ] `M17.1.1` 从冻结文档生成 RC 变更清单和数据库兼容性说明。
- [ ] `M17.1.2` 部署与生产同构的单机预发布环境。
- [ ] `M17.1.3` 运行完整 CI、三数据库契约、Playwright、axe、安全和性能测试。
- [ ] `M17.1.4` 默认关闭 AI、Video Provider、Download Billing、OIDC、Marketplace，验证核心论坛可用。
- [ ] `M17.1.5` 执行管理员、版主、普通用户、未验证用户、匿名用户和被处罚用户冒烟。
- [ ] `M17.1.6` 执行备份、恢复、迁移、回滚和密钥恢复演练。
- [ ] `M17.1.7` 所有 P0/P1 缺陷关闭；剩余 P2 必须有负责人、计划和默认关闭状态。

## M17.2 Feature Flag 专项启用

- [ ] `M17.2.1` 核心论坛和附件在默认配置下上线。
- [ ] `M17.2.2` 通过 Download Billing 门槛后再开启下载抵扣。
- [ ] `M17.2.3` 通过 AI 门槛、Provider 和逐次同意测试后再开启 AI。
- [ ] `M17.2.4` 通过 Video SSRF/HLS/CSP/版权门槛后逐 Provider 开启。
- [ ] `M17.2.5` 通过 OIDC conformance、RP 集成和密钥恢复后开启 OIDC。
- [ ] `M17.2.6` 通过 Marketplace 账务、并发、退款、对账和紧急冻结后逐 Client/Scope 开启。
- [ ] `M17.2.7` 每次开启记录审批人、范围、版本、时间、回滚操作和观察指标。

## M17.3 正式上线

- [ ] `M17.3.1` 确认实际部署地区、运营主体、用户条款、隐私政策、邮件政策和内容审核责任。
- [ ] `M17.3.2` 设置生产域名、TLS、DNS、可信代理、Caddy、systemd 和文件权限。
- [ ] `M17.3.3` 执行最终数据库迁移、备份和恢复点确认。
- [ ] `M17.3.4` 执行匿名公开浏览、注册、邮箱验证、登录、发帖、回复、举报和管理员处理冒烟。
- [ ] `M17.3.5` 验证监控、告警、日志脱敏、磁盘/WAL/队列指标。
- [ ] `M17.3.6` 发布后观察窗口内不打开未验收 Feature Flag。
- [ ] `M17.3.7` 完成上线后 24 小时、7 天复盘并记录到 `docs/CHANGELOG.md`。

---

# 细粒度执行计划

## 任务粒度和完成记录规范

每个叶子任务应控制在约 15–60 分钟；超过 60 分钟必须继续拆分。完成任务时必须在任务行追加：

- 实现文件或迁移文件；
- 验收命令及结果；
- 契约/权限/审计影响；
- 如有例外，记录原因和后续任务编号。

任务状态只允许：`[ ]` 未开始、`[~]` 进行中、`[x]` 已完成、`[!]` 阻塞。每次开发只推进一个叶子任务，完成后再开始下一个。

## NOW：第一条可运行业务链

### NOW-001 工程命令统一（依赖：无）

- [ ] `NOW-001-a` 确认根目录任务入口名称：`check`、`dev`、`test`、`build`。
- [ ] `NOW-001-b` 增加后端命令：`check:backend`、`test:backend`、`build:backend`。
- [ ] `NOW-001-c` 增加前端命令：`check:frontend`、`test:frontend`、`build:frontend`。
- [ ] `NOW-001-d` 增加 `check:openapi`：YAML、operationId、内部引用和必需扩展检查。
- [ ] `NOW-001-e` 增加 `check:migrations`：SQLite 本地执行，MySQL/MariaDB 由 CI 执行。
- [ ] `NOW-001-f` 增加 `check:all`，任何子命令失败都返回非零。
- [ ] `NOW-001-g` 在干净 shell 中运行一次根目录 `check:all` 并记录输出。

**验收：** 不需要手动切换目录即可运行全部检查；不修改原型既有命令语义。

### NOW-002 数据库连接和迁移（依赖：NOW-001）

- [ ] `NOW-002-a` 在 backend 添加 `sqlx` runtime、SQLite、MySQL 和 MariaDB feature。
- [ ] `NOW-002-b` 扩展配置结构：数据库 URL、最大连接数、busy timeout、迁移目录。
- [ ] `NOW-002-c` 实现数据库池创建和启动时连接测试。
- [ ] `NOW-002-d` 实现 `migrate --check`，只检查版本和 checksum，不执行迁移。
- [ ] `NOW-002-e` 实现显式 `migrate` 子命令，迁移失败立即停止。
- [ ] `NOW-002-f` 为 SQLite 设置 WAL、foreign_keys 和 busy_timeout。
- [ ] `NOW-002-g` 为 MySQL/MariaDB 设置事务隔离和连接字符集。
- [ ] `NOW-002-h` 写入 migration history/checksum 表并测试重复执行幂等。
- [ ] `NOW-002-i` 在三数据库执行空库迁移和第二次重复迁移。

**验收：** 三数据库迁移结果具有相同逻辑表和状态；生产启动不自动执行未知迁移。

### NOW-003 健康检查和错误边界（依赖：NOW-002）

- [ ] `NOW-003-a` 保持 `/healthz` 只检查进程，不访问数据库或外部 Provider。
- [ ] `NOW-003-b` 添加受保护 `/readyz`，检查数据库连接和迁移版本。
- [ ] `NOW-003-c` 为数据库不可用、迁移不匹配分别定义 Problem code。
- [ ] `NOW-003-d` 将 request_id 写入成功响应、错误响应、日志和测试断言。
- [ ] `NOW-003-e` 为 readiness 失败编写集成测试。
- [ ] `NOW-003-f` 验证 Caddy 只公开 `/healthz`，`/readyz` 仅本机/受控监控访问。

### NOW-004 身份数据迁移（依赖：NOW-002）

- [ ] `NOW-004-a` 从 skeleton 拆出 users 字段：username、email、display_name、status、timezone。
- [ ] `NOW-004-b` 增加 email verification token hash、expires_at、consumed_at。
- [ ] `NOW-004-c` 增加 password reset token hash、expires_at、consumed_at。
- [ ] `NOW-004-d` 增加 user_sessions 的 device、last_seen、idle/absolute expiry、revoked_at。
- [ ] `NOW-004-e` 增加 TOTP enrollment、recovery code hash 和 2FA required 状态。
- [ ] `NOW-004-f` 为每种数据库编写对应迁移，不修改已执行 skeleton。
- [ ] `NOW-004-g` 添加唯一约束、索引、外键和状态 CHECK 的三数据库测试。

### NOW-005 注册（依赖：NOW-003、NOW-004）

- [ ] `NOW-005-a` 实现注册请求 schema 和字段长度/格式校验。
- [ ] `NOW-005-b` 实现用户名、邮箱规范化和唯一性冲突的统一错误。
- [ ] `NOW-005-c` 实现 Argon2id hash 参数配置和测试向量。
- [ ] `NOW-005-d` 在事务内创建 pending_verification 用户和 token hash。
- [ ] `NOW-005-e` 在同一事务写 verification email Outbox 事件。
- [ ] `NOW-005-f` 注册响应不返回密码、token 或邮箱存在性信息。
- [ ] `NOW-005-g` 添加限流、重复提交和事务回滚测试。

### NOW-006 邮箱验证和冷静期（依赖：NOW-005）

- [ ] `NOW-006-a` 实现验证 token hash 查询、过期和一次性消费。
- [ ] `NOW-006-b` 验证成功将用户迁移为 active，并记录事件。
- [ ] `NOW-006-c` 实现重发验证邮件及发送频率限制。
- [ ] `NOW-006-d` 实现后台冷静期策略的读取、更新、版本和审计。
- [ ] `NOW-006-e` 实现未验证用户动作矩阵：允许登录/浏览/账号修改，拒绝写内容/上传/交易/奖励。
- [ ] `NOW-006-f` 实现冷静期动作矩阵并确保服务端而非前端裁决。
- [ ] `NOW-006-g` 为匿名、未验证、冷静期、正常和封禁用户编写权限测试。

### NOW-007 登录和 Session（依赖：NOW-004、NOW-006）

- [ ] `NOW-007-a` 实现登录密码验证和常量时间失败路径。
- [ ] `NOW-007-b` 实现 Session 生成、hash 存储和 `__Host-bblbb_session` Cookie。
- [ ] `NOW-007-c` 登录时旋转旧 Session 并写安全审计。
- [ ] `NOW-007-d` 实现登出当前 Session。
- [ ] `NOW-007-e` 实现 `/me` 安全投影和邮箱验证状态。
- [ ] `NOW-007-f` 实现 Session 列表、设备信息和逐个撤销。
- [ ] `NOW-007-g` 修改密码后撤销其他 Session。
- [ ] `NOW-007-h` 实现 idle/absolute timeout 的请求时判断。
- [ ] `NOW-007-i` 测试 Cookie 属性、Session fixation、过期、撤销和账号状态变化。

### NOW-008 CSRF 和请求来源（依赖：NOW-007）

- [ ] `NOW-008-a` 实现 Session 绑定 synchronizer CSRF token。
- [ ] `NOW-008-b` 实现 `GET /api/v1/auth/csrf` 的 private/no-store 响应。
- [ ] `NOW-008-c` 为匿名注册/登录建立预认证 CSRF 状态。
- [ ] `NOW-008-d` 校验 `X-CSRF-Token` 和 Origin。
- [ ] `NOW-008-e` 无 Origin 时按策略校验 Referer。
- [ ] `NOW-008-f` 验证 GET/HEAD/OPTIONS 无业务副作用。
- [ ] `NOW-008-g` 验证 Bearer-only 无 Cookie 请求不被错误要求 CSRF。
- [ ] `NOW-008-h` 编写浏览器集成测试：缺 token、错误 token、跨 Origin、正确 token。

### NOW-009 第一条三数据库契约链（依赖：NOW-005 至 NOW-008）

- [ ] `NOW-009-a` 为注册创建相同 Fixture 和请求。
- [ ] `NOW-009-b` 为验证、登录、注销和 Session 撤销创建相同 Fixture。
- [ ] `NOW-009-c` 断言三数据库 HTTP 状态、Problem code 和投影一致。
- [ ] `NOW-009-d` 断言事务失败时用户、token、Session 和 Outbox 全部回滚。
- [ ] `NOW-009-e` 在 CI service container 执行 MySQL 8 和 MariaDB 10.11。
- [ ] `NOW-009-f` 保留测试日志、数据库版本和迁移版本作为构建证据。

### NOW-010 前端第一条业务链（依赖：NOW-005 至 NOW-008）

- [ ] `NOW-010-a` 创建注册页面和字段错误映射。
- [ ] `NOW-010-b` 创建邮箱验证提示、重发验证入口和结果状态。
- [ ] `NOW-010-c` 创建登录页面、TOTP 二次输入和统一失败提示。
- [ ] `NOW-010-d` 创建 `/me` 页面并显示邮箱验证、Session 和账号状态。
- [ ] `NOW-010-e` 处理 401、403、409、422、429、503 Problem 响应。
- [ ] `NOW-010-f` 添加键盘/移动端冒烟和 Playwright 流程。

**完成 NOW-010 后，才进入 M3/M4 的论坛内容实现。**

## 后续里程碑拆分规则

M3–M17 的每一个高层任务继续按以下顺序拆成叶子任务：

1. 数据迁移与约束；
2. Rust domain/service；
3. Rust handler 与权限/CSRF/幂等；
4. Outbox/Job/审计；
5. SvelteKit 页面与错误状态；
6. SQLite 契约测试；
7. MySQL/MariaDB 契约测试；
8. 安全/隐私/缓存测试；
9. Playwright、axe 和手工验收；
10. 文档、OpenAPI、错误码、事件目录和 CHANGELOG 同步。

**任何涉及账务、权限、附件、AI 外发、OIDC 或外部市场的任务，不得跳过第 1、3、4、7、8 步。**

---

# 参考文档

- [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md)
- [`docs/PRODUCT-DECISIONS.md`](docs/PRODUCT-DECISIONS.md)
- [`docs/DOCUMENT-STATUS.md`](docs/DOCUMENT-STATUS.md)
- [`openapi/openapi.yaml`](openapi/openapi.yaml)
- [`docs/API.md`](docs/API.md)
- [`docs/API-CONTRACTS.md`](docs/API-CONTRACTS.md)
- [`docs/SCHEMA.md`](docs/SCHEMA.md)
- [`docs/STATE-MACHINES.md`](docs/STATE-MACHINES.md)
- [`docs/PERMISSION-MATRIX.md`](docs/PERMISSION-MATRIX.md)
- [`docs/SECURITY.md`](docs/SECURITY.md)
- [`docs/TESTING.md`](docs/TESTING.md)
- [`docs/OPERATIONS.md`](docs/OPERATIONS.md)
- [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md)
- [`docs/CRAWLER-POLICY.md`](docs/CRAWLER-POLICY.md)
- [`docs/STORAGE.md`](docs/STORAGE.md)
- [`docs/AI.md`](docs/AI.md)
- [`docs/VIDEO-PLUGIN.md`](docs/VIDEO-PLUGIN.md)
- [`docs/MARKETPLACE.md`](docs/MARKETPLACE.md)
- [`docs/INTERNAL-MARKETPLACE.md`](docs/INTERNAL-MARKETPLACE.md)


---

# 上线阻断清单

以下任意一项未完成，禁止标记 v1.0 正式上线：

- [ ] 存在未审计的管理员高风险操作。
- [ ] 存在只在前端执行的权限、可见性、价格、库存或配额校验。
- [ ] 隐藏内容仍可能出现在 API、SSR、DOM、搜索、RSS、SEO、AI 或缓存。
- [ ] 三数据库中任一数据库迁移或关键事务不一致。
- [ ] 积分、购买、下载、解锁或退款可能重复扣款或修改历史流水。
- [ ] 未验证用户仍可执行发帖、回复、上传、交易或奖励动作。
- [ ] Session、CSRF、TOTP、OIDC PKCE 或 Refresh Token reuse 测试失败。
- [ ] S3 URL 过期会错误删除附件或释放容量。
- [ ] AI Provider 未经逐次明确同意接收用户正文。
- [ ] AI 能直接封禁、删除、放行或修改权限。
- [ ] 反爬只依赖 robots 或 User-Agent，缺少 Rust 限流和行为检测。
- [ ] 备份未完成真实恢复演练。
- [ ] OIDC 或 Marketplace 默认开启但尚未通过专项门槛。
- [ ] 没有可执行的迁移、回滚、停用、恢复和事故 Runbook。
- [ ] 法律地区、运营主体和隐私/内容责任没有完成评审。

---

# 参考文档

- [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md)
- [`docs/PRODUCT-DECISIONS.md`](docs/PRODUCT-DECISIONS.md)
- [`docs/DOCUMENT-STATUS.md`](docs/DOCUMENT-STATUS.md)
- [`openapi/openapi.yaml`](openapi/openapi.yaml)
- [`docs/API.md`](docs/API.md)
- [`docs/API-CONTRACTS.md`](docs/API-CONTRACTS.md)
- [`docs/SCHEMA.md`](docs/SCHEMA.md)
- [`docs/STATE-MACHINES.md`](docs/STATE-MACHINES.md)
- [`docs/PERMISSION-MATRIX.md`](docs/PERMISSION-MATRIX.md)
- [`docs/SECURITY.md`](docs/SECURITY.md)
- [`docs/TESTING.md`](docs/TESTING.md)
- [`docs/OPERATIONS.md`](docs/OPERATIONS.md)
- [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md)
- [`docs/CRAWLER-POLICY.md`](docs/CRAWLER-POLICY.md)
- [`docs/STORAGE.md`](docs/STORAGE.md)
- [`docs/AI.md`](docs/AI.md)
- [`docs/VIDEO-PLUGIN.md`](docs/VIDEO-PLUGIN.md)
- [`docs/MARKETPLACE.md`](docs/MARKETPLACE.md)
- [`docs/INTERNAL-MARKETPLACE.md`](docs/INTERNAL-MARKETPLACE.md)
