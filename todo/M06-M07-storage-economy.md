# M6-M7：附件、S3、下载抵扣、积分与内部商城

> 总索引：[`../TODO.md`](../TODO.md)
> 通用叶子任务、元数据和证据规则见 [`M00-M02-foundation.md`](M00-M02-foundation.md)。
> 本文件覆盖所有涉及容量、对象生命周期、不可变账本、积分扣款和装扮的高风险路径。

---

<a id="m6"></a>

# M6：附件、媒体资源、S3 与下载抵扣

**完成定义：** 本地和 S3 Adapter 通过同一契约；上传、处理、配额、临时 URL、Range、删除和迁移语义一致；下载抵扣不会重复扣款。

## M06-SCHEMA：附件、引用、配额与下载授权模型

**元数据：** `P0` · `owner=unassigned/backend-db-storage` · `risk=critical` · `depends=M03-PROFILE,M04-SCHEMA` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/storage/model*`、`backend/src/download/model*`、`docs/SCHEMA.md`
**验收：** 三数据库迁移和容量/授权约束契约通过。

- [ ] `M06-SCHEMA-01` `P0` `[45m]` 新增 attachments：owner、backend、object_key、size、hash、media metadata、status、revision 和保留时间。
- [ ] `M06-SCHEMA-02` `P0` `[30m]` 新增 attachment_references，覆盖头像、Cover、正文图、封面和普通附件的稳定引用。
- [ ] `M06-SCHEMA-03` `P0` `[45m]` 新增 user quota counters、quota policy revisions、reserved bytes 和 charged bytes 字段。
- [ ] `M06-SCHEMA-04` `P0` `[45m]` 新增 download_policies、policy revisions、authorizations、transactions 和 sign attempts。
- [ ] `M06-SCHEMA-05` `P0` `[30m]` 定义 pending/processing/ready/quarantined/deleted 状态与非法迁移约束。
- [ ] `M06-SCHEMA-06` `P0` `[30m]` 为 object key、owner/key、引用类型和有效授权建立唯一/索引约束。
- [ ] `M06-SCHEMA-07` `P0` `[45m]` 测试预留容量、并发 complete、降级、删除保留期和物理释放的跨库数值一致。
- [ ] `M06-SCHEMA-08` `P1` `[30m]` 同步 `SCHEMA`、`STORAGE`、`DOWNLOAD-BILLING`、状态机和事件目录。

## M06-ADAPTER：Local/S3 Storage Adapter

**元数据：** `P0` · `owner=unassigned/backend-storage` · `risk=critical` · `depends=M06-SCHEMA,M01-CONFIG` · `blocked=none`
**目标文件：** `backend/src/storage/adapter/`、`backend/src/storage/local*`、`backend/src/storage/s3*`、`backend/tests/storage/`
**验收：** Local、AWS S3、MinIO、R2 目标契约在适配器测试中通过；Secret 不离开 Rust。

- [ ] `M06-ADAPTER-01` `P0` `[45m]` 定义 create/temp/upload/complete/read/range/delete/head/copy/list 的抽象接口。
- [ ] `M06-ADAPTER-02` `P0` `[30m]` 实现本地根目录外存储、不可猜 object key、路径穿越/绝对路径/符号链接阻断。
- [ ] `M06-ADAPTER-03` `P0` `[45m]` 接入 S3 SDK，所有凭据只由 Rust 配置层读取，前端只得到脱敏状态。
- [ ] `M06-ADAPTER-04` `P0` `[45m]` 支持 virtual-hosted-style、path-style、region/`auto` 和 endpoint TLS 校验。
- [ ] `M06-ADAPTER-05` `P1` `[45m]` 实现 multipart upload 生命周期、part 数量/大小上限、complete/abort 和孤儿清理。
- [ ] `M06-ADAPTER-06` `P0` `[45m]` 定义预签名上传/下载请求头绑定和短 TTL，禁止把它当作权限裁决。
- [ ] `M06-ADAPTER-07` `P0` `[45m]` 统一 AWS S3、MinIO、Cloudflare R2 的 contract Fixture；其他兼容服务不得自动宣称支持。
- [ ] `M06-ADAPTER-08` `P0` `[45m]` 处理 S3 403/404/409/429/5xx、超时、DNS/TLS 和部分上传的错误分类。
- [ ] `M06-ADAPTER-09` `P0` `[30m]` 测试 Secret 不出现在 API、SSR、hydration、浏览器存储、配置导出、日志和错误中。
- [ ] `M06-ADAPTER-10` `P1` `[30m]` 将 Adapter 运行指标和供应商错误映射到稳定 Problem code。

## M06-UPLOAD：两阶段上传与内容安全处理

**元数据：** `P0` · `owner=unassigned/backend-storage-security` · `risk=critical` · `depends=M06-ADAPTER,M03-AUTHZ` · `blocked=none`
**目标文件：** `backend/src/storage/upload/`、`backend/src/routes/attachments/`、`backend/tests/storage/upload*`
**验收：** create→upload→complete→process→ready/quarantine 状态和故障注入完整通过。

- [ ] `M06-UPLOAD-01` `P0` `[30m]` 实现 create attachment，后端计算 owner、大小上限、策略版本和 object key。
- [ ] `M06-UPLOAD-02` `P0` `[30m]` 在 create 阶段预留容量，拒绝未验证、冷静期、封禁和无上传权限用户。
- [ ] `M06-UPLOAD-03` `P0` `[30m]` 实现 presigned upload 或 Rust stream 两种模式，并限制 Content-Type、body、超时和并发。
- [ ] `M06-UPLOAD-04` `P0` `[45m]` complete 执行服务端 HEAD，重新核对存在性、大小、metadata、owner 和 object key。
- [ ] `M06-UPLOAD-05` `P0` `[45m]` worker 流式执行 magic、hash、病毒扫描、图片重新解码和像素/压缩炸弹限制。
- [ ] `M06-UPLOAD-06` `P0` `[30m]` 默认拒绝 SVG、polyglot、宏文档、可执行文件和 MIME/扩展名欺骗。
- [ ] `M06-UPLOAD-07` `P0` `[30m]` 实现 pending/processing/ready/quarantined/deleted 投影和重试，不把未 ready 附件关联公开内容。
- [ ] `M06-UPLOAD-08` `P0` `[30m]` complete 重复调用幂等；实际大小变大、用户降级或容量被占满时拒绝且不超卖。
- [ ] `M06-UPLOAD-09` `P1` `[45m]` 生成缩略图、移除 EXIF/GPS、记录处理版本和失败原因安全摘要。
- [ ] `M06-UPLOAD-10` `P0` `[45m]` 测试中断上传、签名过期、对象被替换、HEAD 不符、病毒命中和 worker 崩溃恢复。

## M06-QUOTA：等级容量、头像与 Cover

**元数据：** `P0` · `owner=unassigned/backend-storage` · `risk=critical` · `depends=M06-UPLOAD,M03-PROFILE` · `blocked=none`
**目标文件：** `backend/src/storage/quota/`、`backend/src/routes/profile/`、`backend/src/routes/admin/storage/`、`backend/tests/storage/quota*`
**验收：** 后台配置、升级/降级、共享容量和 Cover 生命周期数值一致。

- [ ] `M06-QUOTA-01` `P0` `[30m]` 定义站点硬上限、等级单文件上限、总容量、每日上传量和保留期策略。
- [ ] `M06-QUOTA-02` `P0` `[45m]` 实现管理员读取/更新等级配额，要求版本、reason、recent-auth 和审计。
- [ ] `M06-QUOTA-03` `P0` `[30m]` create 与 complete 都重新读取当前等级和 quota policy revision。
- [ ] `M06-QUOTA-04` `P0` `[45m]` 统一 quota_bytes_reserved/charged/released 计算，覆盖头像、Cover、封面、正文图和普通附件。
- [ ] `M06-QUOTA-05` `P0` `[30m]` 上传/替换/并发请求使用固定锁顺序，防止预留超卖和负数释放。
- [ ] `M06-QUOTA-06` `P1` `[45m]` 实现 Cover 上传、预览、替换、移除、默认背景和引用完整性检查。
- [ ] `M06-QUOTA-07` `P0` `[30m]` Cover 只能引用本人 ready 且安全处理通过的附件，移除只解除引用。
- [ ] `M06-QUOTA-08` `P0` `[45m]` S3 URL 到期只使 URL 失效，不改变 ready、对象生命周期、Cover 引用或容量。
- [ ] `M06-QUOTA-09` `P0` `[45m]` 删除进入 30 天保留；物理删除成功、无引用且对象校验通过后才释放容量。
- [ ] `M06-QUOTA-10` `P0` `[45m]` 测试升级、降级、处罚覆盖、并发替换、延迟清理、孤儿 mark-and-sweep 不误删在用文件。
- [ ] `M06-QUOTA-11` `P1` `[30m]` 完成 Profile Cover、附件和 Admin storage operation coverage。

## M06-DOWNLOAD：下载授权与积分抵扣

**元数据：** `P0` · `owner=unassigned/backend-download` · `risk=critical` · `depends=M06-QUOTA,M07-LEDGER` · `blocked=none`
**目标文件：** `backend/src/download/`、`backend/src/routes/download/`、`backend/tests/download/`、`docs/DOWNLOAD-BILLING.md`
**验收：** 策略优先级、免费授权、扣款原子性、URL 失败和 Range 复用测试通过。

- [ ] `M06-DOWNLOAD-01` `P0` `[30m]` 定义站点→板块→附件→等级→用户→Feature Flag 的下载策略优先级。
- [ ] `M06-DOWNLOAD-02` `P0` `[30m]` 后端计算 user/owner/amount/currency，拒绝请求体覆盖价格和身份。
- [ ] `M06-DOWNLOAD-03` `P0` `[30m]` 每次首次授权都创建 download authorization，免费价格也写授权而非绕过流程。
- [ ] `M06-DOWNLOAD-04` `P0` `[45m]` 扣款、point operation、授权、审计和 Outbox 在同一事务完成。
- [ ] `M06-DOWNLOAD-05` `P0` `[45m]` 有效授权重签 S3 URL 不重复扣款；URL TTL 与授权有效期独立。
- [ ] `M06-DOWNLOAD-06` `P0` `[30m]` URL 签发失败返回 `download_url_unavailable`，按幂等策略不重复扣款。
- [ ] `M06-DOWNLOAD-07` `P0` `[30m]` Range 请求共享同一授权，不按 Range、字节数或重试重复收费。
- [ ] `M06-DOWNLOAD-08` `P0` `[45m]` 未 ready、无附件权限、余额不足、封禁和策略停用不泄漏授权/对象信息。
- [ ] `M06-DOWNLOAD-09` `P0` `[45m]` 测试 SQLite/MySQL/MariaDB 账户锁竞争、故障注入和重复 Idempotency-Key。
- [ ] `M06-DOWNLOAD-10` `P1` `[30m]` 更新 Download Billing 与 Attachments operation coverage、指标和管理查询。

## M06-MIGRATION：本地与 S3 迁移/回滚

**元数据：** `P0` · `owner=unassigned/operations-storage` · `risk=critical` · `depends=M06-ADAPTER,M06-QUOTA` · `blocked=none`
**目标文件：** `backend/src/storage/migration/`、`docs/OPERATIONS.md`、`docs/STORAGE.md`、`backend/tests/storage/migration*`
**验收：** local→S3、S3→local 的 hash、权限、断点、切换、回滚和恢复演练通过。

- [ ] `M06-MIGRATION-01` `P0` `[45m]` 设计迁移 manifest：object key、size、hash、source revision、target status 和 cursor。
- [ ] `M06-MIGRATION-02` `P0` `[45m]` 实现只读校验/预演模式，不修改配置、不删除源对象。
- [ ] `M06-MIGRATION-03` `P0` `[45m]` 实现可断点续传的复制、hash 校验、引用统计和失败重试。
- [ ] `M06-MIGRATION-04` `P0` `[30m]` 切换前验证头像、Cover、公开/受限附件和 Range 读写冒烟。
- [ ] `M06-MIGRATION-05` `P0` `[30m]` 切换 backend 后验证 ready、上传处理、签名 URL、清理 Job 和配额数值。
- [ ] `M06-MIGRATION-06` `P0` `[45m]` 实现失败回滚到 source backend，禁止在未核对 hash 时删除源对象。
- [ ] `M06-MIGRATION-07` `P0` `[45m]` 测试对象缺失、权限错误、大小/hash 不符、部分完成和重复运行。
- [ ] `M06-MIGRATION-08` `P1` `[30m]` 编写维护窗口 Runbook、观察指标、停止条件和恢复点确认。

## M06-UI：上传、存储和下载前端/后台

**元数据：** `P1` · `owner=unassigned/frontend-storage` · `risk=high` · `depends=M06-QUOTA,M06-DOWNLOAD` · `blocked=none`
**目标文件：** `frontend/src/lib/upload/`、`frontend/src/routes/settings/`、`frontend/src/routes/admin/storage/`、`frontend/tests/`
**验收：** 上传失败恢复、容量提示、URL 重签和管理设置的 E2E/a11y 通过。

- [ ] `M06-UI-01` `P1` `[45m]` 实现文件选择、预签名上传进度、取消、重试和 complete 状态。
- [ ] `M06-UI-02` `P1` `[30m]` 显示当前等级单文件上限、总容量、reserved/charged 和安全处理状态。
- [ ] `M06-UI-03` `P1` `[30m]` Cover/头像/封面引用只能选择本人 ready 附件，提供安全预览。
- [ ] `M06-UI-04` `P1` `[30m]` S3 URL 过期时调用后端重签，不把旧 URL 当永久状态或删除附件。
- [ ] `M06-UI-05` `P1` `[45m]` 实现下载免费/扣费确认、余额不足、授权有效和 URL 失败状态。
- [ ] `M06-UI-06` `P1` `[45m]` 管理后台实现 local/S3 配置、path-style、TTL、测试连接和脱敏状态。
- [ ] `M06-UI-07` `P0` `[30m]` 后台修改 TTL 只影响新签发 URL，界面明确迁移需预演/hash/回滚。
- [ ] `M06-UI-08` `P1` `[45m]` 测试键盘、移动端、无 JS 表单退化、403/404/429/503 和敏感字段不进 DOM。

---

<a id="m7"></a>

# M7：不可变积分账本、等级、签到与内部商城

**完成定义：** B 币只能站内规则产生和消费；余额与流水原子一致；奖励反刷；商品权益、限时过期、补偿和装扮投影安全。

## M07-LEDGER：账本和账户内核

**元数据：** `P0` · `owner=unassigned/backend-economy` · `risk=critical` · `depends=M01-AUDIT,M01-DB` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/economy/ledger/`、`backend/tests/economy/ledger*`、`docs/MARKETPLACE-ACCOUNTING.md`
**验收：** 账本属性、并发、故障注入和三数据库锁语义通过。

