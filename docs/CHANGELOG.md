## v1.0.0-rc.8 — 2026-09-16（装扮中心工坊、行内引用回复、审核版本对比、信任等级单轨与全站移动端适配）

> 基线 commit 待打 tag 时补记。rc.7 之后的关键增量：
> 1. 商城装扮中心与工坊系统（Cosmetic Studio & Wardrobe）：可配置装扮样式库（迁移 0079–0081）、Steam 动效头像框与背景资产、衣柜实时试穿与多槽位装备、后台装扮可视化设计工坊与一键发布货架；
> 2. 交互与内容体验：评论区楼中楼行内引用回复与光标精准保持、后台内容审核版本对比组件（GitDiffViewer）、富文本编辑器增强；
> 3. 移动端与全站体验：移动端 Bottom Sheet 浮层标准化（规范与手势关闭）、全站文案后台可配置化（GET /api/v1/site 与迁移 0065）、注册页全屏视觉体系重塑；
> 4. 平台与权限：LinuxDo 信任等级管理单轨（TL0–TL4）与规则后台可配置化、附件上传类型策略化（OOXML/音视频/文本族白名单）、记住我会话 60 天长效与 Passkey 契约对齐、MariaDB collation 修复。
> 发布顺序：后端迁移 → backend → worker → frontend。

### 商城装扮中心与工坊系统（M07-SHOP-STUDIO / 迁移 0079–0081）

- **数据与端点**：
  - 新增迁移 0079/0080 `cosmetic_defs`（可配置装扮样式库，昵称渐变色标/呼吸光、头像框线宽/光晕/阴影/动效参数 schema 校验）与 0081 `steam_catalog`（Steam 动效头像框与背景目录镜像）。
  - 后端新增装扮工坊端点：`POST /api/v1/admin/shop/studio/publish`（工坊作品一键上架商城）、`GET /api/v1/shop/cosmetics`（全量装扮样式查询）、`POST /api/v1/admin/shop/steam-catalog/sync`（Steam 目录同步）与 `GET /api/v1/steam-assets/{kind}/{file}`（静态媒体资产服务）。
- **个人衣柜（/me/wardrobe）**：
  - 支持头像框、昵称特效、个人主页全景特效、徽章多槽位装备与实时预览切换。
  - 接入 Steam 动效头像框循环播放与高质量背景渲染，服务端与无 JS 状态兼容回退。
- **后台装扮工坊（/admin/shop/studio）**：
  - 提供可视化装扮设计器：多色标线性渐变调节器、呼吸光强度/周期调节、头像框内外发光与脉冲微调，支持本地草稿保存与一键上架。

### 评论区行内引用回复与富文本编辑（Inline Quote Reply & Editor）

- **行内引用回复**：
  - 新增 `inline_reply.rs` 与 `caret.ts`，评论区支持楼中楼精准 `@` 用户提及与行内引用特定楼层。
  - 编辑器光标定位与选区精准恢复，避免回复插入打断输入流畅度。
- **内容审核版本比对（/admin/content）**：
  - 集成 `GitDiffViewer.svelte`，在管理后台直接比对帖子/评论历史修订版本的高亮 Diff。

### 手机端浮层标准化（Bottom Sheet 交互规范）

> 确立「手机端弹层 = 底部上滑约半屏（50dvh）Bottom Sheet」为全站标准：scrim +
> 抓手 + 72px 阈值下滑关闭 + 底部安全区 + aria-modal；桌面 ≥768px 保持锚定浮层不变。
> 规范：[`MOBILE-SHEET.md`](MOBILE-SHEET.md)（已登记 DOCUMENT-STATUS）。

- 共享实现：`mobile.css` 末段「§ 手机端半屏弹层」样式层（`.app-sheet-backdrop`/`.app-sheet-grab`
  共享类 + 各浮层 sheet 化规则）+ `lib/utils/sheet-drag.ts` 下滑关闭手势 action
  （断点常量 `SHEET_MEDIA_QUERY` 为 CSS/JS 唯一事实来源；`utils/sheet-drag.test.ts` 单测）。
- 接入浮层：发帖页「发布设置 / 视频引用 / AI 助手」3 个面板（`TopicComposer`）、
  「收到的表情」明细弹窗与「添加互动表态」选择器（`ReactionBar`）、
  用户资料卡窄屏形态由浮动卡对齐为全宽贴底 + scrim（`UserCard`）；
  导航用户菜单 Bottom Sheet（既有）作为设计参照。
- 导航收敛：移动端抽屉菜单移除个人链接区（我的主页/我的等级/积分明细/我的收藏/
  发布内容/账号设置/通知，与右侧头像用户菜单重复），抽屉仅保留退出登录；
  个人相关入口统一由头像菜单（Bottom Sheet）提供。

### 复核修正（M18 后端批量/导出未完成与安全审计待修复项）

> 二次复核撤销上一轮对 M18-ADMIN-BATCH-01/02 的完成结论：当前代码只有前端 server action 逐条调用单条端点，`ExportButton` 只导出当前已加载数据；后端原子批量、全量流式 CSV、权限/审计端点尚未完成。
>
> 同轮审计确认并进入修复：幂等重放跨用户 IDOR、附件状态重签下载、S3 私有重定向缓存、个人/通知/下载响应缓存隔离、板块角色全局授予、board_mute 与 fail-open sanction lookup、管理员 step-up 及上传隔离对象清理等问题。

### 产品决策（取消 M19 设计系统重做，路线图收敛）

> 经产品所有者确认，当前默认主题视觉与组件体验符合需求，取消 M19「冷墨」全站三层表面重做与 Panel 组件重构。

- 执行册收敛：M19（5 工作包、42 叶子任务）正式下线并归档至 `todo/archived-M19-design-system.md`；路线图校验器与仪表盘收敛至 M0–M18。
- 任务总数由 865 调整为 823 项；该条记录保留当时快照，当前状态以 `TODO.md` 为准。

### 后台表格化与批量盘点收口（levels 配额批量 + 管理矩阵）

