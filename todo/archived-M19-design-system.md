# M19 设计系统重做（Design System · 冷墨）[已归档/取消]

> **状态：已取消（2026-09-12 产品所有者确认当前主题符合需求，取消全站三层表面重做与 Panel 组件重构，归档供参考）。**
> 总索引：[`../TODO.md`](../TODO.md)
> 通用规则和证据格式见 [`M00-M02-foundation.md`](M00-M02-foundation.md)。
> 本文件所有 checkbox 都是唯一叶子任务；工作包标题和出口门槛不重复计数。
>
> **规范基线：** [`../docs/DESIGN-SYSTEM.md`](../docs/DESIGN-SYSTEM.md)（三层表面纪律、
> 配色/字体、组件封装规范）。任何实现与规范冲突时以规范为准；需要偏离必须先改规范。
>
> **来源：** 2026-09-08 默认主题视觉复审——「卡片太多，不像次世代论坛」。
> 逐页拆解结论：病因不是「卡片存在」（Discourse/linux.do、V2EX 都有卡片），
> 而是**卡片粒度错误**（条目级卡片阵列）、**盒子嵌套**（L1 套 L1）、**层级缺失**。
> 详见 `docs/DESIGN-SYSTEM.md` §1.1。
>
> **范围：** 纯前端样式与组件封装，不涉及后端、OpenAPI、数据库契约。
> 活跃度脊柱消费已有 `PostSummary.reply_count/like_count/created_at`，无新增接口。
>
> **注册：** 已在 `scripts/check-roadmap.rb`（`EXPECTED_TASK_FILES` / `MILESTONE_FILES` /
> `EXPECTED_MILESTONES`）与 `TODO.md` §1/§5 仪表盘登记；`make check-roadmap` 通过
> （862 叶子任务 / 107 工作包）。

---

<a id="m19"></a>

# M19：默认主题重做与组件封装

**完成定义：** 全站（前台 21 页 + 后台 29 页）符合 `docs/DESIGN-SYSTEM.md` §2 三层表面纪律；
L1 容器一律由组件提供（手写 `app-card` 归零）；亮/暗 × 桌面/390px 四组截图复核通过；
`npm run check` 与 `vitest run` 全绿；44 个无 JS 测试不回归。

---

## M19-FOUND：样式基础层

**元数据：** `P1` · `owner=agent/frontend` · `risk=high` · `depends=none` · `blocked=none`
**目标文件：** `frontend/src/lib/styles/{tokens,fonts,surface,chinese-elegance,theme-tokens}.css`、`frontend/src/app.css`、`frontend/src/app.html`、`frontend/src/lib/theme.ts`
**验收：** 颜色 Token 唯一来源为 `tokens.css`；`surface.css` 三层纪律生效；数据型主题切换不破；CSS 层序测试通过。
**风险说明：** `chinese-elegance.css` 色板删除是本里程碑影响面最大的单点改动——它此前在 `tokens.css` 之后重声明整套暖米色板，导致 `tokens.css` 颜色永不生效（历次「调色没反应」的根因）。回归可对照 `/tmp/ce-orig.bak` 或 git 历史。

