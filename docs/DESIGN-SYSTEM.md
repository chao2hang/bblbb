# BBLBB — 设计系统规范「冷墨 / Cold Ink」

> 版本：v1.1
> 适用范围：`frontend/` 全部路由与组件（前台 + 管理后台）
> 关联文档：[`BLBUI-DESIGN-SYSTEM.md`](BLBUI-DESIGN-SYSTEM.md)（BLBUI 组件与实现边界）、[`FRONTEND.md`](FRONTEND.md)（技术基线）、[`THEME.md`](THEME.md)（数据型主题边界）、[`PROTOTYPE-UI.md`](PROTOTYPE-UI.md)（原型视觉来源）
> 状态：M19 全站重做已取消（当前主题符合需求）；本文件与 [`BLBUI-DESIGN-SYSTEM.md`](BLBUI-DESIGN-SYSTEM.md) 作为后续局部组件编写与演进参考。

---

## 1. 为什么要有这份规范

### 1.1 问题诊断

2026-09 全站视觉复审的结论：默认主题「卡片太多，不像次世代论坛」。逐页拆解后，
病因**不是「卡片存在」**——Discourse / linux.do、V2EX 这类成熟论坛都有卡片——
而是卡片缺乏纪律，具体是三条：

| 病因 | 现状证据 | 后果 |
|---|---|---|
| **卡片粒度错误** | `/boards` 用 `app-board-grid` 渲染 3 列卡片阵列，每个板块一个独立盒子 | 一屏 N 个等重矩形，无主次，观感像组件库演示页 |
| **盒子嵌套** | `app-card` 内再放 `app-stat` / `app-account-card` / `app-board-card` | 双层边框互相顶着，视觉噪音翻倍 |
| **层级缺失** | 首页左栏板块卡、工具条、列表框、右栏 2 卡权重相同 | 眼睛没有落点，主内容不突出 |

成熟论坛的共同做法是：**一个列表 = 一个容器，内部行用发丝线分隔**。
Discourse 的 topic-list、V2EX 的主题列表都是「一个框、很多行」，不是「很多个框」。

### 1.2 一个必须记住的历史坑

`chinese-elegance.css` 曾在 `tokens.css` **之后**重新声明整套颜色变量
（`:root { --color-bg-page: #F5F3EE; ... }`）。同优先级后者胜，导致
**改 `tokens.css` 的颜色永远不生效**——这是过去多次"调色没反应"的根因。

**现行约定：颜色 Token 的唯一来源是 `tokens.css`。** 任何其他层不得声明
`--color-*`。新增颜色请改 `tokens.css`，不要在下游层"就地覆盖"。

---

## 2. 设计语言

### 2.1 三层表面纪律（本规范的核心契约）

全站只有三种表面。写新 UI 前先回答：**它是列表容器、条目，还是浮层？**

#### L1 · 列表/模块容器

```css
background: var(--color-bg-card);   /* 与页面画布不同色，卡片得以成立 */
border: 1px solid var(--color-border);
border-radius: var(--radius-lg);
box-shadow: none;                    /* 内容区永不投影 */
```

- 一个列表、一个功能模块 = **一个** L1。
- L1 内部的条目**不得**再有边框/圆角/独立底色。
- **禁止 L1 套 L1**（盒中盒）。需要分组时用 §2.2 的章节标签 + 发丝线。

#### L2 · 条目行（L1 内部）

```css
background: transparent;
border: 0;
border-bottom: 1px solid var(--color-border-muted);  /* 最后一行去掉 */
border-radius: 0;
```

- hover 时整行浅底 `--color-surface-hover`，不做位移/投影/缩放。
- 条目内的次级信息靠字号与颜色降级，不靠再包一个盒子。

#### L3 · 浮层

```css
background: var(--color-bg-raised);
border: 1px solid var(--color-border);
border-radius: var(--radius-md);
box-shadow: var(--shadow-pop);       /* 全站唯一准许阴影的层 */
```

