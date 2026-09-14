# BBLBB BLBUI 前端规范

> 版本：v1.0
> 适用范围：`frontend/` 的前台、编辑器、消息和管理后台
> 基线：`@chaos_team/blbui-core@0.0.24` 与 `@chaos_team/blbui-svelte@0.0.24`
> 关联：[`DESIGN-SYSTEM.md`](DESIGN-SYSTEM.md)、[`THEME.md`](THEME.md)

## 1. 目标

BBLBB 使用 BLBUI 作为视觉和交互 token 的唯一基础来源，同时保留论坛必须具备的 SSR、无 JS 导航、原生表单提交和内容可访问语义。组件是否使用 `aui-*`，由运行时能力和业务契约决定，不允许为了“看起来接入”渲染隐藏宿主。

本规范解决四个问题：

- 一套颜色、字体、间距、圆角、焦点环和动效变量。
- 一种组件只有一个真实渲染路径，不允许 hidden host + 本地复制 DOM。
- 页面按功能分区布局，列表、编辑、对话和后台工作台不共用一套卡片模板。
- 加载、空、错误、权限和成功状态必须可区分，并且状态不能只靠颜色表达。

## 2. BLBUI 接入边界

### 2.1 全局初始化

根布局只负责一次注册：

```ts
import { registerAdminElements } from '@chaos_team/blbui-core/register';
registerAdminElements();
```

样式只在 `src/app.css` 顶部引入一次：

```css
@import '@chaos_team/blbui-core/styles.css';
```

业务组件禁止再次调用 `registerAdminElements()`。独立测试若需要验证 Shadow DOM，测试 setup 必须显式注册；普通 SSR/DOM 组件测试只验证组件自己的语义契约。

### 2.2 真实使用的组件

| 组件 | BBLBB 用法 | 约束 |
|---|---|---|
| `aui-card` | `ui/Card.svelte`、`ui/Panel.svelte` 的 L1 模块容器 | 只承载一个模块，不嵌套另一个 L1 |
| `aui-status-tag` | `Badge.svelte`、`Tag.svelte`、导航搜索结果状态 | 状态要有文字，不得只显示颜色 |
| `aui-table` | `Table.svelte` 的可见外壳 | 内容仍为原生 `<table>`，保留 `caption`、`scope` 和无 JS |
| `aui-page-header` | 管理后台 `PageHeader.svelte` | SSR/no-JS 由 `noscript` fallback 提供标题 |
| `aui-spinner` | 全局搜索加载提示 | 不传未声明的 `size` 属性；尺寸由 token/上下文决定 |
| `aui-stat` | 仅在不需要图标、表单和 no-JS 统计快照的场景使用 | 后台统计卡优先使用原生 SSR 适配层 |

`aui-button`、`aui-input`、`aui-dialog`、`aui-pagination` 和 `aui-shell` 当前不直接替换业务控件，原因不是视觉差异，而是 BLBUI 0.0.24 的契约限制：

- `aui-button` 不是外层 `<form>` 的 form-associated control，不能替代提交按钮；它也不支持 `href`、`formaction`、`block` 和项目现有的 `ghost`/三档尺寸。
- `aui-input` 的输入位于 Shadow DOM，没有项目现有的 `label[for]`、无 JS 表单和原生校验契约。
- `aui-dialog` 自带 overlay/focus 生命周期，直接与现有焦点陷阱、Escape、滚动锁并用会产生双重事件。
- `aui-pagination` 是前后页控制，不提供项目的页码链接和 URL/SEO 退化。
- `aui-shell` 强制 viewport 高度和内部滚动，会覆盖后台已有的路由滚动与移动抽屉。

这些适配层必须消费同一组 `--aui-*` token，且不得渲染 hidden AUI 节点。

## 3. Token 规范

### 3.1 CSS 顺序

```text
blbui styles.css
fonts.css
 tokens.css       ← BBLBB 语义 token + --aui-* 亮/暗映射
base.css
layout.css
components.css
pages.css
visual-overrides.css
prototype-app.css
prototype-workflows.css
chinese-elegance.css
 theme-tokens.css  ← 数据型主题同时映射 --color-* 与 --aui-*
 theme-layout.css
 surface.css
```

`tokens.css` 是默认颜色的唯一来源。页面、组件和旧原型层不得声明新的 `--color-*`，不得写死颜色 hex。数据型主题必须同时投影 `--color-*` 和 `--aui-*`，否则 Shadow DOM 会停留在默认主题。

### 3.2 语义变量

- 画布：`--color-bg-page` / `--aui-bg`
- 模块面：`--color-bg-card` / `--aui-surface`
- 浮层面：`--color-bg-raised` / `--aui-surface-elevated`
- 主文字：`--color-text-primary` / `--aui-text-primary`
- 次文字：`--color-text-secondary` / `--aui-text-secondary`
- 边框：`--color-border` / `--aui-border`
- 当前态和主操作：`--color-brand` / `--aui-primary`
- 成功、警告、危险：`--color-success`、`--color-warning`、`--color-danger` 及其 AUI 对应变量
- 全站基础字体：`--font-family-base`（自托管 Inter Tight + 系统 CJK），`--aui-font-ui` 映射至此变量
- 全站等宽与元信息：`--font-family-mono`（自托管 IBM Plex Mono），`--aui-font-mono` 映射至此变量，数值列配 `font-variant-numeric: tabular-nums`
- 全站人文衬线：`--font-family-serif`（Georgia / 宋体 / 思源宋体）

