# BBLBB — 主题系统规范

> 版本：v0.5（M13 已实现：v1 数据型主题 + 管理 API + 封闭 Token schema；v1.1 日/夜双模式 Token）
> 主题只改变展示，不改变身份、权限、审核、积分或内容可见性。主题分为可安全运行时加载的“数据型主题”和构建时编译的“可信代码型主题”。
> **日/夜双模式（v1.1）**：亮/暗是同一主题的 Token 模式（§9）——6 个可选 `color.*.dark`
> key 承载夜间色板；站点亮色模式（`html.light`）取日间板，暗色模式（`html.dark`）取夜间板，
> 由 `theme-tokens.css` 按模式自动解析。旧主题包不含 `.dark` key 时零迁移（回退日间值）。

## 0. 实现状态（M13-THEME）

- **已实现**：`themes`/`theme_revisions` 三库迁移（0057_theme.sql）；领域层
  `backend/src/theme/mod.rs`（封闭 Token schema 校验、fallback、revision）；
  路由 `GET /api/v1/themes/active`、`GET/PUT /api/v1/me/preferences/theme`
  （If-Match revision + `private, no-store`）、`/api/v1/admin/themes*`
  （上传/默认/设置/删除，admin.manage + reason + recent-auth + 审计）。
- **全站生效与实时预览（M13-THEME-08 / M18-ADMIN-THEMES）**：
  - 根布局（`+layout.server.ts`）并行获取活跃主题并注入数据树；
  - 根布局（`+layout.svelte`）监听活跃主题，自动调用 `applyThemeTokens()` 并设置 `data-theme-custom="true"`；
  - 全局样式最后覆盖层（`chinese-elegance.css`）将 `data-theme-custom` 与 `data-theme-preview` 映射为高优先级 CSS 变量，确保背景、卡片、导航栏、侧边栏、按钮完全响应主题色彩变换；
  - 管理后台提供悬浮全局实时预览条、UI 组件库效果展示弹窗（Showcase）及安全退出预览机制；
  - 官方预置库完整收录 BBLBB 原生默认配色（经典赤墨与水墨青石），基准 Token 与品牌色一致。
- **封闭 Token schema（v1.1，21 key）**：15 个基础 key（颜色/字体/圆角/密度/**页面结构**/
  阴影/动效）+ 6 个可选 `color.*.dark` 夜间变体（与日间 key 同构的标准 hex 校验；
  纯新增、向后兼容——逐 key 校验，缺失即回退）；值级校验拒绝 CSS、HTML、JS、SVG、
  远程资源与任意 style 字符串；未知 key 拒绝；资产路径只允许相对路径
  （无 `..`/绝对路径/URL）。
- **日/夜双模式渲染（v1.1）**：
  - `theme-tokens.css`（app.css 最后颜色层）是 `--bb-*` → 语义变量的**唯一映射点**：
    区块 A 日间解析、区块 B 夜间解析（`html.dark` 作用域，`.dark` 变体优先 →
    缺失回退日间值 → 再回退内置官方夜色板）、区块 C 日/夜共用全量语义映射
    （颜色派生/字体/圆角/密度/阴影/动效）、区块 D 减少动效 0 时长；
  - 属性所有权：`dataset.theme`（light/dark）与 light/dark class 归日夜模式系统
    （`$lib/theme.ts` + app.html 防闪烁脚本）所有；数据型主题名写入独立的
    `data-theme-name`，投影器绝不覆盖日夜状态（`html[data-theme="dark"]` 语义保持）；
  - 派生安全变量（`--bb-density-scale`/`--bb-shadow-control|pop|modal`/
    `--bb-motion-duration`/`--bb-*-dark`）由 projection 从**代码内封闭映射**生成
    （值来自 closed 枚举，绝不承载用户原始输入）——`radius.control/card`、
    `space.density`、`shadow.card`、`motion.duration`、`motion.reduced` 五个
    Token 由此真正驱动已编译 CSS 的 `--radius-*`/`--space-*`/`--shadow-*`/
    `--duration-*`。