适用且仅适用于：下拉菜单、模态框、Toast、hover 用户卡、抽屉。
它们真的浮在页面之上，阴影表达"这是临时的、可关闭的一层"。

#### 禁止清单

- 内容区任何 `box-shadow`（含 hover 抬升）。
- 条目级卡片阵列（每个数据项一个盒子）。
- L1 嵌套 L1。
- `border` + `background` + `border-radius` 三件套出现在非 L1/L3 元素上。

### 2.2 章节标签代替卡片头

需要表达"这是一组内容"时，**不要**再包一个盒子。用 mono 全大写小标签 + 其下发丝线：

```css
font-family: var(--font-family-mono);
font-size: 11px;
font-weight: 600;
letter-spacing: var(--label-letter-spacing);
text-transform: uppercase;
color: var(--color-text-tertiary);
```

信息密度更高，且不会在页面上堆矩形。

### 2.3 配色

画布是**冷石墨**，不是暖米纸。论坛内容是文字与代码，冷中性底色让正文墨色更黑、
让唯一的强调色更亮；暖米黄会把一切拉向"纸感"，而纸感正是模板化观感的来源。

| 语义 | 亮色 | 暗色 | 用途 |
|---|---|---|---|
| `--color-bg-page` | `#F7F8F8` | `#0D1014` | 页面画布 |
| `--color-bg-card` | `#FFFFFF` | `#14181E` | L1 卡片面 |
| `--color-bg-raised` | `#FFFFFF` | `#171C23` | L3 浮层面 |
| `--color-text-primary` | `#10141A` | `#EDEFF2` | 正文/标题 |
| `--color-text-secondary` | `#545C68` | `#A2AAB6` | 次要文本 |
| `--color-text-tertiary` | `#7A828E` | `#79818D` | 元信息/标签 |
| `--color-brand` | `#2C4BD8` | `#7B92FF` | **仅信号**，见下 |
| `--color-link` | `#1B2430` | `#DCE1E8` | 正文链接 |
| `--color-border` | `#DFE3E7` | `#262C35` | L1 边框 |
| `--color-border-muted` | `#EAEDF0` | `#1C222A` | L2 行分隔 |

**强调色纪律（重要）：** `--color-brand` 是**信号**，不是装饰。只准出现在：

1. 当前态（选中 tab 下划线、侧栏当前项竖线）
2. 未读/待处理（通知角标）
3. 主操作按钮（一屏最多一个）
4. 精华/置顶等**状态**标记
5. 活跃度脊柱（§2.5）

正文链接用 `--color-link`（近墨色）+ 下划线承担可辨识性。
这样强调色出现的地方**一定有含义**，不会退化成品牌涂装。

### 2.4 字体规范（Typography System）

全站字族实行**三级严格规范**，禁止任何页面或组件自行拼写临时 font-family 或引入未受控字体：

| 语义角色 | CSS Token 变量 | 标准字族回退链（Font Stack） | 承担职责与适用边界 |
|---|---|---|---|
| **Base / UI** | `--font-family-base` | `"Inter Tight Variable", "Inter Tight", -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "MiSans", "HarmonyOS Sans SC", "Noto Sans SC", "Microsoft YaHei", sans-serif` | 全站通用界面文本、正文、表单控件、对话框、一般阅读段落 |
| **Display** | `--font-family-display` | `var(--font-family-base)` | 页面主标题、大字号展示文本、Hero 区块（负字距收紧） |
| **Utility / Mono** | `--font-family-mono` | `"IBM Plex Mono", "SF Mono", "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace` | **全站元信息与数据**：时间戳、计数值、板块标示、kicker 标签、序号、表头、层级徽章、金额价格、代码块。必须配 `font-variant-numeric: tabular-nums` |
| **Serif** | `--font-family-serif` | `Georgia, "Songti SC", "Noto Serif SC", serif` | 典雅引语、名言金句、特定人文传统排版装饰 |
| **AUI 桥接** | `--aui-font-ui`<br>`--aui-font-mono` | 映射至 `var(--font-family-base)` 与 `var(--font-family-mono)` | 保证 blbui Shadow DOM 组件与全站字族无缝一致 |

