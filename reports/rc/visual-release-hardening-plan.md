# BBLBB 前端视觉上线收敛计划

> 目标：对生产 SvelteKit 前端进行多轮人工视觉审核、修复与复审，直到核心页面与关键流程达到可上线的体验质量。
> 范围：`frontend/` 生产前端；`prototype/` 只作为信息架构和视觉回归参考，不把 Mock 原型证据当成生产上线证据。
> 当前基线：`docs/DESIGN-SYSTEM.md`、`docs/BLBUI-DESIGN-SYSTEM.md`、`todo/M19-design-system.md`。

## 1. 完成定义

一次迭代只有同时满足以下条件才允许关闭：

- 页面整体布局有明确主次，首屏任务、内容流和辅助信息的视觉权重正确。
- 每个可见元素的尺寸、边距、对齐、行高、字重、颜色和层级符合设计 token；没有同一语义在不同路由出现不同几何。
- L1 模块、L2 条目、L3 浮层边界清晰；论坛列表不出现条目级卡片阵列或盒中盒。
- 桌面 1440/1024、平板 768、移动 390 四个宽度下无横向溢出、重叠、截断和不可操作控件；内容密度随宽度有意收缩。
- 亮色与暗色均通过人工截图检查；正文、次要文本、边框、焦点环、状态色均可读。
- normal、hover、focus-visible、active、disabled、loading、empty、error、permission 和提交成功状态可区分；状态不只依靠颜色。
- 键盘可完成关键路径，触控目标满足页面密度规则，模态框/抽屉可关闭并恢复焦点。
- 生产路由 SSR/no-JS 结构不回归，自动测试、检查和构建通过；真实后端、安全、运维准入仍按发布门槛单独核验。

## 2. 审查方法

每轮对代表页面执行同一套人工检查：

1. **结构检查**：确认页面标题、导航、主内容、辅助栏、操作区的层级和顺序；记录布局网格、容器宽度、间距、最小/最大高度。
2. **元素检查**：逐项查看导航、标题、说明、按钮、输入、标签、列表行、头像、图标、表格、分页、提示、空态、错误态和浮层；检查边距、对齐、文字溢出和点击热区。
3. **状态检查**：触发 hover/focus/disabled/loading/empty/error/permission/success，确认反馈清晰、文案与动作一致、布局不会跳变。
4. **响应式检查**：亮/暗 × 1440/1024/768/390 截图；重点检查导航折叠、侧栏/抽屉、表格、编辑器、消息 composer、按钮和弹窗。
5. **交互检查**：用 Playwright 执行关键点击、表单提交、返回、Escape、Tab、主题切换、筛选、分页和移动端会话切换；同时检查 console/pageerror/requestfailed。
6. **人工复审**：修复后重新截图并用人工视觉逐图对比；未达到“可扫描、可操作、无视觉冲突”标准的项目回到下一轮，不以一次自动测试通过关闭。

## 3. 多轮执行计划

### Round 0：基线与清单

- 盘点生产路由矩阵、全局壳、设计 token、组件和旧样式层。
- 建立页面分组、代表截图、交互旅程和问题编号。
- 记录当前工作区已有修改，后续只在其基础上增量修复。
- 出口：路由/组件/状态清单和首轮基线截图可复现。

### Round 1：全局壳与设计系统

覆盖：`+layout`、Navbar、BottomNav、Toast、No-JS、主题切换、全局容器、字体、颜色、焦点环、滚动和安全区。

检查：

- 页面顶部留白、导航高度、logo/搜索/右侧动作的对齐和断点。
- 亮暗主题 token 是否真正生效，是否存在旧 `data-theme` 选择器或写死颜色。
- `app.css` 层序、L1/L2/L3 表面纪律、阴影/圆角/边框是否统一。
- 移动底部导航是否遮挡内容、焦点是否可见、Toast 是否覆盖操作。

出口：全站基础几何统一，导航在四种宽度与两种主题都无截断/重叠。

### Round 2：公开首页与内容索引

覆盖：`/`、`/discover`、`/boards`、`/boards/[slug]`、`/tags`、`/tags/[slug]`、`/search`、`/favorites`。

检查：

- 首页主信息流、板块导航、推荐栏的主次，列表容器与条目行关系。
- 帖子/板块/标签行的活跃度、计数、时间、图标和链接热区。
- 筛选、搜索、加载更多、空态、权限提示、分页的可见反馈。
- 390px 下列表是否变成可读的单列，长标题/描述是否破坏行高。

出口：公开浏览路径具有统一的信息架构和扫描节奏。

### Round 3：帖子阅读、回复与编辑器

覆盖：`/posts/[id]`、评论/反应、`/editor`、草稿、附件、视频、AI 辅助面板。

检查：

