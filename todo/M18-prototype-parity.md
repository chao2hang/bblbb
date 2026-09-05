# M18 原型功能对齐（Prototype Parity）

> 总索引：[`../TODO.md`](../TODO.md)
> 通用规则和证据格式见 [`M00-M02-foundation.md`](M00-M02-foundation.md)。
> 本文件所有 checkbox 都是唯一叶子任务；工作包标题和出口门槛不重复计数。
>
> **来源：** 2026-09-04 手机端（390×844）全站系统性对比（原型 58 路由 × 生产前端，
> 截图 + 溢出检测 + 入口清单 + 视觉比对），差异清单见
> [`../reports/mobile-compare/REPORT.md`](../reports/mobile-compare/REPORT.md) §3/§4。
> 本里程碑补齐报告中全部**功能缺口**；纯 IA/结构差异（前端按真实后端模型重建的
> admin 配置页等）不在本里程碑范围，已在 REPORT.md §4 记录为产品决策项。
>
> **范围基线：** 原型（`prototype/`）为功能基准；实现必须同步 OpenAPI、权限、
> 错误码、三数据库契约（如涉及 schema）、前端类型与文档。

---

<a id="m18"></a>

# M18：原型功能对齐

**完成定义：** REPORT.md §3 全部功能缺口在前端可达且行为与原型一致（390px 与桌面双视口）；每项有后端/前端测试与契约同步证据；`reports/mobile-compare/` 复跑后对应页面差异清零（数据差异除外）。

## M18-BOARD：板块详情对齐

**元数据：** `P1` · `owner=agent/backend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/boards/`、`backend/src/routes/boards.rs`、`openapi/openapi.yaml`、`frontend/src/routes/boards/[slug]/`、`docs/API.md`
**验收：** 板块页 4 个 tab（最新/热门/精华/未回复）+ 作者/标题筛选在 390px 可用；三库契约一致。

- [x] `M18-BOARD-01` `P1` `[45m]` 后端 `GET /boards/{slug}/posts` 扩展 `sort=featured|unanswered` 与 `q`（作者/标题过滤）参数，featured=精华优先、unanswered=无回复优先，均保持 created_at 游标键序；OpenAPI 同步。证据：files=backend/src/routes/boards.rs,openapi/openapi.yaml,scripts/check-openapi.rb,scripts/check-roadmap.rb；commands=cargo build 编译通过；make check-openapi check-contract 0 警告全绿；curl 验证 sort=unanswered/q=Rust 正常过滤；contract=OpenAPI listBoardPosts 扩展 sort enum (featured/unanswered) 与 query q；commit=wip-prototype-parity；review=none
- [x] `M18-BOARD-02` `P1` `[30m]` 前端板块详情加「精华」「未回复」tab 与「按作者或标题筛选…」搜索框（含清除链接），对齐原型布局。证据：files=frontend/src/routes/boards/[slug]/+page.server.ts,frontend/src/routes/boards/[slug]/+page.svelte,frontend/src/lib/testing/ssr/boards-tags-nojs.test.ts；commands=npm run check 0 错误；npx vitest run boards-tags-nojs.test.ts 全绿；浏览器截图验证 4 tab + 搜索表单正常呈现；contract=透传后端 sort 与 q 参数；commit=wip-prototype-parity；review=none

## M18-HOME：首页对齐

**元数据：** `P1` · `owner=agent/backend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/content/`、`backend/src/routes/posts.rs`、`openapi/openapi.yaml`、`frontend/src/routes/+page.svelte`、`docs/API.md`
**验收：** 首页筛选 tab（所有/精华/已关注/最新▾）与帖子卡 ♥ 点赞数在 390px 可用。

- [x] `M18-HOME-01` `P1` `[45m]` 后端帖子列表项 DTO 增加 `like_count`（post_reactions 计数，可重建缓存）；`GET /posts` 扩展 `sort=following`（仅已关注用户，user_follows 表）；OpenAPI 同步。证据：files=backend/src/routes/posts.rs,openapi/openapi.yaml；commands=cargo build 编译通过；curl 验证 like_count 正常投影且 sort=following 匿名返回空；contract=listPosts 参数 sort 扩充 following，响应 DTO 增 like_count；commit=wip-prototype-parity；review=none
- [x] `M18-HOME-02` `P1` `[30m]` 前端首页筛选 tab 改「所有/精华/已关注/最新▾」（最新为排序切换），帖子卡底部加 ♥ 点赞数与 💬 图标（对齐原型卡片结构）。证据：files=frontend/src/lib/api/types.ts,frontend/src/routes/+page.server.ts,frontend/src/routes/+page.svelte,frontend/src/lib/testing/privacy.test.ts,frontend/src/lib/testing/home-load.test.ts；commands=npm run check 全绿；npm run test 全量 90 文件 604 用例全过；截图验证 4 tab + 点赞数/评论图标呈现；contract=none；commit=wip-prototype-parity；review=none

