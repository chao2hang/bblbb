# BBLBB 原型待办事项清单 (todo.md)

本文件系统梳理了 BBLBB 原型系统（57 个页面模板、Hash 路由与 Mock 数据运行时）中目前剩余需要实现、打磨及验证的全部事项。按照功能领域、优先级与执行阶段分层管理。

---

## 阶段一：核心交互与控制层完善 (Priority: High)

### 1. 批量操作工具栏与表格联动
- [x] **全选与多选机制绑定**
  - 为管理后台核心列表页（`admin-users.html`、`admin-articles.html`、`admin-orders.html`、`admin-categories.html` 等）的 `table` 表头提供主控 Checkbox。
  - 行级 Checkbox 状态监听与选中计数器更新 (`selectedCount`)。
- [x] **悬浮批量操作栏状态驱动 (`.batch-actions-bar`)**
  - 当 `selectedCount > 0` 时显示 `.batch-actions-bar.active`，并展示当前已选数量。
  - 实现“批量通过审核”、“批量下架/禁用”、“批量导出 CSV”、“批量删除”按钮的点击事件与二次确认 Modal。
  - 操作成功后触发 Mock 数据局部刷新、清除全选高亮状态，并抛出 Toast 提示。

### 2. 通用弹窗 (Modal) 与抽屉 (Drawer) 流程闭环
- [x] **审核驳回与原因填写弹窗**
  - `admin-articles.html` / `admin-article-audit.html` / `admin-comments.html`：审核驳回时弹出原因表单（快捷理由标签 + 文本域），提交后更新状态为“已驳回”并记录驳回信息。
- [x] **详情查看与快捷编辑抽屉**
  - 用户详情/权限配置抽屉（`admin-users.html`、`admin-roles.html`）。
  - 订单物流与支付凭单抽屉（`admin-orders.html`、`admin-payments.html`）。
- [x] **分类与标签管理弹窗**
  - `admin-categories.html`：支持无刷新添加同级/子级分类、编辑分类名称与排序权重。

### 3. 表格即时检索与筛选联动
- [x] **客户端通用 Filter 模块封装**
  - 支持关键字搜索（防抖 250ms）、状态下拉筛选（全部/待审/正常/禁用）、时间范围筛选。
  - 筛选后实时重算表格分页、无结果时展示 `.empty-state` 占位视图。

---

## 阶段二：专业业务组件与视觉增强 (Priority: Medium)

### 1. 审核变更比对器 (Audit Diff Viewer)
- [x] **文章/草稿版本 Diff 视窗**
  - 在 `admin-article-audit.html` 中实现双栏对比视图（修改前 vs 修改后）。
  - 高亮增删文本差异（绿色新增、红色删除、黄色变更），方便审核人员一目了然。

### 2. 交互状态反馈与空状态优化
- [x] **骨架屏与加载动效**
  - 在大图加载、数据切换时注入 `.skeleton` 骨架占位效果。
- [x] **空状态与异常处理组件**
  - 统一全系统所有页面在搜索无结果、列表为空、权限不足时的空状态展示（插画/图标 + 友好引导文案 + 快捷创建/重置按钮）。

### 3. 数据仪表盘图表交互与周期切换
- [x] **统计指标卡与趋势图表联动**
  - `admin-dashboard.html` / `admin-analytics.html`：日/周/月/年时间跨度切换，触发图表数据重绘与环比增长率动态变化动画。

---

## 阶段三：视觉规范、响应式与深色模式打磨 (Priority: Medium)

### 1. 新中式美学 (Chinese Elegance) 视觉微调
- [x] **色彩与质感一致性校验**
  - 校验全站印章红 (`#c23531`)、竹青色、宣纸暖灰底色与古铜边框渐变在各子页面中的统一应用。
  - 微调宋体/楷体标题与无衬线正文的行高阶梯，消除跨平台中文字符截断与基线漂移。

### 2. 移动端与响应式适配检查
- [x] **小屏幕适配与侧边栏折叠**
  - 移动端视口（< 768px）下管理后台侧边栏收拢为汉堡抽屉菜单。
  - 表格在窄屏下的水平滚动条优化或卡片式瀑布流视图回退。

### 3. 深色模式 (Dark Theme / 墨色风雅) 覆盖
- [x] **高对比度暗色主题校验**
  - 检查管理端和前台深色背景下的卡片边框高亮、文字层级可读性，杜绝纯白硬编码颜色穿透。

---

## 阶段四：端到端验证与质量保障 (Priority: High)

### 1. 路由与页面完整性校验
- [x] **57 个子页面路由遍历自动化检查**
  - 编写并执行自动化巡检脚本，遍历 `prototype/index.html#/...` 下的所有 57 个页面路径，验证其 HTML 模板加载正常、无 404 缺失、无控制台报错。

### 2. Mock 运行时状态持久化
- [x] **localStorage 同步机制**
  - 确保文章增删改、用户状态变更、订单流转在刷新页面后依然保持最新 Mock 状态。
  - 提供“重置测试数据 (Reset Mock Data)”一键快捷操作。

---

*生成时间: 2026-09-02*

## 完成证据（2026-09-02）

- `node verify.mjs`：182/182 通过，失败 0；console、pageerror、requestfailed 均为 0。最近报告：`.verify/spa-2026-09-02-06-45-16/report.json`。
- `node verify-mock-runtime.mjs`：23/23 通过，失败 0；无浏览器错误。最近报告：`.verify/mock-2026-09-02-06-44-23/report.json`。
- 覆盖登录持久化、草稿/发布、付费与回复解锁、通知已读、消息失败、MFA、举报批量处理、积分流水、AI 任务、帖子/用户批量操作、筛选空态、用户 Drawer、文章 Diff、BI 年度周期和移动端管理菜单。
- 当前入口是 `prototype/index.html` 的 Hash SPA；文章审核 Diff 使用 `#admin-article-audit:p-201`。页面和管理数据仍是本地 Mock/localStorage，不代表真实 API、支付、邮件、上传、MFA、OAuth 或生产权限链路已完成。