- [x] `M19-FOUND-01` `P1` `[30m]` 新建 `fonts.css` 自托管 Inter Tight Variable + IBM Plex Mono（仅 latin/latin-ext 子集，CJK 走系统字体），在 `app.css` 最前引入。证据：files=frontend/src/lib/styles/fonts.css,frontend/src/app.css,frontend/package.json；commands=npm install @fontsource-variable/inter-tight @fontsource/ibm-plex-mono；curl 验证 dev server 已投影 @font-face；contract=none；commit=pending；review=none
- [x] `M19-FOUND-02` `P1` `[45m]` `tokens.css` 重写为冷墨色板（日/夜/系统跟随三分支）+ 字体族 + 圆角收紧 + 阴影仅留浮层；新增 `--color-bg-raised` / `--color-link` / `--color-link-hover` / `--meta-letter-spacing` / `--label-letter-spacing`。证据：files=frontend/src/lib/styles/tokens.css；commands=playwright 探针确认 body/卡片计算色为新值；contract=none；commit=pending；review=none
- [x] `M19-FOUND-03` `P0` `[30m]` 删除 `chinese-elegance.css` 的重复色板（`:root` 与 `html[data-theme="dark"]` 两个颜色块），仅保留几何桥接变量；确立「颜色 Token 唯一来源是 tokens.css」。证据：files=frontend/src/lib/styles/chinese-elegance.css；commands=grep 确认 F5F3EE/5A6C7D/FBFAF7 全部消失；playwright 探针确认 tokens.css 颜色开始生效；contract=none；commit=pending；review=none
- [ ] `M19-FOUND-04` `P1` `[30m]` `tokens.css` 将 `--color-bg-card` / `--color-bg-raised` 改回**独立于画布**的白（亮 `#FFFFFF`）与深灰（暗 `#14181E` / `#171C23`）——L1 卡片成立的前提；当前与画布同值属上一轮「全站去卡片化」的过度设计。
- [ ] `M19-FOUND-05` `P0` `[60m]` 重写 `surface.css`：从「摊平一切」改为 `docs/DESIGN-SYSTEM.md` §2.1 的 L1/L2/L3 三层纪律（L1 有边框圆角无阴影、L2 仅发丝线、L3 唯一准许阴影），文件头写入纪律规范注释与禁止清单。
- [ ] `M19-FOUND-06` `P1` `[30m]` `surface.css` 补 §2.2 章节标签、§2.4 元信息 mono + `tabular-nums` 全站映射（时间/计数/表头/序号/价格/配额/ID）。
- [ ] `M19-FOUND-07` `P1` `[20m]` 遗留 24 处 `html[data-theme="dark"]` 旧夜间选择器并入 `html.dark`，否则暗色下这些组件不跟随。
- [x] `M19-FOUND-08` `P1` `[15m]` `theme-tokens.css` 补 `--color-bg-raised` / `--color-link` / `--color-link-hover` 映射，保证数据型主题不破。证据：files=frontend/src/lib/styles/theme-tokens.css；commands=待随 FOUND-05 一并跑 vitest theme-tokens-css.test.ts；contract=none；commit=pending；review=none
- [x] `M19-FOUND-09` `P2` `[15m]` `app.html` 与 `lib/theme.ts` 的 `theme-color` meta 同步新色板（亮 `#F7F8F8` / 暗 `#0D1014`）。证据：files=frontend/src/app.html,frontend/src/lib/theme.ts；commands=grep 确认两处一致；contract=none；commit=pending；review=none
- [ ] `M19-FOUND-10` `P1` `[20m]` `surface.css` 追加到 `app.css` 末尾并确认层序断言通过（`chinese-elegance < theme-tokens < theme-layout < surface`）。

**出口门槛：** `npx vitest run theme-tokens-css.test.ts theme-layout-shell.test.ts` 全绿；playwright 探针确认 L1 卡片面与画布异色、L2 行无边框、L3 有阴影。

---

## M19-COMP：组件封装

**元数据：** `P1` · `owner=agent/frontend` · `risk=high` · `depends=M19-FOUND` · `blocked=none`
**目标文件：** `frontend/src/lib/components/ui/`、`frontend/src/lib/components/forum/`
**验收：** L1 的 `border`/`radius`/`background` 只出现在组件内部一处；新组件均有单测；组件内零 `!important`、零 hex。
**背景（实测采纳率）：** `ui/Card.svelte` 仅 1 处引用，而 `app-card` 手写 **324** 次；`Pagination`/`Input`/`Select`/`SectionHeader`/`BoardCard`/`ArticleCard` 引用为 **0**。容器类封装名存实亡是「改一处修不完全站」的根因。