强调色只用于当前态、未读、主操作、状态和活跃度脊柱。正文链接使用 `--color-link` 加下划线。全站禁用硬编码 font-family，统一消费 `--font-family-*` 规范变量。

## 4. 组件契约

### 4.1 按钮

| 变体 | 用途 | 规则 |
|---|---|---|
| `primary` | 当前页面最重要的提交/发布动作 | 一个视区最多一个主要动作 |
| `secondary` | 取消、返回、次要确认 | 透明底、边框、无阴影 |
| `ghost` | 行内辅助操作 | 不抢主操作层级，hover 才出现控制面 |
| `danger` | 删除、撤销、停用 | 必须有明确危险文案或确认步骤 |

统一尺寸：`sm` 适用于表格行和工具条，`md` 是默认，`lg` 只用于认证页或首要提交。最小触控高度 44px；密集后台控件可使用 32px，但外围点击区域必须不小于 44px。

`ui/Button.svelte` 对业务保留原生 `<button>`/`<a>`，样式完全消费 `--aui-*`，不渲染隐藏 AUI 节点。所有表单提交必须保持 `type`、`formaction`、原生浏览器行为。

### 4.2 输入和选择

输入必须有：

- 可关联的 `label[for]`；
- 稳定 `id`、`name` 和 `autocomplete`；
- 错误时 `aria-invalid="true"` 与 `role="alert"`；
- 提示时 `aria-describedby`；
- 移动端不触发浏览器缩放（字体至少 16px 或使用 AUI 输入 token）。

`Input.svelte` 和 `Select.svelte` 是原生语义适配层，视觉值必须跟随 AUI token。

### 4.3 容器和列表

- `Panel` / `Card`：一个列表或功能模块一个 L1。
- `PanelSection`：L1 内分组，用 mono 章节标签和发丝线，不套第二个卡片。
- `ListRow`：L2 条目，整行 hover，最后一行去底线。
- `MetricGroup`：统计数字用发丝线分组，不做一排小卡片。
- `Meta`：时间、计数、板块名、价格、ID 和表头统一 mono + tabular nums。

页面只传内容和状态，不传颜色、border、radius 的 style escape hatch。变体使用 TypeScript 枚举 prop，不拼任意类名。

### 4.4 浮层

下拉、对话框、Toast、用户 hover card 和移动抽屉才允许使用 L3 表面：`--color-bg-raised`、边框和 `--shadow-pop`。内容模块不得使用 box-shadow，条目不得使用独立卡片背景。

## 5. 页面布局

### 前台

论坛前台禁止使用 `app-route-head`、kicker + 大标题 + 描述的后台式页头块。页面标题只保留为无障碍标题；首屏空间优先给内容流、筛选、发布动作、板块索引或资料内容。

- 首页/发现：主内容为一个信息流模块；侧栏使用无框导航和章节标签。
- 板块列表：单个列表容器 + 多条 `BoardRow`，禁止三列卡片阵列。
- 板块详情：板块说明无框，帖子列表单一 L1；排序链接使用真正的链接，不伪装成 JS tab。
- 帖子详情：正文是阅读区；回复是单一 L1；作者信息使用章节分隔。
- 消息：会话列表和对话区各自一个 L1；移动端切换会话后隐藏另一列，composer 保持可见。
- 编辑器：主编辑区 + 设置侧栏；发布设置使用章节分组，不能在卡片内再套卡片。
- 认证页：允许一个居中 L1，因为登录/注册本身是聚焦任务。
- 商品、衣柜、结算：允许商品卡片网格，这是商品比较语义，不适用于论坛列表。

### 管理后台

- 左侧模块导航，右侧单一滚动内容区。
- 页头必须包含标题、描述和右侧动作。
- 筛选、表格、统计和批量动作顺序固定为：`PageHeader → FilterBar → Metric/State → Table/List`。
- 破坏性操作必须是 `danger` 按钮并通过确认对话框完成。
- 移动端侧栏变为抽屉；内容区不能同时出现第二个固定滚动容器。

## 6. 状态和交互

每个异步功能都要区分：

1. 初始加载：`role="status"`，说明正在加载什么。
2. 成功但为空：说明为什么为空，并给出一个下一步动作。
3. 请求失败：`role="alert"`，说明失败对象，给出重试或返回动作。
4. 无权限：说明需要什么权限，不伪装成空数据。
5. 提交中：按钮 `disabled` + `aria-busy` 或明确进行中文案。
6. 成功：使用与动作同名的 toast，例如“已发布”对应“发布”。

状态不可只靠颜色；所有状态都必须有文字、图标或结构变化。

## 7. 提交前检查

- [ ] 没有 `display:none` 的 AUI 占位节点。
- [ ] 一个组件只有一条真实渲染路径。
- [ ] 亮色和暗色下原生控件、AUI Shadow DOM 使用同一语义色。
- [ ] 表单仍可 SSR、无 JS 提交和浏览器回退。
- [ ] 列表没有 L1 套 L1，也没有条目级卡片阵列。
- [ ] 390px 下标题、按钮和表格不溢出；触控目标符合页面密度规则。
- [ ] 键盘焦点可见，Escape、Tab、返回焦点行为可验证。
- [ ] 加载、空、错误、权限状态互不混淆。
- [ ] `npm run check`、DOM 组件测试和 SSR 测试通过。