> 全量盘点 27 个 admin 页面：列表型实体均已是表格形态 + 约定 B 批量
> （14 页此前已接通：users/posts/attachments/notifications/boards/tags/
> plugins/marketplace/oauth/achievements/ai/assignments/moderation-cases/
> shop）；本轮补齐 /admin/levels 配额批量并产出管理矩阵文档
> [`ADMIN-MANAGEMENT-MATRIX.md`](ADMIN-MANAGEMENT-MATRIX.md)（每页形态/
> 批量/端点/类型判定；只读流水、配置单例、主题预览卡等按类型注明不批量
> 理由）。

- /admin/levels 附件配额补约定 B：行复选框 + 全选 + `BatchBar` +
  「批量设置配额」Dialog（`?/batchQuota`）。服务端逐档 PATCH（每档各自
  If-Match=打开时快照的 policy_version + reason 审计 + step-up）；
  **单档 409 冲突不阻塞其余档**，返回 207 按档汇总（成功 TLx / 冲突 TLy）。
  批量均为前端循环调用既有单条端点（同 M18-ADMIN-BATCH 先例），无新增契约端点。
- TL 规则不做批量（各级语义互异），逐级编辑/reset 保留行「⋮」菜单入口。
- 证据：`npm run check` 0 错误 0 警告；SSR 用例补批量断言后 16/16；
  vitest 全量 821/821；`cargo clippy --workspace` 0 警告。

### 等级规则可配置化（TL 规则后台编辑 + 停用语义修复）

> 承接等级合并单轨：/admin/levels 的信任等级规则从只读种子升级为管理端
> 可配置（名称/摘要/启用/全部阈值条件），并修复「停用实际不生效」的语义漏洞。

- 后端新增 `PATCH /api/v1/admin/trust-levels/{level}`（level.manage + CSRF +
  If-Match=version 乐观锁 + reason 审计 `admin.trust_level_rules.update`）：
  全量更新 name（1–50 字）/summary（≤200 字）/is_enabled/requirements_json。
  requirements 校验：未知键拒绝（防拼写错误静默失效）、负数/越界拒绝
  （ratio ∈ [0,1]、window_days ≤ 3650）、TL0 禁止条件、TL1–TL3 禁
  manual_only、TL4 强制 manual_only=true。新增
  `POST /api/v1/admin/trust-levels/{level}/reset`（reason 审计
  `admin.trust_level_rules.reset`）恢复内置 LinuxDo 默认（含 name/summary）。
- 停用语义修复：`load_rules` 原只取 is_enabled=1 行，而 parse_rules 对缺失
  等级回填代码默认值——停用某级后引擎实际仍按默认规则评估。现改为全量行
  加载 + 引擎感知：停用级不参与自动晋升目标选择；TL3 停用时不做自动降级
  （存量用户保持不动）；手动授予不受停用影响；/me/trust-level 的
  next_level 跳过停用级（不展示不可达条件）。管理列表随之可见停用行。
- 前端 /admin/levels：每行「⋮」→「编辑规则」弹层（名称/摘要/启用开关 +
  逐条阈值条件增删改：比例按 % 输入、TL0 空条件提示、TL4 manual_only 固定；
  「保存规则」= PATCH，「恢复默认」= reset，共用 reason 审计字段），保存后
  invalidateAll 即时刷新。
- 登记与测试：route-coverage 增 `admin/trust-levels/{p}` PATCH 与
  `admin/trust-levels/{p}/reset` POST；trust_levels 集成测试新增 3 例
  （PATCH 鉴权/校验/冲突/审计矩阵、编辑对评估引擎双向生效、reset 恢复默认
  与 TL3 停用不降级）。
- 文档同步：TRUST-LEVELS §3/§4/§5/§5.1、SCHEMA §9（version/is_enabled 语义）、
  API/OPERATIONS 端点登记。
- 证据：`cargo test --test trust_levels` 10/10；`cargo check`/`clippy
  --workspace` 0 警告；`npm run check`（svelte-check）0 错误 0 警告
  （顺手修 RowActionsMenu role=menu 缺 tabindex 的 a11y 警告）；
  `ruby scripts/check-route-coverage.rb` OK。

### 等级管理合并单轨（/admin/levels = LinuxDo 信任等级）

> 产品决策：等级体系统一为 LinuxDo 式信任等级（TL0–TL4），后台单入口。
> 体系对照见 `TRUST-LEVELS.md` §1.1；经验方案表（0050）保留为契约字段
> （`PublicUser.level`/`visibility_level`/商城 `required_level`）的内部同步来源。

- 后端移除等级存档/方案端点：`GET /api/v1/admin/levels`（0062 `level_rules`
  存档 CRUD，运行时无消费点）、`PATCH /api/v1/admin/levels/{level}`、
  `GET /api/v1/admin/levels/scheme`（0050 方案表投影）及对应处理器、辅助函数
  与集成测试；`scripts/check-route-coverage.rb` 同步注销（双向校验通过，
  后端路由 224 条）。0062 表保留为历史存档（不新增迁移）。
- 附件配额档位键切换：`users.level`（经验等级 1–10）→ `users.trust_level`
  （TL0–4）。`upload.rs`（create/complete 重检）、`storage.rs`
  （GET /attachments quota 摘要）、`quota.rs`（`retention_days_for_owner`）
  全部改读信任等级；`default_policy_for_level` 五档化（TL0 2MB/100MB/20MB →
  TL4 20MB/2GB/400MB，单调放宽、TL3–TL4 单文件 20MB 平台期），
  `GET/PATCH /api/v1/admin/levels/{id}/attachment-quota`（冻结契约）端点
  形状不变、`{id}` 语义改为 TL。
- 前端 /admin/levels 重写为信任等级单轨页：TL0–TL4 规则/用户数表
  （`GET /admin/trust-levels`）+ 手动设置信任等级卡（`?/setLevel`，TL4 唯一
  授予通道）+ 按信任等级取档的附件配额（`?/updateQuota`，If-Match + reason
  审计 + step-up）。原 /admin/trust-levels 独立路由删除、导航合一；
  client.ts/types.ts 移除存档 CRUD 死函数与 `AdminLevelItem`。
- 徽章主题安全化：晋升条件/停用/「5 级」徽章原用主题未定义的
  `--color-surface-2`/`--color-info-soft`（回退浅色底 + `color:inherit`，
  暗色主题下文字不可读），改用主题原生 `sbadge sb-gray`/`sb-brand`/
  `sb-danger`；等级列加 nowrap 修「TL0 新用户」折行。Playwright 对运行中
  dev 栈实测暗/浅两主题截图目检通过（临时验证脚本与铸造会话用后即删）。