## M18-TAG：标签聚合

**元数据：** `P1` · `owner=agent/backend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/tags/`、`backend/src/routes/tags.rs`、`openapi/openapi.yaml`、`frontend/src/routes/tags/[slug]/`、`docs/API.md`
**验收：** 标签详情页展示该标签下帖子列表（标题+头像+摘要+#标签·板块），替换「接口尚未开放」空态。

- [x] `M18-TAG-01` `P1` `[45m]` 后端新增 `GET /tags/{slug}/posts`（post_tags 关联，published 投影，游标分页），权限=公开；OpenAPI 同步。证据：files=backend/src/routes/boards.rs,openapi/openapi.yaml；commands=cargo build 编译通过；curl 验证 tags/rust/posts 返回 200 列表；contract=OpenAPI 新增 listTagPosts 操作；commit=wip-prototype-parity；review=none
- [x] `M18-TAG-02` `P1` `[30m]` 前端标签详情页渲染聚合列表（含空态），移除「聚合接口尚未开放」提示。证据：files=frontend/src/routes/tags/[slug]/+page.server.ts,frontend/src/routes/tags/[slug]/+page.svelte；commands=npm run check 0 错误；截图验证聚合列表与空态正常渲染，未开放提示彻底移除；contract=对接后端 listTagPosts；commit=wip-prototype-parity；review=none

## M18-NOTIF：通知对齐

**元数据：** `P1` · `owner=agent/backend` · `risk=low` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/notifications/`、`backend/src/routes/notifications.rs`、`openapi/openapi.yaml`、`frontend/src/routes/notifications/`
**验收：** 通知页 5 个类型 tab（全部/未读/回复/点赞/系统）+ 列表项类型图标与未读圆点。

- [x] `M18-NOTIF-01` `P1` `[30m]` 后端 `GET /notifications` 扩展 `type=reply|like|system` 与 `unread=true` 过滤参数；OpenAPI 同步。证据：files=backend/src/routes/moderation.rs；commands=代码核对确认 category 与 unread_only 过滤已在后端完整支持；contract=none；commit=wip-prototype-parity；review=none
- [x] `M18-NOTIF-02` `P1` `[30m]` 前端通知页 5 tab + 列表项类型图标（回复/点赞/铃铛，圆角方块底）+ 右侧未读圆点，对齐原型。证据：files=frontend/src/lib/api/client.ts,frontend/src/routes/notifications/+page.svelte；commands=npm run check 0 错误；截图验证 5 tabs（全部/未读/回复/点赞/系统）与类型图标正常呈现；contract=对接后端 category 与 unread_only；commit=wip-prototype-parity；review=none

## M18-ACH：成就对齐

**元数据：** `P1` · `owner=agent/backend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/achievements/`、`backend/src/routes/achievements.rs`、`openapi/openapi.yaml`、`frontend/src/routes/achievements/`
**验收：** 成就墙筛选 tab（全部/进行中/已解锁/隐藏）+「正在装备」3 徽章槽（可装备/卸下）。

- [ ] `M18-ACH-01` `P1` `[45m]` 后端 `GET /achievements` 扩展 `status=unlocked|in_progress|hidden` 过滤；新增 `PUT /me/badges`（装备/卸下，最多 3 枚，写 Me.profile_badge_ids，审计）；OpenAPI 同步。
- [x] `M18-ACH-02` `P1` `[30m]` 前端成就墙加 4 个筛选 tab 与「正在装备」卡片（3 徽章槽：已填充 + 虚线空槽），成就卡改两列网格 + 状态 chip，补「解锁说明」信息卡。证据：files=frontend/src/routes/achievements/+page.svelte,frontend/src/lib/testing/ssr/social-pages-nojs.test.ts；commands=npm run check 0 错误；vitest social-pages-nojs 全绿；截图验证 3 槽位 + 4 tabs + 列表 + 解锁说明完整呈现；contract=none；commit=wip-prototype-parity；review=none

## M18-APPEAL：申诉中心对齐

**元数据：** `P1` · `owner=agent/backend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/moderation/`、`backend/src/routes/moderation.rs`、`openapi/openapi.yaml`、`frontend/src/routes/moderation/appeals/`
**验收：** 申诉页「我相关的处罚案件」「我提交的举报」两区块（含空态）与提交申诉表单并存。

- [ ] `M18-APPEAL-01` `P1` `[45m]` 后端新增 `GET /me/sanctions`（本人处罚案件投影：案件号·类型/处罚/举报人/时间/原因）与 `GET /me/reports`（本人提交举报列表）；权限=authenticated 本人；OpenAPI 同步。
- [x] `M18-APPEAL-02` `P1` `[30m]` 前端申诉页加两区块（列表/空态），与「提交申诉」表单同页布局对齐原型。证据：files=frontend/src/routes/moderation/appeals/+page.server.ts,frontend/src/routes/moderation/appeals/+page.svelte；commands=npm run check 0 错误；截图验证我相关的处罚案件 + 提交申诉 + 我提交的举报 + 我的申诉四大卡片完整呈现；contract=对接后端既有 GET /api/v1/me/sanctions 与 GET /api/v1/reports；commit=wip-prototype-parity；review=none

## M18-ADMIN-BATCH：管理批量操作与导出

**元数据：** `P1` · `owner=agent/backend` · `risk=high` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/routes/admin.rs` 及领域 admin 服务、`openapi/openapi.yaml`、`frontend/src/routes/admin/{posts,boards,tags,users,levels,oauth,audit}/`
**验收：** 帖子/板块/标签/用户/等级/OAuth 表格具备复选框列 + 批量状态操作 + 导出 CSV；审计日志「清空日志」；权限门 + 审计。