- 阅读宽度、标题/作者/元信息/正文/代码块/引用/回复的垂直节奏。
- 活跃度脊柱、反应、收藏、举报和登录门的状态表达。
- 编辑器工具栏、标题/摘要/正文/标签/可见性/价格/定时/附件的分组和错误关联。
- 自动保存、冲突、提交中、发布成功/失败、无 JS 回退是否可理解且不跳变。

出口：阅读和创作任务在桌面与移动端都能连续完成。

### Round 4：身份、账户与个人关系

覆盖：`/login`、`/register`、`/password-reset`、`/password-reset/confirm`、`/verify-email`、`/mfa`、`/me`、`/users/[username]`、`/settings`、`/settings/privacy`、`/notifications`、`/messages`、`/apikeys`。

检查：

- 认证页聚焦任务、表单间距、错误/成功/MFA 切换、密码显示控件和回跳信息。
- 资料页 header、统计、tab、设置导航、危险区域的层级和确认流程。
- 通知筛选/全部已读、消息会话/线程/composer、移动端返回与滚动。
- 未登录、无权限、空态、服务失败和提交中状态的文案与视觉差异。

出口：关键身份和社交流程可键盘、触屏和无 JS 退化完成。

### Round 5：商品、积分与市场

覆盖：`/shop`、`/shop/[id]`、`/shop/orders/[id]`、`/me/balance`、`/me/billing`、`/me/wardrobe`、`/marketplace`、`/marketplace/checkout/[id]`、`/marketplace/purchases`、`/achievements`。

检查：

- 商品卡仅在商品比较语义下保留，价格、库存、门槛、有效期和主操作对齐。
- 结算/支付确认显示价格、余额、不可退款和失败恢复；危险动作明确确认。
- 余额/流水/成就/衣柜的信息密度、数字对齐和空态。
- 移动端卡片、表格、结算表单和弹窗的可操作性。

出口：消费和账户资产流程不出现误导性的状态、价格或按钮层级。

### Round 6：后台工作台

覆盖：`/admin` 与所有 `/admin/*`，重点 `/admin/users`、`/admin/content`、`/admin/moderation/cases`、`/admin/themes`、`/admin/settings`、`/admin/marketplace`、`/admin/bi`。

检查：

- 左侧导航、页头、筛选、统计、表格、批量动作的固定顺序。
- 表头/数字列/行 hover/选中/分页/横向滚动；后台密度不牺牲可读性。
- 危险操作确认、原因字段、权限门、版本冲突、加载/错误/成功状态。
- 1024/768/390 下侧栏抽屉、表格和批量工具条是否可用。

出口：后台页面共享统一工作台骨架，关键操作可追踪且不误触。

### Round 7：交互与状态专项

- 为每个共享组件建立状态矩阵：Button、Input、Select、Dialog、Pagination、Table、Badge、Tag、Panel、ListRow、Meta、Toast、EmptyState、ProblemState、LoadingState。
- 逐一检查 Tab 顺序、focus-visible、Escape、焦点回收、`aria-*`、提交禁用、`aria-busy`、`role=alert/status`。
- 检查 no-JS 表单、SSR 首帧、返回链接和浏览器原生校验。

出口：共享组件状态与交互契约一致，页面不再自行发明冲突样式。

### Round 8：全量视觉回归与修复

- 用 Playwright 生成代表页面截图和 DOM 指标，按亮/暗 × 1440/1024/768/390 分类。
- 人工逐图审查：重叠、溢出、空洞、密度、对齐、标题截断、对比度、浮层边界和移动安全区。
- 按 P0（不可用/泄漏/阻塞）、P1（明显影响任务或视觉一致性）、P2（细节 polish）排序修复。
- 每个修复项必须重新截图复审，失败就进入下一轮，不直接标记完成。

出口：核心页面截图没有未关闭 P0/P1，P2 仅限不影响理解和操作的细节。

### Round 9：上线前验证

- `npm run check`、重点 Vitest/SSR/no-JS、Playwright responsive/a11y/keyboard、必要的生产构建。
- 核对 console/pageerror/requestfailed、横向溢出、触控目标、主题 token 和字体加载。
- 检查 SEO、错误页、权限门、缓存/敏感内容不泄漏的前端表现。
- 把真实后端、数据库、邮件、存储、支付、OAuth、安全、监控和回滚门槛与视觉结果分开记录。

出口：前端视觉/交互准入项有证据；非前端生产阻塞项明确列出，不伪装成已上线。

### Round 10+：复审回合

只要 Round 9 或人工复审发现不合格，保留问题编号和截图证据，回到对应页面组继续修复；每一轮都重复“截图 → 人工审查 → 修复 → 截图复审 → 自动回归”，直到完成定义全部满足或出现明确的外部基础设施阻塞。

## 4. 问题记录格式

每个问题记录：