- [ ] `M07-LEDGER-01` `P0` `[45m]` 新增 currencies、point_accounts、point_operations、point_transactions 和 balance snapshots。
- [ ] `M07-LEDGER-02` `P0` `[30m]` 将 delta、balance_after、reason、source、idempotency scope 和 policy version 设为不可变字段。
- [ ] `M07-LEDGER-03` `P0` `[45m]` 统一账户锁定顺序、SQLite `BEGIN IMMEDIATE` 和 MySQL/MariaDB 行锁适配。
- [ ] `M07-LEDGER-04` `P0` `[45m]` 实现 debit/credit/freeze/unfreeze/reversal/compensation domain commands。
- [ ] `M07-LEDGER-05` `P0` `[30m]` 禁止充值、提现、现金兑换、普通用户转账和现实价值承诺。
- [ ] `M07-LEDGER-06` `P0` `[45m]` 余额不足、负数、溢出、并发双扣和每步失败均完整回滚。
- [ ] `M07-LEDGER-07` `P0` `[30m]` 相同幂等 key 重放返回原 operation；不同摘要返回冲突。
- [ ] `M07-LEDGER-08` `P0` `[45m]` 以属性测试验证 `initial + Σ(delta) = balance` 和冻结余额恒等式。
- [ ] `M07-LEDGER-09` `P0` `[30m]` 管理员发放要求 reason、权限、recent-auth、审计和可配置双人复核。
- [ ] `M07-LEDGER-10` `P0` `[30m]` 奖励撤销/退款只写反向补偿流水，不更新/删除历史记录。
- [ ] `M07-LEDGER-11` `P0` `[45m]` 在三数据库运行同一账本 Fixture、锁竞争、死锁/超时重试和恢复测试。