- [ ] `M19-COMP-01` `P1` `[45m]` 新建 `ui/Panel.svelte`（L1 容器）：`title` / `actions` / `footer` 插槽 + `variant: default|plain|flush` 枚举；不提供 `style` prop，`class` 仅布局用；样式写组件内 scoped `<style>`。
- [ ] `M19-COMP-02` `P1` `[30m]` 新建 `ui/PanelSection.svelte`（L1 内部分组：mono 章节标签 + 发丝线），**代替嵌套 L1**。
- [ ] `M19-COMP-03` `P1` `[30m]` 新建 `ui/ListRow.svelte`（L2 条目行）：发丝线 + hover 整行浅底，`href` 可选整行可点，最后一行自动去线。
- [ ] `M19-COMP-04` `P1` `[30m]` 新建 `ui/Meta.svelte`（元信息片段）：mono + `tabular-nums` + 分隔点，取代散落内联 `style`。
- [ ] `M19-COMP-05` `P1` `[30m]` 新建 `ui/MetricGroup.svelte`（统计数字组，发丝线分隔而非一排小盒子），取代 `app-stat` / `app-account-card`。
- [ ] `M19-COMP-06` `P1` `[45m]` 新建 `forum/TopicRow.svelte`（帖子行 = 头像 + 标题 + Meta + **活跃度脊柱**）；脊柱明度由 `reply_count`/`like_count`/时间衰减派生，纯函数可单测。
- [ ] `M19-COMP-07` `P1` `[30m]` 新建 `forum/BoardRow.svelte`（板块行），取代 `app-board-grid` 卡片阵列。
- [ ] `M19-COMP-08` `P2` `[20m]` `ui/Card.svelte` 标记 deprecated 并指向 `Panel`；删除零引用的 `SectionHeader.svelte` / `BoardCard.svelte` / `ArticleCard.svelte`。
- [ ] `M19-COMP-09` `P1` `[45m]` 新组件单测（渲染 + 变体枚举 + a11y 语义 + 脊柱强度纯函数边界值），并加一条**结构守卫测试**：断言 `src/routes/**` 不再出现手写 `class="app-card"`。

**出口门槛：** `npx vitest run` 组件测试全绿；结构守卫测试通过；`npm run check` 0 错误。

---

## M19-FRONT：前台页面改造

**元数据：** `P1` · `owner=agent/frontend` · `risk=medium` · `depends=M19-COMP` · `blocked=none`
**目标文件：** `frontend/src/routes/`（前台 21 页）
**验收：** 各页符合 `docs/DESIGN-SYSTEM.md` §5.1 约定；无 JS 基线与 a11y 不回归。

- [ ] `M19-FRONT-01` `P1` `[60m]` 首页 `/`：中列改单个 `Panel`（工具条并入顶部成一体），左栏板块导航**去框**为纯链接 + mono 计数，右栏无框 + 章节标签分节，帖子行换 `TopicRow`。
- [ ] `M19-FRONT-02` `P1` `[30m]` `/discover` 复用首页同一套组件与布局约定。
- [ ] `M19-FRONT-03` `P1` `[45m]` `/boards`：3 列 `app-board-grid` 卡片阵列 → **单个 `Panel` + `BoardRow`**（最像 AI 的一页，优先级最高）。
- [ ] `M19-FRONT-04` `P1` `[30m]` `/boards/[slug]`：板块信息条无框 + 单个 `Panel` 帖子列表。
- [ ] `M19-FRONT-05` `P1` `[60m]` `/posts/[id]`：阅读区**去框**（正文即页面，放宽阅读宽度），回复列表一个 `Panel`，右栏作者信息无框 + `PanelSection` 分节。
- [ ] `M19-FRONT-06` `P1` `[45m]` `/tags`、`/tags/[slug]`、`/search`、`/favorites` 统一 `Panel` 列表容器。
- [ ] `M19-FRONT-07` `P1` `[30m]` `/notifications`、`/achievements` 统一 `Panel` + `ListRow`。
- [ ] `M19-FRONT-08` `P1` `[45m]` `/me`、`/users/[username]`：资料头去框，统计换 `MetricGroup`（消除三个小盒子）。
- [ ] `M19-FRONT-09` `P2` `[30m]` 认证 5 页（`/login`、`/register`、`/password-reset`、`/password-reset/confirm`、`/verify-email`、`/mfa`）：**保留居中卡片**，去阴影、收紧圆角、mono eyebrow。
- [ ] `M19-FRONT-10` `P2` `[30m]` `/editor`：主编辑区 `Panel`，侧栏设置项无框 `PanelSection` 分节。
- [ ] `M19-FRONT-11` `P2` `[20m]` `/messages`：两栏 `Panel`（此处卡片语义正确，仅统一几何）。
- [ ] `M19-FRONT-12` `P2` `[30m]` `/shop`、`/shop/[id]`、`/marketplace`、`/me/*`：商品**保留卡片网格**（电商语境卡片正确），去阴影、统一圆角、价格走 mono。

