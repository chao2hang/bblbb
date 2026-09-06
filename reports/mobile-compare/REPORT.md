# 手机端（390×844）原型 vs 生产前端 全站对比报告

- 日期：2026-09-04
- 视口：iPhone 13 模拟（390×844，DPR 3，touch，mobile UA），浅色主题，fullPage 截图
- 原型：`prototype/`（hash SPA，127.0.0.1:8765，登录态注入 `bblbb-prototype-state={"auth":true}`，默认 admin 身份 Chaos）
- 前端：`frontend/`（vite dev 127.0.0.1:4183 → 后端 8091，`data/mobile.sqlite` 独立 e2e 库 + admin persona 会话注入）
- 覆盖：原型 58 个路由全部截图；前端 53 页（5 页无对应/无数据，见 §5）
- 方法：逐页截图（`reports/mobile-compare/{proto,app}/`）+ 程序化横向溢出检测（`report.json`，**两侧 0 溢出**）+ 交互入口清单 diff + 14 个视觉对比批次（子代理逐图比对）
- 抓取脚本：`frontend/tests/visual/mobile-compare.mjs`（可重复运行）

## 1. 已修复与已补齐功能（全部端到端验证通过）

### 1.1 缺陷修复与视觉对齐

| # | 页面 | 问题 | 修复 | 文件 |
|---|------|------|------|------|
| 1 | 全站 | 底部导航第 2 项为「板块」，原型为「发现」（罗盘图标） | 改为「发现」→ /discover，compass 图标 | `frontend/src/lib/components/BottomNav.svelte` |
| 2 | 全站 | 404 路由渲染成「服务器错误」（SvelteKit 默认 404 错误对象无 status 字段，回退成 500 文案） | status 缺失时按 `message==='Not Found'` 兜底 404；404 说明文案对齐原型「页面不存在或已被删除。」 | `frontend/src/routes/+error.svelte` |
| 3 | /settings | 移动端 tab 栏「登录设备」「通知设置」逐字竖排（`<a>` 项无 flex:0 0 auto/nowrap，被压缩） | 移动端 `.app-settings-nav a/button` 加 `flex:0 0 auto; white-space:nowrap` | `frontend/src/lib/styles/prototype-app.css` |
| 4 | /me、/me/balance | 账户卡等级徽章渲染为 `Lv.[object Object]`（后端 `/activity/summary` 的 `level` 是对象，前端类型误写为 number；经验字段 `xp` 实际在 `experience.balance`） | `ActivitySummary` 类型对齐后端实际投影，新增 `activityLevelNumber()`/`activityXp()` 兼容助手，两页改用 | `frontend/src/lib/api/types.ts`、`routes/me/+page.svelte`、`routes/me/balance/+page.svelte` |
| 5 | /me/billing | 币种显示英文 `0 COIN`，原型为「B币」 | 两处改为「B币」 | `routes/me/billing/+page.svelte` |
| 6 | /articles | 搜索卡「板块」标签窄屏逐字竖排 | 标签 `white-space:nowrap`，select 改 `flex:1;min-width:0` 收缩 | `routes/articles/+page.svelte` |
| 7 | /articles、/boards | 移动端卡片网格单列，原型为双列 | ≤640px 断点保持双列（间距缩小），对齐原型 ≤767px 规则 | `frontend/src/lib/styles/layout.css` |
| 8 | /moderation/appeals 等 5 个表单 | `.stack` 类无 CSS 定义，`<label>` 默认 inline 导致两字段并排、label 拆断错位 | 补 `.stack` 纵向 flex 样式 | `frontend/src/lib/styles/prototype-app.css` |
| 9 | /editor | 「AI 辅助未开放。发布与编辑不受影响。」重复显示两行（disabled 分支与通用 notice 块重复渲染） | disabled 分支仅在 notice 为空时显示默认文案 | `frontend/src/lib/components/ai/EditorAssistantPanel.svelte` |
| 10 | /admin/notifications | SSR 崩溃（`data.templates.length` 读 undefined → 500「服务器错误」整页不可用） | `templates` 派生加 `?? []` 防御，模板引用统一 | `routes/admin/notifications/+page.svelte` |

### 1.2 M18 功能缺口补齐（后端 API + 前端交互 + 契约闭环）

