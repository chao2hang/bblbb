# BBLBB — 主题系统规范

> 版本：v0.4（M13 已实现：v1 数据型主题 + 管理 API + 封闭 Token schema）
> 主题只改变展示，不改变身份、权限、审核、积分或内容可见性。主题分为可安全运行时加载的“数据型主题”和构建时编译的“可信代码型主题”。

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
- **封闭 Token schema**：14 个已知 key（颜色/字体/圆角/密度/阴影/动效）；
  值级校验拒绝 CSS、HTML、JS、SVG、远程资源与任意 style 字符串；未知 key
  拒绝；资产路径只允许相对路径（无 `..`/绝对路径/URL）。
- **Fallback 与重置机制**：主题不存在/不兼容/停用/损坏 → 回退内置 `default`（revision=1）
  并记录非敏感告警；损坏主题自动标记 `corrupt`；设为默认目标为 `default` 时自动取消所有自定义主题的 `is_default` 标记。
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
    "font.body": "system-ui, sans-serif",
    "font.mono": "ui-monospace, monospace",
    "radius.control": "0.5rem",
    "space.density": "1"
  }
}
```

规则：

- `name` 只能使用小写 ASCII、数字和连字符。
- 每个 token 有允许类型和范围；不接受任意 CSS 文本。
- 颜色必须是标准色值，尺寸必须落在允许范围。
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
3. **安全过滤**：所有 Token 值在客户端应用前由 `safeTokenValue()` 进行白名单与特征校验，拒绝任何包含 `<`、`>`、`url(`、`@import` 等异常字符，确保 XSS 与外部资源隔离。

### 10.2 全局实时预览与退出机制

1. **管理后台实时预览**：在 `/admin/themes` 点击任何主题的「预览」，客户端调用 `previewThemeTokens(theme)`，将 Token 直接注入当前页面的 `document.documentElement`，并标记 `data-theme-preview="true"`。
2. **悬浮状态条与 Showcase**：页面顶部浮现全局预览横条，支持一键打开「组件库效果展示 Showcase」查看真实导航栏、帖子卡片、按钮与排版渲染；支持一键「设为站点默认」或「退出预览」。
3. **安全恢复**：点击「退出预览」或页面销毁时，系统调用 `clearThemeTokens()`，并精准恢复当前站点默认活跃主题，绝不残留未保存的临时色彩。

### 10.3 官方原生默认配色与预置主题库

系统预置库不仅提供视觉扩展主题，更完整收录了 BBLBB 的官方原生默认配色，确保管理员随时可查看、预览及切回原版：

| 主题代号 | 主题名称 | 色彩基调 | 特征 |
|---|---|---|---|
| `default` / `bblbb-classic` | **BBLBB 经典赤墨（原版默认）** | 背景 `#f5f3ed`、卡片 `#fffefb`、文字 `#17211f`、强调色 `#b23e2a`（暖珊瑚红） | 官方原生基准视觉，底蕴沉稳，对比舒适 |
| `chinese-elegance` | **水墨青石（中国风）** | 背景 `#f5f3ee`、卡片 `#fbfaf7`、文字 `#1f1d1a`、强调色 `#5a6c7d`（青石蓝） | 典雅素雅的宣纸水墨质感与青石灰蓝点缀 |
| `midnight` | **暗夜极光** | 背景 `#0f172a`、卡片 `#1e293b`、文字 `#e2e8f0`、强调色 `#38bdf8`（天蓝） | 深蓝灰暗色主题，护眼沉浸 |
| `paper` | **复古羊皮纸** | 背景 `#faf6ef`、卡片 `#ffffff`、文字 `#2c2c2c`、强调色 `#b23e2a` | 温暖复古书卷质感 |
| `forest` | **翡翠森林** | 背景 `#f0f5f2`、卡片 `#ffffff`、文字 `#132a21`、强调色 `#0f756c`（墨绿） | 清新自然的森林与薄荷翡翠色系 |
| `cyberpunk` | **赛博霓虹** | 背景 `#181126`、卡片 `#241b35`、文字 `#f3f0f7`、强调色 `#ec4899`（霓虹粉） | 深紫暗夜与高饱和潮酷碰撞 |

### 10.4 CSS 变量最高优先级级联覆盖

前端设计系统的终极覆盖层（`chinese-elegance.css`）在文件末尾定义了终极选择器：
```css
:root[data-theme-custom="true"],
:root[data-theme-preview="true"],
html[data-theme-custom="true"],
html[data-theme-preview="true"] {
  --color-bg-page: var(--bb-color-background) !important;
  --color-bg-card: var(--bb-color-surface) !important;
  --color-text-primary: var(--bb-color-text) !important;
  --color-text-secondary: var(--bb-color-muted) !important;
  --color-brand: var(--bb-color-accent) !important;
  --color-border: var(--bb-color-border) !important;
}
```
结合对 `body`、`.app-card`、`.app-navbar`、`.app-side`、`.btn.primary` 的变量映射，保证主题激活时全站每个角落均能彻底、一致地响应色彩变换。