- 文档同步：`TRUST-LEVELS.md` §1.1/§5.1（合并单轨后体系定位与后台入口）、
  `SCHEMA.md` §9（level_rules 端点下线、配额档位键）、`API.md`（端点登记与
  quota 口径）、`OPERATIONS.md` §19.8、`STORAGE.md` §8.2、
  `PROTOTYPE-IA.md` §1.4/§2.10。
- 证据：`cargo test --test economy_ext --test storage_quota --test
  storage_upload --test trust_levels` 全通过（含新增
  `default_policy_tiers_follow_trust_levels` 五档互异/单调用例；
  policy-follows 测试改写信任等级）；`cargo clippy --workspace`、
  `cargo check` 0 警告；`npm run check`（svelte-check）0 错误；SSR 空安全
  测试改写至新数据形状后 vitest 21/21 通过；`ruby
  scripts/check-route-coverage.rb` OK。

### 消息页移动端适配（实测校准 + 禁用缩放）

> 纯前端布局修复，无契约/后端变更；经 Playwright 注入测试会话在
> 390×844 / 436×948 实测校准（测量用临时脚本/截图/临时会话行已清理）。

- /messages 移动端（≤767px）撑满视口：页面根覆写主题遗留的
  `#page-messages.app-page { height: calc(100dvh - 64px) }`（旧 chrome 口径，
  实测多出 46px 造成整页滚动）与 `display: block !important`
  （chinese-elegance 行 1210；级联中 !important 无视优先级，display 需同加
  `!important` 才能取胜），改为 `height: calc(100dvh - 110px - safe-area)` +
  `display: flex`（110 = 实测顶栏 52 + 底部 Tab 58）。实测 `scrollH = dvh`、
  卡片底贴 Tab 顶（gap = 0），空态（还没有私信 / 会话还没有消息）在卡片内
  垂直居中。桌面双栏布局不变。
- 断点对齐：组件移动分支由 ≤768 收敛为 ≤767（与 `styles/mobile.css` 的消息页
  owning 规则一致，消除 768px 宽度的双方都不管缝隙）；删除与 mobile.css 竞争
  的线程样式——线程视图继续由 mobile.css 管理（单一滚动宿主、输入框安全区、
  16px 字号防 iOS 缩放）。
- 移动端禁用缩放：`app.html` viewport meta 增加
  `maximum-scale=1, user-scalable=no, viewport-fit=cover`（站点统一决策；
  刘海屏 safe-area 变量随之生效）。
- 证据：`npm run check`（svelte-check）0 错误；`vitest` 全量 801/801；
  Playwright 实测 `scrollable=false`、`gap_card_to_nav=0`（截图目检通过）。

### 管理后台工具栏统一单行布局

> 纯前端布局修复，无契约/后端变更。

- 管理后台 7 个列表页（posts / attachments / boards / plugins / marketplace / notifications / audit）的「搜索 + 状态筛选 + 清除」工具条由两行全宽堆叠（`.app-field`/`.app-select` 默认 `width:100%`，`space-between` 行把「清除」挤到右缘裁切换行）统一为单行 flex：搜索框 `flex:1` 撑满、筛选下拉定宽 168px、「清除」为 `btn ghost sm`；窄屏自动换行。posts/attachments 的「清除」由裸文本链接升级为按钮样式（保留 no-JS GET 重置语义）。
- 证据：`npm run check`（svelte-check）0 错误；`vitest` 全量 801/801 通过。

### 等级管理对齐（/admin/levels 改读真实经验方案阶梯）

> 背景：/admin/levels 页面原展示 0062 `level_rules` 存档表（运行时无消费点，
> 接口失败还回退硬编码 mock），与真实等级体系（经验方案 10 级 + 信任等级
> TL0–TL4）不一致。本轮按 docs/TRUST-LEVELS.md §1.1 的体系定位对齐。

- 后端新增 `GET /api/v1/admin/levels/scheme`（documented non-contract，
  `level.manage`，`private, no-store`）：活跃 `level_schemes` 方案 + `levels`
  等级行（level=sort_order，与 `users.level` 同语义）+ `users.level` 每级
  用户数（GROUP BY 聚合，status='active' 未删除）；无活跃方案返回
  `scheme:null` 空态而非 404。登记 `scripts/check-route-coverage.rb`。
- 前端 /admin/levels 重写：等级列表改为真实经验方案阶梯（等级/阈值/用户数/
  每级附件配额摘要），删除 `level_rules` 存档展示、硬编码 mock 回退与死
  `update` action；附件空间配额（If-Match + reason 审计 + step-up）保持不变，
  等级档位改由方案阶梯驱动；页面增加指向 /admin/trust-levels 的引导。
- 文档同步：TRUST-LEVELS.md §1.1（三张等级表角色对照）、SCHEMA.md §9（补记
  0062 `level_rules` 与新端点）、API.md / OPERATIONS.md（non-contract 登记 +
  存档状态说明）、PROTOTYPE-IA.md §1.4/§1.5/§2.10（实现差异）。
- 证据：`cargo test --test economy_ext admin_levels_scheme`（新增集成测试：
  403 矩阵、空方案空态、默认方案 10 级阶梯与每级用户数）通过；
  `npm run check`（svelte-check）0 错误；SSR 空安全测试更新至新数据形状。
  余项：`cargo clippy --workspace`、`cargo check`、vitest 全量见 TODO 收口。

### 文档对齐（等级管理以信任等级为准）

> 纯文档修订（同日并入上一条的代码对齐）；保留原文档盘点结论。

- `TRUST-LEVELS.md` 新增 §1.1「体系定位」：三张等级表运行时角色对照——`trust_level_rules`（0070，等级管理主线，/admin/trust-levels）、`level_schemes`+`levels`（0050 经验等级权威，同步 `users.level`）、`level_rules`（0062，管理端存档，运行时无消费点）。
- `SCHEMA.md` §9 补记 0062 `level_rules` 表（结构、种子、仅 `/admin/levels` CRUD、阈值/限额运行时无消费点）。
- `API.md` / `OPERATIONS.md` 为 `GET/PATCH /api/v1/admin/levels` 标注「管理端存档 CRUD」状态说明。
- `PROTOTYPE-IA.md` §1.4/§1.5/§2.10 补实现差异：/admin/levels 已实现形态（规则存档列表 + 每级附件配额）、原型的晋升路径可视化未实现、新增 /admin/trust-levels（LinuxDo 式 TL0–TL4，等级管理主线）。