**出口门槛：** 前台各页亮/暗 × 1440px/390px 截图复核；`npx vitest run` 无 JS 测试全绿。

---

## M19-ADMIN：后台收敛

**元数据：** `P1` · `owner=agent/frontend` · `risk=low` · `depends=M19-COMP` · `blocked=none`
**目标文件：** `frontend/src/lib/styles/prototype-app.css`、`frontend/src/lib/styles/surface.css`、`frontend/src/lib/components/admin/`
**验收：** 29 个 `/admin/*` 页整体收敛，无逐页手写。
**策略：** 后台共用 `app-card` / `app-table` / `app-stat` / `app-admin-side` 四个类，改类即整体生效。

- [ ] `M19-ADMIN-01` `P1` `[30m]` `app-card` 收敛为 L1（去阴影、统一圆角、头部改 mono 章节标签）。
- [ ] `M19-ADMIN-02` `P1` `[30m]` `app-table`：表头 mono 大写小字无底色、行发丝线、数字列 `tabular-nums`。
- [ ] `M19-ADMIN-03` `P1` `[30m]` `app-stat` / `app-account-card` **去盒**改 `MetricGroup`（消除盒中盒）。
- [ ] `M19-ADMIN-04` `P1` `[20m]` `app-admin-side` 去框，当前项左侧强调色竖线。
- [ ] `M19-ADMIN-05` `P2` `[30m]` 抽查 `/admin`、`/admin/users`、`/admin/settings`、`/admin/themes` 四页并修正遗留盒中盒。

**出口门槛：** 后台 4 页截图复核；`npx vitest run admin-*` 全绿。

---

## M19-VERIFY：全量验证

**元数据：** `P0` · `owner=agent/frontend` · `risk=medium` · `depends=M19-FRONT,M19-ADMIN` · `blocked=none`
**目标文件：** `frontend/src/lib/testing/`、`reports/design-system/`
**验收：** 全量测试绿、四组截图归档、规范检查清单逐条通过。

- [ ] `M19-VERIFY-01` `P0` `[30m]` `npm run check`（svelte-check + tsc）0 错误。
- [ ] `M19-VERIFY-02` `P0` `[30m]` `npx vitest run` 全量通过（重点：44 个 SSR/无 JS 测试 + `theme-tokens-css` 层序断言）。
- [ ] `M19-VERIFY-03` `P1` `[45m]` Playwright 截图归档到 `reports/design-system/`：亮/暗 × 1440px/390px × 主干 8 页，与改造前对照。
- [ ] `M19-VERIFY-04` `P1` `[30m]` a11y 复核：`:focus-visible` 焦点环、触控目标 ≥44px、`prefers-reduced-motion`、状态不只靠颜色（WCAG 1.4.1）。
- [ ] `M19-VERIFY-05` `P1` `[20m]` 数据型主题回归：自定义主题（含 sidebar/wide 结构预设）切换后三层纪律仍成立。
- [ ] `M19-VERIFY-06` `P2` `[20m]` 按 `docs/DESIGN-SYSTEM.md` §6 检查清单逐条自查并记录结果。

**出口门槛：** 上述全部 `[x]`；`reports/design-system/REPORT.md` 记录前后对照与遗留项。

---

## 依赖关系

```
M19-FOUND ──► M19-COMP ──┬──► M19-FRONT ──┐
                         └──► M19-ADMIN ──┴──► M19-VERIFY
```

## 统计

| 工作包 | 叶子任务 | 已完成 | 预估 |
|---|---|---|---|
| M19-FOUND | 10 | 5 | ~4.5h |
| M19-COMP | 9 | 0 | ~5h |
| M19-FRONT | 12 | 0 | ~7.5h |
| M19-ADMIN | 5 | 0 | ~2.5h |
| M19-VERIFY | 6 | 0 | ~3h |
| **合计** | **42** | **5** | **~22.5h** |