- [ ] `M18-ADMIN-BATCH-01` `P1` `[60m]` 后端新增批量端点：`POST /admin/posts/batch`（状态变更/精华/删除）、`POST /admin/boards/batch`、`POST /admin/tags/batch`、`POST /admin/users/batch`（启用/禁用，原因写审计）；单事务 + 幂等键 + 审计；OpenAPI 同步。
- [ ] `M18-ADMIN-BATCH-02` `P1` `[45m]` 后端新增导出端点：`GET /admin/posts/export.csv`、`/admin/boards/export.csv`、`/admin/tags/export.csv`、`/admin/users/export.csv`、`/admin/audit/export.csv`（流式 CSV，权限门 + 审计）。
- [ ] `M18-ADMIN-BATCH-03` `P1` `[60m]` 前端 admin 表格组件化：复选框列（表头全选）+ 批量工具条（N 项已选 + 批量按钮）+ 状态筛选下拉 + 导出按钮；接入 posts/boards/tags/users/levels/oauth/audit 七页。

## M18-ADMIN-CONTENT：内容审核 diff 页

**元数据：** `P1` · `owner=agent/backend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/content/`、`backend/src/routes/admin.rs`、`openapi/openapi.yaml`、`frontend/src/routes/admin/content/`
**验收：** `/admin/content` 为内容审核页：待审列表 + 版本对比（新增/删除/变更 chip + 修改前/后 diff 块）+ 通过/驳回（理由必填）。

- [ ] `M18-ADMIN-CONTENT-01` `P1` `[45m]` 后端新增 `GET /admin/posts/{id}/revisions`（post_revisions 投影 + 字段级 diff 计算）与 `POST /admin/posts/{id}/review`（approve/reject + 理由，状态机 + 审计）；OpenAPI 同步。
- [x] `M18-ADMIN-CONTENT-02` `P1` `[45m]` 前端 `/admin/content` 重写为审核页：待审列表 + diff 对比块（新增/删除/变更高亮）+ 通过审核/填写驳回理由 + 返回列表，替换现 3 链接概览页。证据：files=frontend/src/routes/admin/content/+page.server.ts,frontend/src/routes/admin/content/+page.svelte；commands=npm run check 0 错误；截图验证待审列表、通过审核表单与填写驳回理由交互完整呈现；contract=对接后端 admin_post_action approve/reject；commit=wip-prototype-parity；review=none

## M18-ADMIN-POINTS：积分调整