### 附件上传类型策略化（M06-UPLOAD）

> 工作区收口记录，尚未对应 release tag；发布状态以 [`../TODO.md`](../TODO.md) 为准。

- 附件上传类型不再写死：`site_settings` 新增 `storage_allowed_upload_types`（迁移 0068，三方言），管理后台「文件存储」按类目 `image/pdf/text/office/av` 开关，PATCH `/api/v1/admin/storage/config` 保存后立即生效（create 实时读库）；能力白名单仍是安全下限，压缩包/可执行/宏文档始终拒绝。
- 能力白名单在图片/PDF/纯文本基础上扩展：OOXML 文档（docx/xlsx/pptx，含 OPC 包结构校验防 zip 改名绕过）、文本族（csv/json/xml/markdown）与音视频（mp4/m4v/mp3/webm，ftyp/ID3/EBML 魔法检测）。
- `GET /api/v1/attachments` 的 `quota` 摘要新增 `allowed_media_types` 投影；前端共享 `lib/upload/mediaTypes.ts`（扩展名权威归一化 + 策略缓存），编辑器与上传器选文件即预校验，不再把任意文件送到后端撞「存储请求参数有误」；该报错文案同步改为明确指出类型/大小不符。
- 证据：`cargo test --test storage_upload`（20 pass，含策略 fail-open/OOXML/音视频魔法用例）、`cargo test --test storage_quota`（13 pass）、`cargo clippy --workspace` 0 警告、`vitest` upload 相关 24 pass、`svelte-check` 0 错误；E2E 实测 docx/mp4/csv 经 S3 预签名上传 ready，管理端关闭 office 类目后 docx create 即刻 400、恢复后放行。

### 私信发布级验收与内容审核修订（M17-GAPFIX-07 / M18-ADMIN-CONTENT-01）

> 工作区收口记录，尚未对应 release tag；发布状态以 [`../TODO.md`](../TODO.md) 为准。

- 私信完成发布级验收：SQLite、MySQL 8、MariaDB 10.11 跨库语义，CSRF、参与者隔离、幂等重放/冲突、复合游标分页、`private, no-store`、通知和隐私投影均有证据；`/messages` 与用户主页发私信入口通过 SSR/无 JS 和移动端对比验证。
- 5 个 Messages operation 从 `implemented` 升为 `verified`；当前 OpenAPI 覆盖为 223/223，`verified=199`、`implemented=24`。
- 清理已完成的 `prototype/todo.md` 与旧 `prototype/.verify/` 生成快照；正式对比报告和历史 RC 审计资料保留。
- 内容审核页恢复可用（M18-ADMIN-CONTENT-01）：新增 `GET /api/v1/admin/posts/{id}/revisions`（管理域 documented non-contract，post.moderate，不限帖子状态）投影全量不可变修订；`/admin/content` 改为待审队列 + 最后两版「修改前/后」对比块，对比可用才放行通过/驳回（驳回理由改原生 details 展开，无 JS 可用），并修复 403/错误态误显示“没有待审核的内容”空态的问题。

### 全站文案统一（迁移 0065 / 公开端点 / 管理台）

- **数据与端点**：`site_settings` 新增 7 个站点文案列（`site_description`、
  `login_eyebrow/login_title/login_subtitle`、`register_eyebrow/register_title/
  register_subtitle`，空串 = 前端内置通用文案兜底）；`GET/PATCH
  /api/v1/admin/settings` 支持这些字段（长度上限校验 + 审计 + If-Match 乐观锁）；
  新增匿名可读公开投影 `GET /api/v1/site`（documented non-contract，
  `private, no-store`，不含 SMTP/注册开关等运营字段）。
- **管理台**：系统设置页新增「站点文案（登录 / 注册页）」分区，7 个字段
  纳入脏标记 / 恢复默认 / 导出配置 / 保存链路。
- **前台**：根 layout SSR 并行拉取站点公开信息并注入 `data.site`
  （`lib/site/copy.ts` 统一解析与兜底）；新增 `PageTitle` 组件统一
  「页面名 — 站点名」标题格式，全站 31 处硬编码 `— BBLBB` 标题与
  Navbar/登录/注册/首页品牌位全部改为动态渲染；Seo 组件标题后缀与
  `og:site_name` 兜底接入站点名。

## v1.0.0-rc.7 — 2026-09-07（主题管理全局实时预览、官方预置与后台管理设置功能完善）

> 基线 commit `5380a86`（feat/prototype-pages）。rc.6 之后的增量：完善主题管理全局实时预览、UI 组件库效果展示、4 套官方预置主题包一键安装，打通前后端 Token 投影与管理 CRUD；补齐系统设置 SMTP 密码加密存储与后端邮件投递，完善社交原型页面与加载失败状态。发布顺序：后端迁移 → backend → worker → frontend。

### 主题管理与 Token 实时预览（M13-THEME / M18-ADMIN-THEMES）

- **全站实时预览与效果展示**：
  - 将 M13 封闭 Token 体系（`--bb-*`）与全局基础语义变量（`--color-bg-page`、`--color-bg-card`、`--color-text-primary`、`--color-brand`、`--color-border` 等）深度绑定，支持即时预览与无缝复原。
  - 新增悬浮全局预览条与「组件库效果展示 Showcase」弹窗，真实预览导航、帖子卡片、按钮变体、表单控件及代码排版。
- **官方预置主题包与真实数据**：
  - 根除硬编码静态原型假数据，系统初始为空时自动补充系统内置默认主题兜底（`/default`）。
  - 内置 4 套官方高质感数据型主题包（暗夜极光 `/midnight`、复古羊皮纸 `/paper`、翡翠森林 `/forest`、赛博霓虹 `/cyberpunk`），支持一键安装至数据库。
- **管理端 CRUD 交互完善**：
  - 设为默认二次确认（强制要求审计原因，激活主题）；
  - 14 项 Token 可视化编辑器（颜色取色器、字体、圆角、密度选择与实时微缩预览，支持切换 JSON 源码模式，带 `If-Match` 乐观锁）；
  - 补齐主题安全删除（受系统默认保护）。