#### 字体发布与性能纪律
- **自托管（`@fontsource`）与零外部请求**：拉丁部分由 `@fontsource-variable/inter-tight` 与 `@fontsource/ibm-plex-mono` 随包本地分发（`fonts.css`），严禁请求 Google Fonts 等第三方 CDN（保障离线可用与零隐私泄漏）。
- **CJK 走系统字族回退**：中文严格由系统 UI 字体渲染（PingFang / MiSans / HarmonyOS / 思源黑 / 微软雅黑），**绝不**为 CJK 下发 Web Font（一个思源黑子集数 MB，首屏首载不可接受）。
- **层级靠字号、字重与字距**：标题使用负字距（`letter-spacing: -0.025em`），元信息与代码标签使用正字距（`0.02em` ~ `0.09em`），禁止通过在页面里随意切不同字体家族来做视觉强调。

#### 严禁事项（Anti-Patterns）
1. **禁止写死字族名**：禁止在组件内写 `font-family: monospace`、`font-family: Georgia, serif`、`"Space Grotesk"` 等硬编码值，必须统一消费 `var(--font-family-*)`。
2. **禁止虚构变量名**：历史上误写的 `--font-family-ui`、`--font-family-body` 已统一纠正为 `--font-family-base`（并在 Token 层保留兜底别名），新代码一律使用规范变量名。
3. **mono 数据纵向对齐**：凡是展示数字、金额、时间的 mono 元素，一律配 `font-variant-numeric: tabular-nums`。

### 2.5 Signature · 活跃度脊柱

帖子行左侧一条 `2px` 竖线，**明度由该帖真实活跃度决定**
（`reply_count` + `like_count` + 时间衰减，数据已在 `PostSummary`）：

```
冷（无回复/陈旧）  →  几乎不可见（--color-border-muted）
热（高回复/近期）  →  强调色实色（--color-brand）
```

它替代"热门"徽章：结构装置承载**真实信息**，而不是装饰。
这是全站唯一的"大胆"之处，其余保持克制——把冒险花在一个地方。

### 2.6 质量底线（不声张，但必须做到）

- 响应式至 390px；触控目标 ≥44px。
- `:focus-visible` 可见焦点环（`--color-focus-ring`），键盘可达。
- `prefers-reduced-motion: reduce` 下关闭过渡与动画。
- 无 JS 基线：SSR 首帧可读可导航（本仓库有 44 个 no-JS 测试锁定）。
- 状态不能只靠颜色表达（WCAG 1.4.1），危险态需文字/形态佐证。

---

## 3. 样式层架构

### 3.1 加载顺序（`app.css`，顺序有测试锁定）

```
fonts.css              自托管字族
tokens.css             ★ 颜色/字体/间距 Token 唯一来源
base.css               reset、排版、焦点、prose
layout.css             容器、栅格、navbar 骨架
components.css         通用组件样式
pages.css              页面级样式
visual-overrides.css   共享打磨层
prototype-app.css      .app-* 类全量定义
prototype-workflows.css
chinese-elegance.css   几何/版式规则（★ 已剥离颜色声明）
theme-tokens.css       数据型主题 --bb-* → 语义变量映射
theme-layout.css       主题结构预设 sidebar/wide
surface.css            ★ 三层表面纪律（最后，压过上游 !important）
```

`theme-tokens-css.test.ts` 断言 `chinese-elegance < theme-tokens < theme-layout`，
新增层不得插入这三者之间。

### 3.2 各层职责边界

| 层 | 可以做 | 不可以做 |
|---|---|---|
| `tokens.css` | 声明 `--color-*` / `--font-*` / `--space-*` | 写选择器规则 |
| `components.css` | 组件类样式，只消费语义变量 | 声明颜色 Token、写死 hex |
| `surface.css` | 用 `!important` 收敛表面/几何 | 写死颜色（必须消费变量） |
| `theme-tokens.css` | `--bb-*` → 语义变量映射 | 组件规则 |