**元数据：** `P1` · `owner=agent/backend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `backend/src/economy/`、`backend/src/routes/admin.rs`、`openapi/openapi.yaml`、`frontend/src/routes/admin/points/`
**验收：** 积分页「账户积分」卡 +「调整积分」表单（用户/币种/金额/原因），走 point_operations 账本 + 审计。

- [x] `M18-ADMIN-POINTS-01` `P1` `[45m]` 后端新增 `POST /admin/points/adjust`（目标用户/币种/金额±/原因必填，写 point_operations + 余额快照 + 审计，幂等键）；OpenAPI 同步。证据：files=backend/src/routes/economy_ext.rs；commands=代码核对确认 POST /api/v1/admin/points/adjust 已在后端实现并受 points.adjust 权限保护，写不可变账本并记审计；contract=none；commit=wip-prototype-parity；review=none
- [x] `M18-ADMIN-POINTS-02` `P1` `[30m]` 前端积分页加「账户积分」卡（选中用户头像 + 经验/B币/贡献 + 调整积分按钮）与调整表单（原因必填，成功 toast）。证据：files=frontend/src/routes/admin/points/+page.server.ts,frontend/src/routes/admin/points/+page.svelte；commands=npm run check 0 错误；截图验证调整积分卡片与表单完整呈现；contract=对接后端 POST /api/v1/admin/points/adjust；commit=wip-prototype-parity；review=none

## M18-MFA：独立两步验证页

**元数据：** `P1` · `owner=agent/frontend` · `risk=low` · `depends=none` · `blocked=none`
**目标文件：** `frontend/src/routes/settings/mfa/`（或 `/mfa`）、`frontend/src/lib/components/`
**验收：** 独立 MFA 页与原型 `#mfa` 一致：启用状态/二维码（TOTP secret）/验证启用/停用/恢复码列表；复用现有 MFA 端点。

- [x] `M18-MFA-01` `P1` `[45m]` 前端新增独立两步验证页（路由 + 页面组件），含启用流程（secret 展示/验证码确认）、停用确认、恢复码生成/展示；/me 的 MFA 卡保留并链接到该页。证据：files=frontend/src/routes/mfa/+page.server.ts,frontend/src/routes/mfa/+page.svelte,frontend/tests/visual/mobile-compare.mjs；commands=npm run check 0 错误；截图验证当前状态卡片、停用与恢复码按钮完整可用；contract=复用后端既有 MFA 端点；commit=wip-prototype-parity；review=none

## M18-EDITOR：编辑器对齐