### 系统管理与后台增强

- **系统设置与 SMTP 持久化**：新增迁移 0064，支持系统级 SMTP 配置脱敏查询、PATCH 乐观锁更新，密码安全隔离存储；邮件服务与后台任务接入数据库配置。
- **存储与 AI 管理端点**：完善存储用量及脱敏密钥管理配置与 SSR 契约。
- **前端错误处理与空态**：新增 `LoadFailureState` 与统一 `problem-toast` 反馈，完善个人主页与社交互动原型页面。

## v1.0.0-rc.6 — 2026-09-07（CI 管线修复：runner 构建环境、clippy 1.98 适配与磁盘占用限制）

> 基线 commit `16b4e59`（main）。rc.5 之后的增量：**无任何运行时行为
> 变化**——无新增迁移（仍为 `1..63`）、无契约变化（223 operations
> 不变）、无前端变更；本次交付全部是 CI 管线与构建配置修复。目标：
> 结束自 PR #8 起 CI 全红的状态，让 main 首次通过完整 CI（已达成，
> main run 34084293347 全 6 job 绿）。无独立发布要求；若出包，随 rc.5/
> rc.3 内容一并沿用既有顺序（后端迁移 → backend → worker → frontend）。

### CI 构建环境（PR #14）

- `backend/.cargo/config.toml` 按 AGENTS.md §3.1 把 target-dir 固定为
  开发机路径 `/data/cargo-target/bblbb`，GitHub runner 无权创建 `/data`
  → `Permission denied`。ci / nightly / release-rc 增加 workflow 级
  `CARGO_TARGET_DIR=${{ github.workspace }}/target`（Cargo 环境变量
  优先于 config 文件，已验证）+ 各 cargo job 的 target-dir 缓存
  （key 覆盖 `Cargo.lock`）；既有 sccache / registry / git 缓存步骤
  全部保留（AGENTS.md §3.4），本地开发行为不受影响。
- 跨库迁移测试（MySQL 8 / MariaDB 10.11）：workflow 用 `mysql` CLI
  直灌迁移 SQL（不写 `schema_migrations` 状态表），测试二进制再用
  应用内置 `run_migrations`（按状态表判断 pending）重复建表 →
  `1050 Table 'users' already exists`。新增「Reset schema」步骤
  （skeleton 校验后 `DROP/CREATE SCHEMA bblbb`），首个测试二进制
  自建 schema 并写入状态表，后续二进制幂等跳过（6 个测试二进制均
  已逐一核对为幂等模式）。
- `cargo fmt --check` 既有违规 3 文件 6 处（economy/activity/
  service.rs、plugins/mod.rs、routes/auth.rs）——纯换行修复。

### clippy 1.98 适配（PR #14 / #15）

CI 的 `dtolnay/rust-toolchain@stable` 漂移到 1.98.0（本地 1.97.1），
`-D warnings` 下暴露既有代码问题：

- `result_large_err` ×347：AppError（axum 统一错误枚举，含 sqlx
  错误等大值变体）≥160B。逐点 Box 化需改全部路由签名，crate 级
  `#![allow(clippy::result_large_err)]`（`lib.rs`，注释说明理由；
  1.97 下该 allow 为无害冗余，两种工具链一致）。
- `inconsistent_digit_grouping` ×6：`3600_000`（4|3 分组）→
  `3_600_000`（economy/activity/service.rs ×4、tests/economy_ext.rs
  ×2）。
- `unused_variables` ×2：tests/economy_ext.rs 预置标签 `tag_a/
  tag_b` → `_tag_a/_tag_b`（6e0e0a1 引入；本地 check 目标不含
  `--all-targets` 故从未暴露）。
- `bool_assert_comparison` ×7：tests/admin_ext.rs 的
  `assert_eq!(bool, true/false)` → `assert!`/`assert!(!)`（保留
  自定义消息；本地实跑 10 passed 验证语义不变）。

### runner 磁盘占用限制（PR #15）

`cargo test --workspace --all-features` 全量 codegen 148 个测试可
执行文件（每个静态链接整棵依赖树，实测 ~175MB/个、合计 ~33GB），
叠加 clippy rmeta/rlib 与 sccache 默认 10GiB 缓存后超出 runner
14GB 磁盘。三处配合（均加性，不删任何缓存步骤）：

- `backend/Cargo.toml` `[profile.test] strip = "debuginfo"`：测试
  可执行文件缩到 ~1/2-1/3（backtrace 符号保留；仅影响 `cargo
  test`，release 构建与本地开发不受影响）。
- ci.yml rust job：Check 步骤拆为 fmt+clippy 与测试两步；测试按
  `cargo metadata` 的 target 清单分 4 批（40/批）运行，每批结束按
  target 名精确删除本批可执行文件（rlib/rmeta 共享保留、.so 不碰），
  lib/bins 单元测试单独跑。
- `SCCACHE_CACHE_SIZE: 4GiB`（ci rust job + nightly fault-injection）。

### 验证

- **main CI run 34084293347 全 6 job 绿**（Rust checks 20m20s：
  fmt → clippy 1.98 → 分 4 批 148 个测试 target 全 passed；MySQL 8 /
  MariaDB 10.11 / SQLite 迁移、前端、契约全过）——CI 自 PR #8 起
  首次全绿
- PR #15 run 34083003294（commit 9c7a940）全 6 job 绿
- 本地 `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` exit 0；`cargo test --workspace --all-features` exit 0
  （160+ target 全过）；`cargo fmt --check` 干净
- 本地 chunk 机制演练：3 target/2 批全 passed，批后可执行文件确认
  删除；ci/nightly YAML 解析通过；Cargo.toml `cargo metadata` 有效

---

## v1.0.0-rc.5 — 2026-09-07（MFA 注册二维码与 /me 页排版重构）

> 基线 commit `f7fbe8c`（feat/prototype-pages）。rc.4（含其文档 + 测试
> 交付）之后的增量：纯前端变更，无新增迁移（仍为 `1..63`）、无契约变化
> （223 operations 不变）、无后端变更；发布顺序仅前端（`frontend` 重新
> 构建部署即可）。

### M18-MFA-01 验收缺口修复：注册二维码