- **页面结构预设（layout.mode）**：主题可声明整站结构变体
  `classic`（经典顶栏）/`sidebar`（左侧竖栏导航）/`wide`（宽幅容器 + 多列网格）。
  预设是**前端已编译的闭集**（后端 `LAYOUT_MODE_ALLOWLIST` 与前端
  `theme-layout.css` 一一对应）：主题数据只能提供 enum 值，不能注入选择器、
  HTML、CSS 或 JS；缺失/非法值回退 `classic`；移动端（<1024px）一律回退
  经典结构（BottomNav/触控目标不变）。SSR 首帧由根布局写入
  `.app-shell[data-theme-layout]`，预览与正式切换共用同一投影函数。
- **Fallback 与重置机制**：主题不存在/不兼容/停用/损坏 → 回退内置 `default`（revision=1）
  并记录非敏感告警；损坏主题自动标记 `corrupt`；设为默认目标为 `default` 时自动取消所有自定义主题的 `is_default` 标记。
- **内置 default 持久化种子**：启动时 `theme::ensure_default_theme`（`main.rs`，
  INSERT OR IGNORE 幂等）把内置 `default` 主题（`default_tokens()`，active、revision=1，
  含 revision 1 修订记录）写入 `themes` 表——后台「主题管理」始终有一条可编辑、
  可审计、带修订的默认主题记录；行已存在则绝不回写（保留管理员编辑），且仅在
  站点无其他生效默认主题时才把新行设为站点默认。
- **revision 一致性**：`themes.revision` 单调递增，SSR/浏览器/缓存/用户偏好
  共享同一 revision；主题变更即失效旧 ETag/偏好 If-Match。
- **上传隔离态**：数据包上传 → disabled；管理员显式“设为默认”激活。
- **代码型主题**：不提供在线上传/执行路径（v1 只接受 kind=data）。

## 1. 两类主题

### 1.1 数据型主题（v1 推荐）

包含：

- `theme.json` 元数据。
- CSS Token，例如颜色、字号、圆角、间距和密度。
- Logo、背景和字体等静态资源。
- 预览图。

不包含 JavaScript、Svelte 源码、HTML 模板或远程 URL 脚本。数据型主题可在运行时上传、验证、启用和切换。

### 1.2 可信代码型主题

包含 Svelte 组件或布局覆盖，必须：

1. 由实例管理员视为与 BBLBB 前端同等可信的代码。
2. 安装到源码树或构建输入目录。
3. 通过依赖审计、类型检查、测试和人工检查。
4. 重新执行 `pnpm build` 并部署生成物。

代码型主题不能通过“上传后沙箱测试”变成安全代码；浏览器和 Node 中执行的主题代码拥有应用进程或页面环境能力。

## 2. 数据型主题清单

```json
{
  "schema_version": 1,
  "name": "bblbb-default",
  "display_name": "BBLBB Default",
  "version": "1.0.0",
  "author": "BBLBB",
  "kind": "data",
  "supports": ">=1.0 <2.0",
  "assets": {
    "logo": "assets/logo.svg",
    "preview": "assets/preview.webp"
  },
  "tokens": {
    "color.background": "#ffffff",
    "color.surface": "#f8fafc",
    "color.text": "#172033",
    "color.muted": "#64748b",
    "color.accent": "#2563eb",
    "color.border": "#dbe3ee",
    "font.body": "system-ui",
    "font.mono": "ui-monospace",
    "radius.control": "0.5rem",
    "radius.card": "0.5rem",
    "space.density": "comfortable",
    "layout.mode": "classic"
  }
}
```

规则：

- `name` 只能使用小写 ASCII、数字和连字符。
- 每个 token 有允许类型和范围；不接受任意 CSS 文本。
- 颜色必须是标准色值，尺寸必须落在允许范围。
- 字体必须是 `FONT_FAMILY_ALLOWLIST` 的精确成员（不接受逗号组合列表）；
  密度必须是 `compact|comfortable|relaxed`；结构必须是
  `classic|sidebar|wide` 之一（已编译闭集）。
