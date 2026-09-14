# BBLBB — 手机端半屏弹层（Bottom Sheet）交互标准

> 状态：Implementation（2026-09 确立）
> 负责领域：前端/设计
> 事实来源：本文件。所有「手机端锚定浮层/工具面板/明细弹窗」一律按本规范执行；
> 样式事实来源为 `frontend/src/lib/styles/mobile.css` 的「§ 手机端半屏弹层」段，
> 手势事实来源为 `frontend/src/lib/utils/sheet-drag.ts`。
> 交互基准：iOS HIG Sheets（medium/large detent）+ Android Material Modal Bottom Sheet。

## 1. 目的与范围

手机端（触屏、窄视口）没有 hover，锚定式小浮层（如发帖页「发布设置」）存在命中
面积小、易被软键盘/底部导航遮挡、无法滑动关闭等问题。本规范把全站同类浮层统一为
**底部上滑的半屏 Bottom Sheet**：全宽贴底、约半个屏幕高度、带遮罩与抓手、
支持下滑关闭，与安卓/iOS 原生交互一致。

**适用**：由触发按钮锚定展开的工具面板、表单面板、明细列表弹窗、表情选择器等
`role="dialog"` 浮层（桌面端当前以 absolute/fixed 锚定小卡片呈现的）。

**不适用**（保留原交互）：

| 类型 | 理由 |
|---|---|
| toast / 内联 tooltip | 非模态、瞬时反馈，无关闭语义 |
| 下拉菜单（`role="menu"`，如行「⋮」操作菜单、导航 dropdown） | 菜单 = 选择即刻关闭，半屏反而增加操作距离 |
| 居中模态 `Dialog`（`ui/Dialog.svelte`） | 已有焦点陷阱/滚动锁契约的确认/表单模态，形态独立治理 |
| 桌面端（≥768px） | 保持现有锚定浮层，本规范全部规则不生效 |

## 2. 规范条款

### 2.1 触发断点

- 手机端断点统一为 `@media (max-width: 767px)`（与 `mobile.css` 现行移动断点一致）。
- 手势/JS 侧使用同一常量 `SHEET_MEDIA_QUERY = '(max-width: 767px)'`（`utils/sheet-drag.ts` 导出），
  不允许出现第二套断点字符串。

### 2.2 形态与尺寸

| 条款 | 值 |
|---|---|
| 定位 | `position: fixed; left: 0; right: 0; bottom: 0; top: auto`，全宽贴底 |
| 高度 | **固定 `50dvh`**（半个屏幕高度左右）；JS 内联定位值一律以 `!important` 压过 |
| 内部结构 | 弹层为 flex 列：抓手 / 头部（固定）→ 内容区（`flex: 1` + `overflow-y: auto`，弹层内滚动） |
| 表面 | `--color-bg-raised`；`border: 0` + `border-top: 1px solid var(--color-border)` |
| 圆角 | 顶部两角 `20px 20px 0 0`，底部直角贴底 |
| 阴影 | `0 -12px 36px rgb(0 0 0 / 42%)`（向上投影，表达「浮于页面之上」） |

### 2.3 遮罩（scrim）

- 每个弹层配一个共享类 `.app-sheet-backdrop` 的关闭按钮（真实 `<button>`，可聚焦，`aria-label="关闭弹层"`）。
- `position: fixed; inset: 0; z-index: var(--z-modal, 400)`，弹层本体 `z-index: var(--z-modal, 400) + 1`。
- 背景 `color-mix(in srgb, var(--color-bg-page) 68%, transparent)` + `backdrop-filter: blur(2px)`，
  入场 fade 180ms（复用 `user-sheet-fade-in`）。
- 点击 scrim 关闭弹层；桌面端 base 规则 `display: none`，元素存在也不渲染。
- 打开期间锁 body 滚动：`html:has(.app-sheet-backdrop) body { overflow: hidden !important }`
  （与 `.user-sheet-backdrop` 同一模式，防滚动穿透）。

### 2.4 抓手（grabber）与下滑关闭手势

- 弹层第一个子元素为共享类 `.app-sheet-grab` 容器（`aria-hidden="true"`），
  视觉条 `38×4px`、圆角 `--radius-full`、`--color-border-strong`，居中。