**任何层都不得写死 hex 颜色**（`tokens.css` 除外）。写死会让数据型主题失效。

### 3.3 数据型主题兼容

`surface.css` 只消费语义变量，因此自定义主题切换后纪律依然成立。
主题声明独立 surface 色时，L1 用 `--color-bg-card` 表达；默认主题下它与
画布不同色，卡片成立。新增语义变量必须在 `theme-tokens.css` 补映射
（如 `--color-bg-raised` / `--color-link`），否则自定义主题下会回退到默认值。

---

## 4. 组件封装规范

### 4.1 现状：封装名存实亡

采纳率实测（`grep -rl` 统计引用文件数）：

| 组件 | 引用文件数 | 对应手写类出现次数 |
|---|---|---|
| `ui/Card.svelte` | **1** | `app-card` 手写 **324** 次 |
| `ui/Button.svelte` | 49 | — |
| `ui/EmptyState.svelte` | 32 | — |
| `ui/StatCard.svelte` | 1 | `app-stat` / `app-account-card` 散落 |
| `ui/Table.svelte` | 2 | `app-table` 散落 |
| `ui/Tag.svelte` | 1 | `app-tag` 散落 |
| `ui/Pagination` / `Input` / `Select` | **0** | 全部手写 |
| `SectionHeader` / `BoardCard` / `ArticleCard` | **0** | 全部手写 |

**结论：容器与列表类组件基本没被使用，页面直接手写 `app-*` 类。**
这正是"改一处样式修不完全站"的根因，也是卡片纪律无法靠 CSS 单独守住的原因——
324 个手写点，任何一个都可能长出新盒子。

### 4.2 封装原则

1. **容器必须组件化，不许手写 L1。**
   L1 的 `border` / `radius` / `background` 只准出现在组件内部**一处**。
   页面写 `<Panel>` 而不是 `<div class="app-card">`。
2. **组件不接收样式逃生舱。**
   不提供 `style` prop；`class` 仅用于布局（间距/跨列），不得传颜色与边框。
   （安全上这也与 M14-COMPONENTS-06「白名单 prop 投影」一致。）
3. **变体用枚举 prop，不用类名拼接。**
   `<Panel variant="plain">` 而不是 `class="app-card app-card--plain"`。
   枚举是闭集，可被 TypeScript 与测试锁定。
4. **组件负责语义，页面负责内容。**
   组件保证 `<section>` / `<th scope>` / `aria-*` 正确；页面只传数据。
5. **元信息走统一组件**，保证 mono + `tabular-nums` 不会漏。

### 4.3 需要新增/重做的组件

| 组件 | 层 | 职责 | 替代 |
|---|---|---|---|
| `ui/Panel.svelte` | L1 | 列表/模块容器，`title` + `actions` + `footer` 插槽；`variant: default \| plain \| flush` | 324 处手写 `app-card` |
| `ui/PanelSection.svelte` | — | L1 内部分组：mono 章节标签 + 发丝线（**代替嵌套 L1**） | 盒中盒 |
| `ui/ListRow.svelte` | L2 | 条目行：发丝线 + hover，`href` 可选整行可点 | 手写 `.thread` / `.app-post-row` |
| `ui/Meta.svelte` | — | 元信息片段：mono + `tabular-nums` + 分隔点 | 散落的内联 `style` |
| `ui/MetricGroup.svelte` | — | 统计数字组：发丝线分隔，**不是**一排小盒子 | `app-stat` / `app-account-card` |
| `forum/TopicRow.svelte` | L2 | 帖子行 = 头像 + 标题 + Meta + **活跃度脊柱** | 首页/发现/板块各写一遍 |
| `forum/BoardRow.svelte` | L2 | 板块行（取代 3 列卡片阵列） | `app-board-grid` |