- `id`：如 `VIS-R2-001`
- `severity`：`P0` / `P1` / `P2`
- `route`、`viewport`、`theme`、`state`
- `element`：组件或选择器
- `observation`：人工看到的具体问题
- `expected`：应达到的布局/样式/交互
- `fix`：改动文件与原因
- `verification`：修复后截图、交互步骤、自动检查结果
- `status`：`open` / `fixed-awaiting-review` / `verified`

## 5. 当前轮次状态

- Round 0：已完成。已确认生产前端入口、路由矩阵、设计系统和现有 5173 服务，并对公开、登录态、后台代表路由生成亮/暗 × 桌面/移动基线截图。
- Round 1：已完成并复审。移动底栏主题/安全区、首页列表层级、个人中心网格、后台移动操作区与用户表格固定操作列已修复。
- Round 2：已完成并复审。除上一轮视觉修复外，`ListRow`/`Meta` 已接入通知中心，`MetricGroup` 已接入市场与存储后台；fixture 已修正 HTTPS `PUBLIC_ORIGIN` 与 `ALLOWED_ORIGINS`，真实消息长线程 12 条发送成功；Toast 已做重复合并与最多 4 条上限。
- Round 3：本轮验证出口已完成。`Panel` 已接入通知中心；Toast 重复合并与上限已验证；1024/768 × 亮暗 52 个代表页面无横向溢出；全量 `npm run check` 为 0 errors/0 warnings，Vitest 为 102 files/712 tests 全通过，`npm run build` 成功，5173 健康检查返回 200。
- Round 4：已完成本轮验证出口。`PanelSection` 已接入存储后台“运行状态”分组；存储页亮暗桌面/移动截图通过；全量 Vitest 712 条通过，生产构建成功，fixture 编排语法通过。
- Round 5：已完成本轮验证出口。AI 管理 modal 已补齐打开后焦点进入和全局 Escape，390px 暗色截图/交互复审通过；全量 Vitest 712 条通过，生产构建成功。
- Round 6：已完成本轮验证出口。主题与 AI modal 均补齐 Tab/Shift+Tab 焦点循环、打开焦点进入和关闭焦点回收；390px 暗色探针通过；全量 Vitest 712 条通过，生产构建成功。
- Round 7：已完成本轮验证出口。真实长消息线程在 480px 模拟软键盘 viewport 下 focus composer 后可见，composer bottom=367px、BottomNav top=406px，无重叠；截图与全量回归通过。
- Round 8：已完成本轮验证出口。首页与发现页的重复线程行 markup/CSS 已抽为 `FeedThread.svelte`；两页亮暗 × 桌面/390px 8 个场景保持相同内容映射和无溢出；全量 Vitest 712 条通过，生产构建成功。
- Round 9：已完成本轮验证出口。全路由扫描覆盖 69 条 `+page.svelte`、276 个匿名/登录态 × 桌面/390px 运行；无 pageerror、500、根级横向溢出；OAuth 移动操作列已固定右侧；FeedThread 迁移后的全量 Vitest 712 条通过，生产构建成功。
- Round 10：已完成本轮验证出口。修复 Toast/Navbar ARIA、fixture `E2E_API_TARGET`、注册/登录可访问名称与对比度、后台 nav landmark、用户 ProfileCover、匿名用户/板块 `getPublic` 读取、Tiptap contenteditable 稳定 ID，以及对应 E2E/Vitest 契约；受影响 desktop/mobile 场景均定向通过；全量 Vitest 为 103 files/719 tests，通过 `npm run check` 0 errors/0 warnings，生产构建成功。
- Round 11：已完成本轮验证出口。干净 fixture 完整 E2E 基线为 181/196 passed；剩余 13 项均定位为移动响应式测试契约、hydration 竞态、稳定数据取样或嵌套 landmark 断言，已通过条件化断言、axe 重试、SEO 取样、菜单重试等定向回归；Round 11 定向关键场景全部通过。
- Round 12：已完成最终验证出口。干净 fixture 完整 E2E 193/196 通过；剩余 3 个附件/无 JS 帖子场景在最终 fixture 配置下逐项复跑全部通过；axe、认证、后台、移动、无 JS、SEO、键盘、编辑器、Cover 和消息流程均有通过证据。最终 `npm run check` 为 0 errors/0 warnings，Vitest 为 103 files/719 tests 全通过，`npm run build` 成功。
- 当前非阻塞残余：生产构建仍提示 Tiptap SSR tree-shaking unused-export 和一个约 631KB client chunk；未以提高阈值或隐藏 warning 的方式处理。真实后端、邮件、存储、支付、OAuth、安全和运维准入仍需独立签核。
- 当前已知外部阻塞：测试 persona 会话会过期，需每轮 fixture 重新铸造；真实后端、邮件、存储、支付、OAuth、安全和运维准入仍按 `prototype/ACCEPTANCE.md` 与发布清单独立签核。