- TOTP 注册流程原先只展示 Base32 密钥与 otpauth 链接，与验收标准
  「二维码（TOTP secret）」不符。现由服务端（`+page.server.ts`）从后端
  返回的 `otpauth_uri` 生成二维码 SVG data URL（新增依赖 `qrcode`，走
  `lib/browser` 纯字符串渲染，无 canvas 依赖），页面以 `<img>` 渲染——
  不使用 `{@html}`（M04-MARKDOWN-08 HTML sink 政策，
  `check-html-sinks.rb` 保持通过），SSR/无 JS 基线即渲染二维码，
  手机可直接扫码。
- 注册流程改为两步结构：① 认证器扫描二维码 ② 输入 6 位动态验证码
  确认；密钥录入降级为 `<details>` 折叠项（可选中 secret + otpauth
  链接），二维码生成失败（返回 null）自动显示手工录入提示。
- `/mfa` 独立页与 `/me` 的 MFA 卡同步落地；新增
  `frontend/src/lib/mfa/otpauth-qr.ts`（含单测）与
  `frontend/src/types/qrcode-browser.d.ts`（qrcode 最小类型声明）。

### /me 页排版重构

- 账号卡下方新增 app-toolbar 快捷导航（对齐原型 me.html）：两步验证
  （`/mfa`）、登录设备（页内锚点 `#sessions`）、通知设置、OAuth
  授权；安全卡增加 `scroll-margin-top` 锚点滚定位。
- 主栏归组：账号信息 + 登录设备管理 + 两步验证（MFA）；侧栏保留账户
  卡 + 快捷操作 + 快捷入口。
- 快捷入口图标化（2×3 图标网格，原 `icon` 字段定义了但未渲染）；
  快捷操作去掉与「编辑资料」重复的「账号设置」按钮；操作错误提示从
  设备卡/MFA 卡两处收敛为页顶展示一次。

### 验证

- 前端 `npx vitest run` → 92 文件 616 用例全过（新增
  `otpauth-qr.test.ts`、`mfa-nojs.test.ts`，更新 `me-nojs.test.ts` /
  `action.test.ts` 断言二维码输出与降级）
- `npm run check`（svelte-check）→ 0 错误
- `ruby scripts/check-html-sinks.rb` → 通过（新代码无 `{@html}`）
- `npm run build` → 通过（SSR 包正确外部化 `qrcode/lib/browser`）

---

## v1.0.0-rc.4 — 2026-09-05（插件编写指南与文档↔代码一致性测试）

> 基线 commit `d234486`（feat/prototype-pages）。rc.3 之后的增量：纯文档 +
> 测试交付，无新增迁移（仍为 `1..63`）、无契约变化（223 operations 不变）、
> 无运行时行为变化。

### M13-PLUGIN 文档面：插件编写指南

- 新增 `docs/PLUGIN-AUTHORING.md`（434 行）：v1 配置型插件编写指南——
  manifest 字段权威参考（必填/约束/错误码对照、`supports` 语法、9 项
  capability 与 10 项订阅事件白名单及事件源现状、危险内容模式清单）、
  settings_schema 封闭子集与校验时机表、3 个完整可安装范例（互动感谢 /
  积分奖励 / 多订阅枚举）、管理 API 全流程 curl（CSRF / If-Match /
  reason / step-up）、调用摘要六标签与错误码表。
- §0.3 如实标注实现状态：管理面（安装/设置/启停/卸载/审计）已上线，
  事件→动作执行面待接线；范例全部基于已发射事件
  （`reaction.created.v1` / `points.operation_completed.v1`），不使用
  尚无事件源的白名单事件。
- `backend/src/plugins/mod.rs` 新增 `authoring_guide_examples_stay_installable`
  测试：机械提取指南全部 JSON 范例过真实校验器（manifest →
  `parse_plugin_package`，settings → 封闭 schema + 危险内容扫描），
  文档范例漂移即 CI 失败。
- 索引登记：README 文档表、`docs/DOCUMENT-STATUS.md` 状态表、
  `PLUGIN.md` 顶部指引。

### 验证

- `cargo test --lib plugins::` → 8 passed（含新一致性测试）
- `cargo clippy --lib` → 0 警告
- `make check-secrets` / `make check-docs`（lychee 未安装按规则跳过）→ 通过

---

## v1.0.0-rc.3 — 2026-09-05（M17-GAPFIX 社交/经济/管理域 + M18 原型功能对齐）

> 基线 commit `6e0e0a1`（feat/prototype-pages）。rc.2 之后新增 30 个契约
> operation（193 → 223），全部为兼容新增；旧客户端向后兼容
> （`check-client-compat.rb` frozen=193 / current=223 全绿）。迁移 `1..63`，
> 其中 0060-0063 为纯增量（建表 / 加列 / 插入种子），可逆。

### M17-GAPFIX 社交、经济与管理域

- 社交域：关注（用户/板块）、帖子收藏、双向私信（参与者门 + 已读状态）、
  成就目录 / 本人视图 / 徽章装备（3 槽上限）、个人 API 密钥（SHA-256 存储、
  明文仅创建时返回一次、scopes 白名单）；迁移 `0060_social`。
- 管理域（documented non-contract，`OPERATIONS.md §19.8`）：站点统计 / BI
  指标 / 审计读取 / 系统设置（If-Match）/ 帖子管理动作 / 通知广播与召回 /
  角色分配 / 成就管理 / 积分流水与调整 / 等级规则 / 附件管理 / 下载交易 /
  标签合并；迁移 `0061_admin_ext`。
- 经济与个人域：本人积分流水 / 处罚记录 / OAuth 授权管理（撤销）、修改密码
  （吊销其他 Session）、付费内容解锁（同事务扣款 + grant + 通知）、
  `GET /api/v1/stats` 公开站点统计；迁移 `0062_economy_ext`、
  `0063_admin_settings_public_source`。
- 契约与覆盖：OpenAPI 172 paths / **223 operations 全实现**；覆盖登记
  `not_started` 清零（194 verified + 29 implemented，
  `todo/OPENAPI-COVERAGE.md`）。

### M18 原型功能对齐

- 前端补齐全站页面：发现 / 私信 / 收藏 / 成就 / API 密钥 / MFA / 标签聚合 /
  市场与结账 / 账单 / 管理后台（BI、成就、审计、广播、角色等 29 页），
  共 69 个页面路由；`reports/mobile-compare/REPORT.md`（原型 58 路由全覆盖，
  两侧 0 横向溢出，14 批次视觉比对）。