现有 `ui/Card.svelte` 保留但标记为 deprecated，指向 `Panel`；
`SectionHeader` / `BoardCard` / `ArticleCard` 零引用，直接删除或重写为上表组件。

### 4.4 组件样式归属

- 组件样式写在**组件内 `<style>`**（Svelte scoped），只消费语义变量。
- 只有需要跨组件共享的类才进 `components.css`。
- **组件内不得写 `!important`**（`surface.css` 是唯一的收敛层）。
- 组件内不得出现 hex 颜色。

### 4.5 页面改写范式

```svelte
<!-- ❌ 现状：手写容器，随时长出新盒子 -->
<div class="app-card">
  <div class="app-card__head"><h2>最新讨论</h2></div>
  <div class="app-card__body">
    {#each posts as p}
      <div class="app-board-card">…</div>   <!-- 盒中盒 -->
    {/each}
  </div>
</div>

<!-- ✅ 目标：容器与条目都由组件保证纪律 -->
<Panel title="最新讨论">
  {#each posts as post (post.id)}
    <TopicRow {post} />
  {/each}
</Panel>
```

---

## 5. 分页面视觉约定

### 5.1 前台

| 页面 | 约定 |
|---|---|
| `/`、`/discover` | 中列一个 L1（工具条并入其顶部成一体）；左栏板块导航**去框**为纯链接 + mono 计数；右栏无框、仅章节标签分节；帖子行带活跃度脊柱 |
| `/boards` | **单个 L1 + 板块行**（废止 3 列卡片阵列） |
| `/boards/[slug]` | 板块信息条无框 + 单个 L1 帖子列表 |
| `/posts/[id]` | 阅读区**去框**（正文即页面，最大化阅读宽度）；回复一个 L1；右栏作者信息无框 + 发丝线分节 |
| `/tags`、`/tags/[slug]`、`/search`、`/favorites`、`/notifications`、`/achievements` | 统一 L1 列表容器 |
| `/me`、`/users/[username]` | 资料头无框；统计走 `MetricGroup`（消除三个小盒子） |
| 认证页（`/login`、`/register`、`/password-reset*`、`/verify-email`、`/mfa`） | **保留居中卡片**（少数适合卡片的场景），去阴影、收紧圆角、mono eyebrow |
| `/editor` | 主编辑区 L1；侧栏设置项无框分节 |
| `/messages` | 会话列表 L1 + 对话区 L1（此处卡片是对的，保留） |
| `/shop`、`/shop/[id]`、`/marketplace`、`/me/*` | 商品**保留卡片网格**（电商语境卡片是对的），去阴影、统一圆角、价格走 mono |

### 5.2 后台（29 个 `/admin/*`）

后台不逐页手写——它们共用四个类，改类即可整体收敛：

| 类 | 改法 |
|---|---|
| `app-card` | 收敛为 L1（去阴影、统一圆角、头部 mono 章节标签） |
| `app-table` | 表头 mono 大写小字、无底色、行发丝线、数字列 `tabular-nums` |
| `app-stat` / `app-account-card` | **去盒**，改 `MetricGroup`（消除盒中盒） |
| `app-admin-side` | 去框；当前项左侧强调色竖线 |

---

## 6. 变更检查清单

提交视觉相关改动前逐条自查：

- [ ] 新增的盒子属于 L1 或 L3？不是则去掉 `border`/`background`/`radius`。
- [ ] 有没有 L1 套 L1？
- [ ] 内容区有没有 `box-shadow`？
- [ ] 强调色是否只用于信号（当前态/未读/主操作/状态/活跃度）？
- [ ] 元信息是否走 mono + `tabular-nums`？
- [ ] 有没有写死 hex 颜色？（只有 `tokens.css` 准许）
- [ ] 容器是否用组件而非手写 `app-card`？
- [ ] 组件内是否出现了 `!important`？
- [ ] 390px 下是否可用、触控目标是否 ≥44px？
- [ ] 亮色与暗色都验证过？
- [ ] 无 JS 基线是否仍可读可导航？