- `layout.mode` 是可选 token：缺失视为 `classic`；旧 v1 主题包无需迁移。
- `color.*.dark`（v1.1，6 个，全部可选）：夜间（暗色）配色变体，与对应日间
  key 完全同构（标准 hex 校验）。渲染回退链：**夜间变体 → 日间值 → 内置官方
  夜色板**（见 §10.4）。官方预置包与内置 default 均携带完整日/夜双色板。
- 资产路径不能包含 `..`、绝对路径或符号链接逃逸。
- 字体文件有类型和总大小限制。
- 不允许 `url(javascript:)`、远程 `@import`、内联脚本或 HTML。

## 3. 前端组件架构

系统 UI 分成两层：

1. **稳定业务组件**：登录、权限判断、表单提交、审核动作等，主题不可替换。
2. **展示组件**：Header、BoardCard、PostCard、ArticleLayout 等，可由已编译代码型主题覆盖。

推荐目录：

```text
frontend/src/
  lib/components/core/          不可覆盖业务组件
  lib/components/presentation/  默认展示组件
  lib/theme/registry.ts
  themes/
    default/
      manifest.ts
      components/
      styles.css
```

构建时注册：

```ts
const manifests = import.meta.glob('/src/themes/*/manifest.ts', { eager: true });
```

只能从 registry 选择组件，不能用数据库返回的任意文件路径作为 `import()` 参数。

## 4. 运行时选择

主题选择优先级：

1. 管理员预览参数（需要签名或管理员 Session，不接受任意路径）。
2. 已登录用户偏好。
3. 站点默认主题。
4. 内置 `default` fallback。

- 数据库只保存主题名称和 Token 设置。
- 若主题不存在、不兼容或被停用，回退默认主题并记录告警。
- 一个请求内主题固定，SSR 与 hydration 必须使用同一主题，避免闪烁和不一致。

## 5. API

```text
GET   /api/v1/themes/active
GET   /api/v1/me/preferences/theme
PUT   /api/v1/me/preferences/theme
GET   /api/v1/admin/themes
POST  /api/v1/admin/themes/data-packages        # 仅数据型主题
PUT   /api/v1/admin/themes/default
PATCH /api/v1/admin/themes/{name}/settings
DELETE /api/v1/admin/themes/{name}               # 不能删除当前默认/内置主题
```

代码型主题不提供在线上传 API。管理员通过受控部署流程安装。

## 6. 主题切换与缓存

- Token 变更可立即生效，并增加 `theme_revision`。
- SSR 页面可根据 revision 生成 ETag。
- 有用户主题偏好的页面不能进入未区分 Cookie 的共享缓存。
- 静态主题资产使用内容哈希文件名和长缓存。
- 删除数据型主题前检查用户偏好并迁移到默认主题。

## 7. 安全边界

- 数据型主题按不可信输入处理：解压限制、文件数量限制、总大小限制、路径穿越防护、MIME 检查和 schema 校验。
- 代码型主题按可信供应链代码处理，拥有与前端应用相同的安全权限。
- 主题永远不能决定内容是否可见；Rust API 在返回前完成裁决。
- 主题不能替换核心管理员确认流程、CSRF token、登录表单安全属性或审计原因字段。
- CSP 仍由 Caddy/SvelteKit 统一下发，主题不能放宽 CSP。

## 8. 兼容性

- `schema_version` 管理数据型主题清单。
- `supports` 声明兼容的 BBLBB 主版本。
- 展示组件 props 使用独立版本化接口；破坏性变更只能在 BBLBB 主版本中发生。
- 缺失非必要 Token 使用默认值；未知 Token 忽略并记录警告。
- CI 对所有内置代码型主题执行类型检查、Playwright 冒烟和 axe。

## 9. v1 验收范围

- 一个内置默认代码主题。
- 亮色/暗色作为同一主题的 Token 模式，而不是两套业务组件。
- 可上传和切换数据型主题。
- 可按用户保存已安装主题偏好。
- 不支持在线安装 Svelte 代码。
- 不支持主题自定义路由。

## 10. 全站生效链路与前端设计系统桥接（M13-THEME-08 / M18-ADMIN-THEMES）

### 10.1 全站生效链路（Root Layout + API）