- 板块 `sort=featured|unanswered` 与作者/标题筛选、首页 `sort=following` 与
  帖子卡点赞数、`GET /api/v1/tags/{slug}/posts` 标签聚合。
- 修复 icons allowlist 缺失的 `rotate-cw` / `shopping-cart`（静默渲染空白）。

### 小程序端（首次入库）

- 微信小程序客户端 `miniprogram/`：认证（含 MFA）、板块/帖子/评论/搜索、
  发帖/编辑/收藏/点赞、签到、成就、商城与装扮、设备会话管理；复用会话
  Cookie + CSRF 机制。

### 文档与部署

- `API.md §21` 社交/经济/个人域端点规范；`PERMISSION-MATRIX.md` 社交与个人域
  动作表（附录使用数 183 → 223 operation）；`ARCHITECTURE.md §3.5` OAuth
  登录与市场交易的前后端分工边界；`OPERATIONS.md §20` 从零部署 Runbook；
  README 状态刷新（223 ops / 102 工作包 / 819 叶子任务）。

---

## v1.0.0-rc.2 — 2026-08-08（M17 RC 冻结 / 预发布 / 冒烟 / Flag 记录）

### RC 冻结（M17-FREEZE）

- RC 变更清单 `reports/rc/change-list.md`：193 operations 冻结，上一版本 client
  向后兼容（`check-client-compat.rb` 193/193），无未批准破坏性变更。
- 差异文档同步 `reports/rc/doc-sync.md`：Requirements/OpenAPI/Schema/Security/Testing
  与专项文档逐项核对。
- OpenAPI 覆盖终态 `reports/rc/coverage-final.md`：193/193 全部 `verified`
  （含 profile-cover 端点补齐与 posts 反应删除端点补齐）。
- 迁移兼容/升级时长/恢复点 `reports/rc/migration-compat.md`；依赖/SBOM/Secret
  清点 `reports/rc/inventory.md`。
- 评审签字：**阻塞**（需产品/后端/前端/安全/测试/运维/运营负责人评审签字）。

### 预发布环境与数据演练（M17-ENV）

- `deploy/staging/` 生产同构编排说明；合成 persona 数据 + canary 日志扫描 CLEAN。
- 空库安装/升级/重复迁移/错误迁移演练、SQLite/附件/OIDC key 备份恢复
  （RPO=0，RTO=0.18s，verify.sh 全绿）、优雅停机（HTTP 0.30s / worker 0.04s）。
- MySQL/MariaDB 恢复演练：**阻塞**（沙箱无真实数据库；脚本就绪）。

### 全角色冒烟（M17-SMOKE）

- 九类 persona 冒烟全绿（Playwright 194 用例 + vitest 567 + 后端 147 binaries），
  报告 `reports/rc/smoke/personas.md`；人工验收清单 `reports/rc/smoke/checklist.md`。

### 专项 Flag 与启用记录（M17-FLAGS）

- 核心论坛/邮箱验证/审核/积分/本地附件默认配置上线；五项可选能力（AI/Video/
  Download Billing/OIDC/Marketplace）默认关闭，逐项启用计划与回滚记录
  `ops/feature-flags/gates.md`（P2 记录审批人/范围/阈值/观察窗口/审计）。

### 法律与上线（M17-LEGAL / M17-LAUNCH）

- 法律/运营/隐私发布确认清单 `docs/legal/README.md`：**阻塞**（需法务/运营签字）。
- 生产上线：**阻塞**（需真实生产主机执行；步骤与 Runbook 已就绪）。

---
## v0.7 — 2026-08-08（M16 测试/安全/故障/经济/性能/发布验收）

### 测试基础设施与契约（M16-HARNESS）

- 稳定错误码四方一致（docs ↔ OpenAPI ↔ backend ↔ frontend）：openapi
  `Problem.code` 同步为 106 码；backend 领域错误转换（marketplace/shop/download/
  activity/ai）输出稳定码而非通用 `conflict/bad_request`；`scripts/check-code-fixtures.rb`
  强制每个稳定码有 Fixture + 前端映射。
- 状态机合法/非法迁移矩阵 `reports/rc/state-machine-coverage.md` +
  `scripts/check-state-machine-matrix.rb`。
- 上一版本生成 client 向后兼容：`compat/frozen-client/`（M15 契约冻结）+
  `scripts/check-client-compat.rb`（操作表面/请求参数/请求体/响应 schema/enum 全兼容）。
- 契约边缘测试 `backend/tests/harness_contract.rs`（最大 limit 钳制/未知参数/非法
  游标 400/cursor 不重不漏）。
- Fixture 约定文档 `docs/FIXTURES.md`；CI 四层 `docs/CI-LAYERS.md` +
  `.github/workflows/{nightly,release-rc}.yml`（PR/nightly/RC/prod-smoke）。
- `check-openapi.rb` 基线冻结 193；`bblbb-migrate` 二进制命名统一（Cargo.toml `[[bin]]`）。

### 安全（M16-SECURITY）

- OWASP ASVS v4.0.3 基线映射 `security/ASVS-BASELINE.md`（含排除项/负责人/证据）。
- 隐藏内容防泄漏扫漏 `security/leak-sweep.md`（16 渠道，PASS）。
- 依赖/Secret/许可证/SBOM 扫描 `ops/security/scan.sh`（Secret OK；cargo audit
  4 项上游固定/无修复按风险接受并跟踪；SBOM 634 组件）。

### 存储故障与外部失败（M16-STORAGE-FAULTS）

- `backend/tests/storage/adapter.rs`：Local/S3 adapter contract + S3 mock 故障注入
  （403/404/429/5xx→稳定分类与 retryable）+ multipart 生命周期 + 预签名 URL/重签。
- `backend/tests/faults.rs`：外部失败不变量（URL 签发失败整体回滚、幂等不重复
  扣费、账本恒等式）。

### 经济（M16-ECONOMY）

- `backend/tests/economy/step_injection.rs`：每一步注入失败 → 无订单/权益/流水/
  Outbox/审计残留（余额不足/库存/限购/等级门槛/幂等冲突 5 用例）。

### 性能（M16-PERF）