- 手势仅从抓手/头部区域发起：容器 `touch-action: none; cursor: grab`，页面滚动不受影响。
- 跟手：向下滑动（delta > 0）时弹层 `translateY(delta)` 实时跟随；向上回推禁止越顶（offset 钳制为 0）。
- 松手判定：位移 > `72px` 调用 `onClose` 关闭；否则回弹复位（200ms ease-out）。
- 关闭为即时卸载（与现有 `.user-menu` sheet 一致，不做退场动画）。

### 2.5 动效

| 项 | 值 |
|---|---|
| 弹层入场 | `translateY(100%) → 0`，240ms `cubic-bezier(0.16, 1, 0.3, 1)`（复用 `user-sheet-slide-up`） |
| scrim 入场 | opacity fade 180ms ease-out |
| 降级 | `prefers-reduced-motion: reduce` 下动效交由全局精简层处理，不得新增超时长动画 |

### 2.6 安全区

- 弹层底部预留 `env(safe-area-inset-bottom)`（手势条机型不贴边）：
  容器 `padding-bottom: max(10px, env(safe-area-inset-bottom, 0px))`。
- 顶部无 notch 预留（弹层起点在屏幕中部，不与状态栏相交）。

### 2.7 无障碍（沿用 M14-A11Y 契约）

- 弹层 `role="dialog"`；手机端（模态形态）补 `aria-modal="true"`，桌面锚定浮层不设。
- 关闭途径 ≥ 3：抓手/头部下滑、点击 scrim、关闭按钮（`aria-label`）+ Esc（既有键路保留）。
- 焦点管理：**新建独立组件**必须实现 Dialog 组件契约（打开焦点入内、Tab 陷阱、关闭焦点归还）；
  既有浮层本次统一外观与手势，焦点治理随各自组件迭代补齐。

## 3. 实现契约

| 契约 | 位置 | 说明 |
|---|---|---|
| 样式段「§ 手机端半屏弹层」 | `lib/styles/mobile.css` 文件末段 | scrim/grab/各浮层 sheet 化规则，全部 ≤767px 生效、≥768px `display:none` |
| `sheetDrag(node, { onClose, threshold? })` | `lib/utils/sheet-drag.ts` | Svelte action，绑定在 `.app-sheet-grab` 上；桌面自动 no-op |
| `SHEET_MEDIA_QUERY` | 同上 | 唯一断点常量，CSS 与 JS 共享同一值 |
| `.app-sheet-backdrop` / `.app-sheet-grab` | 共享类 | 组件只渲染共享类节点，不得为单个浮层另造 scrim/抓手样式 |

组件接入清单（新增浮层时对照）：

```svelte
<div class="composer-popover" role="dialog" aria-modal={isNarrow ? 'true' : undefined} ...>
  <div class="app-sheet-grab" aria-hidden="true" use:sheetDrag={{ onClose: closePanel }}></div>
  ...头部/内容...
</div>
{#if panelOpen}
  <button type="button" class="app-sheet-backdrop" aria-label="关闭" onclick={closePanel}></button>
{/if}
```

## 4. 已接入清单（2026-09）

| 浮层 | 组件 | 备注 |
|---|---|---|
| 发布设置 / 视频引用 / AI 助手 | `editor/TopicComposer.svelte` | 发帖页 3 个 `.composer-popover` |
| 收到的表情（明细弹窗） | `ReactionBar.svelte` | portal 到 body 的 JS 定位浮层 |
| 添加互动表态（表情选择器） | `ReactionBar.svelte` | 同上 |
| 用户资料卡（窄屏） | `UserCard.svelte`（`.user-card-sheet`） | 由浮动卡改为全宽贴底 + scrim；JS 窄屏断点仍为 640px（保留窄窗口 hover 卡） |
| 导航用户菜单 | `Navbar.svelte`（`.user-menu`） | 已是 Bottom Sheet（`mobile.css §1218`），作为本规范的设计参照 |

## 5. 验收清单

- [ ] ≤767px：打开浮层呈全宽贴底半屏（50dvh）弹层，带 scrim、抓手，内容区弹层内滚动，底部避开安全区。
- [ ] 自抓手向下滑动跟手；>72px 松手关闭，≤72px 回弹；抓手以外区域滚动内容不触发拖拽。
- [ ] 点击 scrim、点击关闭按钮、Esc 均关闭；关闭后页面可滚动（滚动锁释放）。
- [ ] ≥768px：浮层保持原锚定形态，`.app-sheet-backdrop`/`.app-sheet-grab` 不可见、手势无效。
- [ ] `npm run check` 与 `npm run test` 通过（含 `utils/sheet-drag.test.ts`）。