## M07-LEVELS：等级、经验与自动签到

**元数据：** `P1` · `owner=unassigned/backend-economy` · `risk=high` · `depends=M07-LEDGER,M02-IDENTITY` · `blocked=none`
**目标文件：** `backend/src/economy/levels/`、`backend/src/economy/activity/`、`backend/tests/economy/activity*`
**验收：** 用户时区日界线和反刷测试通过，等级权益由服务端实时裁决。

- [ ] `M07-LEVELS-01` `P1` `[30m]` 定义经验来源、等级公式、缓存失效和等级权益版本。
- [ ] `M07-LEVELS-02` `P1` `[30m]` 实现等级读取/重建，缓存失效不改变账本和历史奖励。
- [ ] `M07-LEVELS-03` `P1` `[45m]` 将每日首次有效业务页面访问定义为签到事件，排除匿名、静态资源、预取、爬虫、健康检查和失败请求。
- [ ] `M07-LEVELS-04` `P1` `[30m]` 以用户时区计算日界线，缺省回退站点时区并记录时区版本。
- [ ] `M07-LEVELS-05` `P1` `[45m]` 实现签到幂等键、并发开页/刷新去重和每日奖励上限。
- [ ] `M07-LEVELS-06` `P1` `[45m]` 配置优质内容、有效点赞、回复、互动和活动奖励的延迟确认/撤销。
- [ ] `M07-LEVELS-07` `P0` `[45m]` 排除自赞、撤赞重赞、批量账号、对刷、异常 IP/设备和被处罚用户奖励。
- [ ] `M07-LEVELS-08` `P1` `[45m]` 测试 UTC 边界、夏令时、并发、重放、失败请求和反刷规则。
- [ ] `M07-LEVELS-09` `P1` `[30m]` 更新 Activity operation coverage、指标和管理员策略入口。