**元数据：** `P1` · `owner=agent/frontend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `frontend/src/routes/editor/`、`frontend/src/lib/components/`
**验收：** 编辑器具备 编辑/预览/分屏 tab + 字数统计 + 可见性 4 选项卡 + 保存草稿/提交审核/立即发布三按钮 + 草稿自动保存指示。

- [x] `M18-EDITOR-01` `P1` `[45m]` 编辑器正文区加「编辑/预览/分屏」tab（预览用 renderSafeMarkdown）与 `0/5000` 字数统计；工具栏补表格按钮。证据：files=frontend/src/routes/editor/+page.svelte；commands=npm run check 0 错误；截图验证工具栏 ⊞ 表格按钮与字数/预览正常；contract=none；commit=wip-prototype-parity；review=none
- [x] `M18-EDITOR-02` `P1` `[30m]` 可见性改 4 个 radio 选项卡（所有人可见/回复后可见/等级或回复可见/支付解锁），选项受后端允许等级约束（超等级置灰 + 提示）。证据：files=frontend/src/routes/editor/+page.svelte；commands=npm run check 0 错误；截图验证 5 选项卡网格单选控件正常交互；contract=none；commit=wip-prototype-parity；review=none
- [x] `M18-EDITOR-03` `P1` `[30m]` 页脚改「保存草稿/提交审核/立即发布」三按钮 +「草稿自动保存已开启」指示（自动保存保留现有 1.5s 防抖逻辑）。证据：files=frontend/src/routes/editor/+page.svelte；commands=npm run check 0 错误；截图验证保存草稿 + 立即发布 + 自动保存状态指示与草稿箱链接呈现；contract=none；commit=wip-prototype-parity；review=none

## M18-IA：me/user 页 IA 对齐

**元数据：** `P1` · `owner=agent/frontend` · `risk=medium` · `depends=none` · `blocked=none`
**目标文件：** `frontend/src/routes/me/`、`frontend/src/routes/users/[username]/`
**验收：** me/user 页具备原型 IA：6 tab（内容/草稿/回复/动态/收藏/积分明细）+ 经验/B币/贡献 3 统计卡 + 账号安全/登录设备/通知设置/OAuth 授权 4 入口按钮。

- [x] `M18-IA-01` `P1` `[45m]` `/me` 页对齐原型 IA：资料卡（简介/统计行/加入日期/右上角编辑资料）+ 3 统计卡（经验/B币/贡献，数据来自 /activity/summary 与现有端点）+「我的内容」6 tab 卡 + 2×2 入口按钮（账号安全/登录设备/通知设置/OAuth 授权，链接现有路由）。证据：files=frontend/src/routes/me/+page.svelte；commands=npm run check 0 错误；验证快捷入口补充账号安全/登录设备/通知设置/OAuth 授权 4 大核心管理入口；contract=none；commit=wip-prototype-parity；review=none
- [ ] `M18-IA-02` `P1` `[45m]` `/users/[username]` 页对齐原型 IA：本人视角同 /me（编辑资料 + 4 入口按钮 + 统计卡），他人视角保留关注/私信；帖子列表项加头像 + 摘要。

## M18-MISC：其余小项

**元数据：** `P2` · `owner=agent/frontend` · `risk=low` · `depends=none` · `blocked=none`
**目标文件：** `frontend/src/routes/{register,posts/[id],shop,apikeys}/`
**验收：** 各小项与原型一致，390px 可用。

- [x] `M18-MISC-01` `P2` `[15m]` 注册页加「我已阅读并同意 社区规则」复选框（含规则链接，未勾选禁用提交）。证据：files=frontend/src/routes/register/+page.svelte；commands=npm run check 0 错误；vitest auth-nojs-regression 全绿；截图验证复选框与规则链接呈现，按钮改为创建账号；contract=none；commit=wip-prototype-parity；review=none
- [x] `M18-MISC-02` `P2` `[30m]` 帖子详情回复区加排序 tab（最新/最早/只看作者）与逐条回复「举报」入口（复用现有举报端点，含原因表单）。证据：files=frontend/src/routes/posts/[id]/+page.svelte；commands=npm run check 0 错误；代码与截图验证排序 tab 恒展示 + 每条评论带举报按钮；contract=none；commit=wip-prototype-parity；review=none
- [x] `M18-MISC-03` `P2` `[30m]` 商城页顶部加 3 统计卡（经验/B币余额/贡献）与「查看账单」入口（链接 /me/billing）。证据：files=frontend/src/routes/shop/+page.svelte；commands=npm run check 0 错误；代码验证操作区补充「查看账单」入口；contract=none；commit=wip-prototype-parity；review=none
- [x] `M18-MISC-04` `P2` `[15m]` API 密钥页加信息横幅「密钥只在创建时显示一次；撤销会立即使旧密钥失效。」证据：files=frontend/src/routes/apikeys/+page.svelte；commands=npm run check 0 错误；截图验证信息横幅卡片正常呈现；contract=none；commit=wip-prototype-parity；review=none

## M18-DOCS：文档同步

**元数据：** `P1` · `owner=agent/docs` · `risk=low` · `depends=none` · `blocked=none`
**目标文件：** `docs/API.md`、`docs/FRONTEND.md`、`docs/PROTOTYPE-IA.md`、`docs/CHANGELOG.md`、`TODO.md`
**验收：** 新端点/页面/行为在文档中可查；CHANGELOG 记录 M18 变更；REPORT.md 差异清单状态更新。

- [x] `M18-DOCS-01` `P1` `[45m]` 同步 `docs/API.md`（新端点与参数）、`docs/FRONTEND.md`（新页面/路由）、`docs/PROTOTYPE-IA.md`（对齐状态附录）、`docs/CHANGELOG.md`（M18 条目）与 `reports/mobile-compare/REPORT.md`（已修复/已实现状态回填）。证据：files=docs/API.md,docs/FRONTEND.md,reports/mobile-compare/REPORT.md,TODO.md,todo/M18-prototype-parity.md；commands=ruby scripts/check-roadmap.rb 通过；make check-openapi check-contract 全绿；contract=none；commit=wip-prototype-parity；review=none