1. **服务端加载（SSR）**：`frontend/src/routes/+layout.server.ts` 在服务端并行调用 `GET /api/v1/themes/active`，将当前生效主题数据（用户偏好优先，站点默认次之，内置 `default` 兜底）注入根布局数据树（`data.activeTheme`）。
2. **客户端投影（Hydration）**：`frontend/src/routes/+layout.svelte` 在浏览器端监听 `data.activeTheme` 变更，通过 `$lib/theme/projection.ts` 的 `applyThemeTokens()` 函数把安全 Token 写入 `document.documentElement` 的 CSS 自定义属性（`--bb-*`），并设置 `dataset.themeCustom = 'true'`。
3. **安全过滤**：所有 Token 值在客户端应用前由 `safeTokenValue()` 进行白名单与特征校验，拒绝任何包含 `<`、`>`、`url(`、`@import` 等异常字符，确保 XSS 与外部资源隔离。`applyThemeTokens()` 先清空旧 `--bb-*` 变量再应用，主题切换不残留上一主题的值。
4. **结构预设链路（layout.mode）**：根布局在 SSR 首帧把解析后的预设写入
   `.app-shell[data-theme-layout]`（`resolveLayoutMode()`：合法值直通，缺失/非法
   回退 `classic`）；`theme-layout.css`（最后加载层）按该属性实现
   `sidebar`（左侧竖栏导航）/`wide`（宽幅容器 + 四列网格）结构变体，
   移动端（<1024px）一律回退经典结构。管理端预览与正式切换共用同一
   投影函数（`applyThemeTokens` 同步 `documentElement` 与 `.app-shell`）。

### 10.2 全局实时预览与退出机制

1. **管理后台实时预览**：在 `/admin/themes` 点击任何主题的「预览」，客户端调用 `previewThemeTokens(theme)`，将 Token 直接注入当前页面的 `document.documentElement`，并标记 `data-theme-preview="true"`。
2. **悬浮状态条与 Showcase**：页面顶部浮现全局预览横条，支持一键打开「组件库效果展示 Showcase」查看真实导航栏、帖子卡片、按钮与排版渲染；支持一键「设为站点默认」或「退出预览」。
3. **安全恢复**：点击「退出预览」或页面销毁时，系统调用 `clearThemeTokens()`，并精准恢复当前站点默认活跃主题，绝不残留未保存的临时色彩。

### 10.3 官方原生默认配色与预置主题库

系统预置库不仅提供视觉扩展主题，更完整收录了 BBLBB 的官方原生默认配色，确保管理员随时可查看、预览及切回原版。**全部 6 个官方包均为日/夜双模式**（v1.1）：站点亮色模式取日间板，暗色模式取夜间板，自动切换：

| 主题代号 | 主题名称 | 日间色板 | 夜间色板（`color.*.dark`） | 特征 |
|---|---|---|---|---|
| `default` / `bblbb-classic` | **BBLBB 经典赤墨（原版默认）** | 背景 `#f5f3ed`、卡片 `#fffefb`、文字 `#17211f`、强调 `#b23e2a`（暖珊瑚红） | 背景 `#101b19`、卡片 `#172522`、文字 `#f5f3ea`、强调 `#f27759`（珊瑚橙） | 官方原生基准视觉；夜间板与内置 `html.dark` 官方暗色板一致 |
| `chinese-elegance` | **水墨青石（中国风）** | 背景 `#f5f3ee`、卡片 `#fbfaf7`、文字 `#1f1d1a`、强调 `#5a6c7d`（青石蓝） | 背景 `#171a1d`、卡片 `#202429`、文字 `#e7e5df`、强调 `#7f95a8`（月白青） | 典雅素雅：日间宣纸水墨，夜间宿墨玄青书卷 |
| `midnight` | **暗夜极光** | 背景 `#eef3f8`、卡片 `#ffffff`、文字 `#16202e`、强调 `#0284c7`（晴空青） | 背景 `#0f172a`、卡片 `#1e293b`、文字 `#e2e8f0`、强调 `#38bdf8`（天蓝） | 日间极昼浅蓝，夜间深蓝灰极光，昼夜皆沉浸 |
| `paper` | **复古羊皮纸** | 背景 `#faf6ef`、卡片 `#ffffff`、文字 `#2c2c2c`、强调 `#b23e2a` | 背景 `#1b1813`、卡片 `#25211a`、文字 `#e9e2d2`、强调 `#e07856`（琥珀暖） | 日间暖羊皮×朱砂红，夜间灯火书斋 |
| `forest` | **翡翠森林** | 背景 `#f0f5f2`、卡片 `#ffffff`、文字 `#132a21`、强调 `#0f756c`（墨绿） | 背景 `#0f1713`、卡片 `#17231c`、文字 `#ddebe2`、强调 `#40c9a2`（翡翠荧光） | 日间薄荷浅林，夜间深林夜色 |
| `cyberpunk` | **赛博霓虹** | 背景 `#f5f1fa`、卡片 `#ffffff`、文字 `#251c38`、强调 `#db2777`（热粉） | 背景 `#181126`、卡片 `#241b35`、文字 `#f3f0f7`、强调 `#ec4899`（霓虹粉） | 日间雾紫纸面，夜间深紫暗夜霓虹 |