## M07-SHOP-SCHEMA：商城、订单与权益

**元数据：** `P1` · `owner=unassigned/backend-db-economy` · `risk=critical` · `depends=M07-LEDGER` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/shop/model*`、`docs/INTERNAL-MARKETPLACE.md`
**验收：** 商品版本、库存、订单、entitlement 和限时过期约束三数据库一致。

- [ ] `M07-SHOP-SCHEMA-01` `P1` `[45m]` 新增 products、price versions、inventory、sale windows、level gates 和 purchase limits。
- [ ] `M07-SHOP-SCHEMA-02` `P1` `[45m]` 新增 shop_orders、order_items、entitlements、equipment_slots 和 presentation projections。
- [ ] `M07-SHOP-SCHEMA-03` `P0` `[30m]` 商品 Token 只能引用后端枚举；拒绝 style、HTML、JS、远程 URL、SVG 和任意 CSS。
- [ ] `M07-SHOP-SCHEMA-04` `P1` `[30m]` 定义永久/限时权益、过期、撤销、重复购买和装备槽互斥状态。
- [ ] `M07-SHOP-SCHEMA-05` `P1` `[45m]` 测试价格版本、库存、限购、时钟边界、并发唯一性和三数据库迁移。
- [ ] `M07-SHOP-SCHEMA-06` `P1` `[30m]` 同步 Schema、状态机、错误码、事件目录和管理员配置矩阵。

## M07-SHOP：购买、装备、反应与补偿

**元数据：** `P1` · `owner=unassigned/backend-shop` · `risk=critical` · `depends=M07-SHOP-SCHEMA,M07-LEDGER` · `blocked=none`
**目标文件：** `backend/src/shop/`、`backend/src/reactions/`、`backend/src/routes/shop/`、`backend/tests/shop/`
**验收：** Shop/Activity 相关 operation 原子、幂等且不能改变权限/现金价值。

- [ ] `M07-SHOP-01` `P1` `[45m]` 服务端重算商品、价格、货币、库存、等级、销售窗口和限购，不信任请求体。
- [ ] `M07-SHOP-02` `P0` `[45m]` 购买同事务锁库存、扣余额、写订单/流水、发 entitlement、审计和 Outbox。
- [ ] `M07-SHOP-03` `P1` `[30m]` 实现永久/限时商品购买、重复 entitlement 和过期自动卸下。
- [ ] `M07-SHOP-04` `P0` `[30m]` 相同 Idempotency-Key 返回原订单；并发购买不得超卖或重复扣款。
- [ ] `M07-SHOP-05` `P1` `[45m]` 装备槽实现昵称颜色、头像挂件、边框、徽章（最多 3 个）、主页/帖子装饰和 Reaction 包。
- [ ] `M07-SHOP-06` `P0` `[30m]` PresentationProjection 只输出后端 Token，Reaction 不改变审核、排序、权限或现金价值。
- [ ] `M07-SHOP-07` `P1` `[30m]` 数字装扮默认不可退款；重复扣款、权益未发放和平台异常使用补偿流水。
- [ ] `M07-SHOP-08` `P1` `[45m]` 实现 Reaction 创建/删除、数量包消耗、通知偏好、目标权限和限流。
- [ ] `M07-SHOP-09` `P0` `[45m]` 测试 Token XSS/CSS/远程资源、并发库存、余额锁、过期、撤销和补偿。
- [ ] `M07-SHOP-10` `P1` `[30m]` 管理员配置商品、库存、活动、补偿和退款要求 reason、版本和审计。
- [ ] `M07-SHOP-11` `P1` `[30m]` 更新 Shop/Activity operation coverage 与前端类型。

## M07-UI：积分、商城、装扮与活动前端

**元数据：** `P1` · `owner=unassigned/frontend-economy` · `risk=high` · `depends=M07-LEVELS,M07-SHOP` · `blocked=none`
**目标文件：** `frontend/src/routes/shop/`、`frontend/src/routes/me/wardrobe/`、`frontend/src/routes/admin/shop/`、`frontend/tests/`
**验收：** 购买确认、衣柜、限时状态、签到和后台管理的 E2E/a11y 通过。

- [ ] `M07-UI-01` `P1` `[45m]` 实现余额、等级、经验和签到状态安全投影。
- [ ] `M07-UI-02` `P1` `[45m]` 实现商品列表、价格版本、库存、等级门槛、限购和有效期展示。
- [ ] `M07-UI-03` `P0` `[30m]` 购买确认页显示准确价格、余额变化、不可退款说明和失败恢复。
- [ ] `M07-UI-04` `P1` `[45m]` 实现订单结果、entitlement、重复请求和补偿待处理状态。
- [ ] `M07-UI-05` `P1` `[45m]` 实现衣柜、装备槽、徽章上限、过期自动卸下和装饰预览。
- [ ] `M07-UI-06` `P1` `[30m]` 支持用户关闭他人装饰、减少动效和隐私设置降级。
- [ ] `M07-UI-07` `P1` `[45m]` 实现 Reaction 选择、撤销、限流、目标权限和通知偏好。
- [ ] `M07-UI-08` `P1` `[45m]` 实现管理员商品/活动/库存/补偿页面，显示版本冲突和审计结果。
- [ ] `M07-UI-09` `P1` `[45m]` Playwright 和 axe 覆盖购买、衣柜、过期、移动端、键盘和无 JS 展示降级。

---

## M6-M7 出口门槛

- 本地/S3 Adapter 覆盖 AWS S3、MinIO、R2；virtual-host/path-style、multipart、Range 和故障矩阵均绿。
- S3 URL TTL、下载授权、对象生命周期和配额释放互相独立；URL 过期不删对象、不释放容量、不重复收费。
- 本地↔S3 迁移具备预演、hash、断点、切换、回滚和恢复证据。
- 账本恒等式、三数据库锁竞争、故障回滚、幂等和补偿全部通过。
- B 币没有充值、提现、现金兑换或普通转账路径；所有管理员调整均有审计。
- 商城 Token 不能执行任意 CSS/HTML/JS；永久/限时权益和异常补偿可恢复。