- `bench/gen-synthetic.sh` 合成数据：100k 用户 / 1M 帖子 / 200k 评论，
  DB 1137MB；`bench/measure.sh` release 真实请求 p95 基线
  `reports/perf/baseline.md`（详情/搜索/登录/发帖/回复 16–18ms、SSR 19–24ms、
  无过滤列表 1207ms 已知慢查询、RSS 35MB）；阈值版本化 `bench/thresholds.md`。

### 发布验收（M16-RELEASE-TEST）

- `reports/rc/`：harness.md / release-test.md / failure-template.md /
  smoke/checklist.md / p0-p1.md / state-machine-coverage.md。
- 演练实测：迁移升级 apply_ms=125 · 备份恢复 RPO=0/RTO=0.18s · 冒烟 PASS=14 ·
  优雅停机 PASS=8 · release bundle PASS=26 · alerts PASS=71 · Playwright 194 passed。

# BBLBB — 文档变更记录

## v0.6 — 2026-08-08（M15 生产运维交付）

### 生产部署

- Release bundle 布局与最小权限（`deploy/RELEASE-BUNDLE.md`）、构建脚本
  `deploy/scripts/build-release-bundle.sh`（backend release 二进制 + frontend
  build + 三方言迁移 + 依赖锁 + METADATA/SBOM/SHA256SUMS）。
- Caddy 模板（`deploy/Caddyfile.template`）：TLS、HTTP→HTTPS、CSP、安全头、
  压缩、body limit（10MB 双层）、`/readyz` 与 `/metrics` 不公开代理。
- systemd units（`deploy/systemd/`）：backend/frontend/worker + 每日备份
  timer；服务用户 `bblbb` 对 release 目录只读（不可写）。
- 启动检查 `deploy/scripts/startup-checks.sh`（origin/Cookie/DB/目录/迁移/
  OIDC key/外部配置）；发布/回滚编排 `deploy/scripts/release.sh`。

### 观测

- `BBLBB__LOG_FORMAT` 配置（text/json）：JSON 日志字段
  timestamp/service/level/request_id/route/method；敏感字段名与值级脱敏
  （Cookie/Authorization/OAuth token/密码/完整邮箱/隐藏正文/Prompt/签名 URL）；
  `ops/scan-log-corpus.sh` 日志语料扫描自检与实测 CLEAN。
- `/metrics` Prometheus 端点（loopback-only）：HTTP p50/p95/p99、错误、
  429、DB pool、SQLite busy、连接失败 + Session/CSRF/TOTP/OAuth/上传/存储/
  账务/任务/Outbox 领域指标（`deploy/monitoring/metrics.md`）。
- 告警定义（`deploy/monitoring/alerts.md`）+ 表推演练
  （`deploy/monitoring/alerts-drill.sh`，PASS=71）。

### 备份与恢复

- `ops/backup/`：sqlite.sh（WAL checkpoint + 安全复制）、manifest.sh、
  daily.sh、mysql.sh/mariadb.sh（真实演练为外部阻塞 [!]）。
- `ops/restore/`：sqlite.sh、verify.sh（用户/账本恒等式/迁移 checksum/grant/
  outbox/audit）、verify-attachments.sh、verify-oidc-keys.sh。
- OIDC 密钥分离存储设计（`ops/backup/oidc-keys.md`）。
- 真实演练 `ops/backup/drill-sqlite.sh`：RPO=0（WAL checkpoint 一致快照）、
  RTO=0.18s（擦除→完整恢复+内容校验，实测）。

### 升级与 Runbook

- `--worker` 模式（`bblbb-backend --worker`）与任务分发
  `backend/src/jobs/dispatch.rs`；SIGTERM 优雅停机实测
  `ops/test-graceful-shutdown.sh`（HTTP 0.30s、worker 0.04s 干净退出）。
- 迁移升级演练 `deploy/scripts/drill-migration-upgrade.sh`
  （apply_ms=68、lock_events=0、幂等二次 apply 0）。
- 发布后冒烟 `ops/smoke/smoke.sh`（db/登录/发帖/回复/附件/账本/管理 API，
  PASS=14）。
- 命令级 Runbook 全套（`ops/runbooks/`）+ 值班矩阵 + 非作者执行记录。

## v0.5 — 2026-08-04

### 需求问答确认

- 产品定位确认为公开兴趣社区，并冻结匿名访问、邮箱验证、冷静期、强制 2FA、无私信和审核公开策略。
- 冻结内容可见性、B币非现金属性、内部商城、共享附件容量、AI 逐次同意、第三方应用额度市场和数据保留规则。
- 已确认扩展能力全部纳入 v1.0 目标，以内部里程碑、Feature Flag 和专项发布门槛控制启用，不再标记为 v1.1/v1.2。
- 新增 `PRODUCT-DECISIONS.md` 和 `CRAWLER-POLICY.md`，明确产品所有者决策以及搜索索引、AI 爬虫和批量访问策略。
- 产品所有者于 2026-08-04 确认冻结 v0.5，作为生产工程开发依据；生产工程实现尚未开始。

## v0.4 — 2026-08-04

### 新增能力

- Marketplace Offer、Checkout Intent、托管确认、双边站内账本、商户待结算余额、退款和 Webhook。
- 下载抵扣的策略优先级、免费授权、原子扣费和 S3 URL 重签语义。
- AI Gateway、Provider、独立同意、Task、Suggestion、预算和故障降级。
- 核心 Video Service 与 Direct/HLS/Xigua Provider Adapter、手动 URL、CSP 和 SSRF/HLS 防护。
- 内部积分商城、装扮槽位、签到任务、社区 Reaction、库存和活跃反刷规则。

### 基线补档

- 新增文档状态与功能发布矩阵。
- 新增统一术语、状态机、错误码和 Endpoint 权限矩阵。
- 新增资源 DTO 契约、Marketplace 账务决策。
- 新增配置、领域事件、数据保留和隐私矩阵。
- 统一基础文档版本为 v0.4。
- 统一 Post 的 `closed_at` 语义、Sanction 枚举、Video 管理 API 和 AI/Video 异步 Task 语义。

### 兼容说明

v0.4 仍处于首次正式开发前，没有已发布的稳定 v1 API 客户端，因此上述统一不构成生产破坏性变更。从 v1.0 发布后，删除字段、改变枚举语义、改变账务或权限规则必须按 `API.md` 的废弃周期处理。