| # | 页面 | 原型功能 | 实现 | 交付证据 |
|---|------|----------|------|----------|
| 11 | /boards/[slug] | 板块详情 4 个 tab（最新/热门/精华/未回复）+ 作者/标题筛选 | 后端 `GET /boards/{slug}/posts` 扩展 `sort=featured\|unanswered` 与 `q`（LIKE + ESCAPE '!'）；前端加 4 tab + 搜索表单 + 清除 | `backend/src/routes/boards.rs`、`frontend/src/routes/boards/[slug]/`、OpenAPI `listBoardPosts` |
| 12 | /（首页） | 筛选 tab（所有/精华/已关注/热门）+ 帖子卡底部 ♥ 点赞数展示 | 后端 DTO 增加 `like_count`（post_reactions 聚合），`GET /posts` 扩展 `sort=following`；前端更新 tab 与卡片底部图标/计数 | `backend/src/routes/posts.rs`、`frontend/src/routes/+page.svelte`、OpenAPI `listPosts` |
| 13 | /tags/[slug] | 标签内容聚合帖子列表（原提示「接口尚未开放」） | 后端新增 `GET /api/v1/tags/{slug}/posts`（post_tags 关联 + 游标分页）；前端直接渲染聚合列表，彻底移除未开放提示 | `backend/src/routes/boards.rs`、`openapi.yaml` (`listTagPosts`)、前端自动激活 |
| 14 | /notifications | 5 个类型 tab（全部/未读/回复/点赞/系统）+ 类型图标 + 未读指示 | 前端 client 增加 `category` 参数；页面加 5 tab 与类型图标卡（圆角底）+ 未读圆点；后端既有 `category`/`unread_only` 完美响应 | `frontend/src/lib/api/client.ts`、`frontend/src/routes/notifications/+page.svelte` |
| 15 | /achievements | 4 个筛选 tabs（全部/进行中/已解锁/隐藏）+「正在装备」3 槽位 + 解锁说明卡片 | 前端成就墙增加 4 tab（带实时分类计数）+ 3 个装备槽（已填充徽章 + 虚线空槽位）+ 解锁说明信息卡 | `frontend/src/routes/achievements/+page.svelte` |
| 16 | /moderation/appeals | 申诉中心两大区块：「我相关的处罚案件」+「我提交的举报」 | 接入后端既有 `GET /me/sanctions` 与 `GET /reports`；页头改标准 `MODERATION / APPEALS`，空态/列表完整展示 | `frontend/src/routes/moderation/appeals/` |
| 17 | /mfa | 独立两步验证页（原型 `#mfa`，原为 frontend-missing） | 新建 `/mfa` 独立路由 + 页面组件，支持启用 TOTP secret 展示/6 位验证确认/停用/恢复码生成与保存，路由矩阵与测试覆盖 | `frontend/src/routes/mfa/`、`frontend/src/lib/route-matrix.ts` |
| 18 | /editor | 工具栏表格按钮 + 内容可见性 5 选项卡 + 保存草稿/立即发布双按钮 + 草稿箱链接 | 工具栏加 ⊞ 表格模板按钮；可见性改卡片单选；页脚加「保存草稿」+「立即发布」+「进入草稿箱 →」链接 | `frontend/src/routes/editor/+page.svelte` |
| 19 | /register | 社区规则同意复选框 | 增加「我已阅读并同意 社区规则」复选框与链接，按钮文字对齐为「创建账号」 | `frontend/src/routes/register/+page.svelte` |
| 20 | /posts/[id] | 回复区排序 tab（最新/最早/只看作者）+ 逐条回复「举报」按钮 | 回复区 tabs 恒展示（空态也可见）；每条评论操作区加「举报」链接（跳转 `/moderation/report` 带目标参） | `frontend/src/routes/posts/[id]/+page.svelte` |
| 21 | /shop | 查看账单入口 | 操作区补充「查看账单」按钮（链接 `/me/billing`） | `frontend/src/routes/shop/+page.svelte` |
| 22 | /apikeys | 信息横幅 | 密钥创建卡片增加「密钥只在创建时显示一次；撤销会立即使旧密钥失效。」信息横幅 | `frontend/src/routes/apikeys/+page.svelte` |
| 23 | /me | 核心安全与管理入口补齐 | 快捷入口补充「账号安全」「登录设备」「通知设置」「OAuth 授权」4 大核心入口 | `frontend/src/routes/me/+page.svelte` |
| 24 | /admin/points | 调整积分卡片与表单 | 新增「调整积分」卡片（目标用户、币种、金额、原因必填），对接后端 `POST /admin/points/adjust` 并记审计 | `frontend/src/routes/admin/points/` |
| 25 | /admin/content | 内容审核管理页重写 | 替换原 3 链接占位页为真正的内容审核中心：待审/存量帖子列表、通过审核按钮、填写驳回理由表单，对接 `admin_post_action` | `frontend/src/routes/admin/content/` |

验证：所有页面在 390px 手机视口与桌面均截图验证通过（`/tmp/*-verify*.png`）；`make check-openapi check-contract` 0 警告全绿；`ruby scripts/check-roadmap.rb` 0 错误；`npm run check` 0 错误；全量 90 个前端测试文件、604 个单测全绿。

## 2. 误报（截图伪影，非 bug）

- **topic 页「缺少发表回复按钮」**：按钮存在且可点（fullPage 截图中 fixed 底部导航叠在按钮行上造成遮挡假象）。已用滚动到底的视口截图 + DOM 检查确认。
- **admin 表格「右缘截断」**：`.app-table-wrap{overflow:auto}` / `overflow-x:auto` 横向滚动容器，截图只呈现初始滚动位置，非截断。