### 10.4 双模式解析与 CSS 变量全量映射（v1.1）

`theme-tokens.css`（`app.css` 最后颜色层，`chinese-elegance.css` 之后、
`theme-layout.css` 结构层之前）是数据型主题到设计系统语义变量的**唯一映射点**
（`tokens.css` 与 `chinese-elegance.css` 的旧局部映射块已移除，防双份漂移）：

1. **区块 A（日间解析）**：`:root[data-theme-custom/preview]`、
   `.theme-preview-scope` 上把 6 个 `--bb-color-*` 解析为中间变量
   `--bb-active-background/surface/text/muted/accent/border`（回退内置日色板）。
2. **区块 B（夜间解析）**：`html.dark` 作用域（日夜模式的暗色态）下
   `--bb-active-*` 优先取 `--bb-color-*-dark` 夜间变体，**缺失回退日间值**
   （旧主题零迁移），再回退内置官方夜色板（`#101B19/#172522/#F5F3EA/#B5C0BA/#F27759/#30433E`）。
3. **区块 C（语义映射，日/夜共用）**：由 `--bb-active-*` 派生全量语义变量——
   核心 6 色 + `color-mix` 派生（`--color-bg-subtle/inset/hover`、
   `--color-surface-hover/selected`、`--color-text-tertiary`、
   `--color-brand-hover/pressed/soft`、`--color-border-muted/strong`、
   `--color-code-bg`、`--color-focus-ring` 等）+ 字体 +
   圆角（`radius.control`→`--radius-sm/md`、`radius.card`→`--radius-lg/xl`）+
   密度（`--space-*` 按 `--bb-density-scale` 0.85/1/1.2 缩放）+
   三级阴影（`--shadow-control/pop/modal`）+ 动效（`--duration-fast/base/slow`）。
   变量定义统一 `!important`，压过 `html.dark` 与无 JS `prefers-color-scheme` 基线。
4. **区块 D（减少动效）**：`data-motion-reduced="true"`（projection 依据
   `motion.reduced` Token 或系统偏好写入）→ 三档时长强制 `0ms`。
5. **属性所有权**：`dataset.theme`（light/dark）归日夜模式系统
   （app.html 防闪烁脚本 + `$lib/theme.ts`）所有；数据型主题名写入
   `data-theme-name`。旧版 `html[data-theme="<主题名>"]` 选择器已废弃移除
   （投影器不再覆盖日夜状态，`html[data-theme="dark"]` 的组件级暗色规则恢复生效）。
6. **组件级兜底**：`chinese-elegance.css` 末尾保留对 `body`/`.app-navbar`/
   `.app-side`/`.app-admin-side`/`.app-card`/`.btn.primary` 的直接 `!important`
   覆盖，统一消费 `--bb-active-*`（日/夜解析后的活动色板）。

由此，主题激活时全站（含日夜两种模式）的每个角落——背景、卡片、导航栏、
侧边栏、按钮、圆角、间距、阴影、动效——均能彻底、一致地响应 Token 变换。