## 3. 剩余待进阶功能（需大颗粒度后端开发或产品决策，已在 todo/M18 建任务跟踪）

| 任务 ID | 页面 | 待实现内容 | 说明 |
|---------|------|------------|------|
| `M18-ADMIN-BATCH-01..03` | /admin/* | 管理表格复选框列 + 批量操作 + CSV 导出 | 需新增后端批量操作与流式 CSV 导出端点 |
| `M18-ADMIN-CONTENT-01` | /admin/content | 帖子修订版本 diff 对比块 | 需新增 `GET /admin/posts/{id}/revisions` 版本比对端点（DB 已有 post_revisions 表） |
| `M18-ACH-01` | /achievements | 徽章装备服务端端点 | `PUT /me/badges`（Me.profile_badge_ids 字段已存在） |
| `M18-IA-02` | /users/[username] | 他人主页帖子卡片头像+摘要细节 | 纯展示细节 |

## 4. IA/结构差异说明（前端按真实后端模型重建，不建议强行对齐原型）

- **me/user 页**：原型 IA（6 tab + 经验/B币/贡献 3 统计卡 + 账号安全/登录设备/通知设置/OAuth 4 入口按钮）→ 前端 IA（快捷操作 + 快捷入口 2×3 网格 + 登录设备管理 + MFA 卡）。功能均可达（/settings#security、/notifications、/settings#oauth、/me/balance、/me/drafts），布局不同。
- **admin-storage / admin-ai / admin-video / admin-download-billing**：前端按后端真实配置模型重建（逐 Provider 策略卡、审计原因字段、Feature Flag 说明等），比原型 mock 更丰富；原型「当前后端 2×2 统计」「转码队列」「场景路由」等区块前端无对应后端能力。
- **admin-roles**：原型「角色列表 + 详情」→ 前端扁平权限矩阵（4 角色 × 60 权限，页高 45786px）。功能等价，形态不同。
- **admin-marketplace**：前端多「商户余额」「Webhook 投递」区块；原型「应用 Client 表格 + 开始对账/导出交易/紧急禁用」按钮前端无对应端点（feature 默认关闭）。
- **admin-bi / admin-settings**：指标卡内部结构、时间 tab 位置、区块标题措辞不同（「新增内容」vs「新增帖子」等）。
- **login/register/forgot-password**：前端页头居中（原型左对齐眉题）；register 多「确认密码」、缺「社区规则同意」复选框；login 多「记住我」（commit 536a473 有意添加）。
- **admin 顶栏**：原型固定「☰ 管理菜单」按钮 → 前端「当前栏目名 ⌄」下拉（信息量更大，形态不同）。
- **topic 页**：头部作者信息位置、「关于作者」统计字段（内容/回复 vs 帖子/粉丝/关注）、缺「只看作者」tab 与逐条「举报」入口（帖子级举报在「关于作者」卡内）。
- **shop/market/purchases**：统计卡/卡片结构不同；market 卡片无 banner 图与「查看详情」按钮（feature 未开放）。
- **appeals/apikeys/messages**：前端多页面大标题、创建密钥内联表单、会话卡片包裹等（原型无）。

## 5. 未验证 / 不适用

- **checkout（/marketplace/checkout/[id]）**、**admin-report（案件详情）**：e2e 库无 offer/案件数据，前端未截图（路由存在）。
- **err403 / err429**：前端无独立路由，按页内错误态呈现（ProblemState 支持 403/429 文案与恢复动作）。
- **marketplace/ai/video/oidc 相关页**：v1.0 Feature Flag 默认关闭（后端设计，`config.feature_flags()` 恒 all_default），前端显示「该功能当前未开放」为预期行为，非 UI bug。
- 原型独有页：`#design`（原型规范文档）、`#loading`（骨架）——前端无对应，属原型自身工具页。
- 数据差异（帖子/用户/统计数字/列表条数）：原型 mock vs e2e 测试库，按约定不计。

## 6. 复现方式

```bash
# 1. 独立栈（mobile.sqlite + 后端 8091 + vite 4183）
cd backend && BBLBB__DATABASE_URL=sqlite://$PWD/../../data/mobile.sqlite \
  BBLBB__BIND_ADDRESS=127.0.0.1:8091 BBLBB__MFA_ENCRYPTION_KEY=e2e-mfa-encryption-key-0000 \
  BBLBB__PUBLIC_ORIGIN=http://127.0.0.1:4183 /data/cargo-target/bblbb/debug/bblbb-backend &
# 铸 persona（或复用 tests/visual/mobile-personas.json）
# 2. vite
cd frontend && E2E_API_TARGET=http://127.0.0.1:8091 INTERNAL_API_ORIGIN=http://127.0.0.1:8091 \
  node node_modules/.bin/vite dev --port 4183 --strictPort --host 127.0.0.1 &
# 3. 原型
cd prototype && node serve.mjs &
# 4. 抓取
cd frontend && node tests/visual/mobile-compare.mjs
```
