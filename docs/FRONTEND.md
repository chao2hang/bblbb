# BBLBB — SvelteKit 前端规范

> 版本：v0.4
> 前端负责 SSR、渐进增强、管理后台、SEO 和主题展示；Rust 后端始终是数据、身份、权限和协议的最终裁决者。

## 1. 技术基线

- SvelteKit 稳定版、Svelte 稳定版、TypeScript strict mode。
- `@sveltejs/adapter-node`，生产环境独立 Node 进程。
- pnpm 管理依赖并提交锁文件。
- 使用原生 `fetch` 和薄 API 客户端，不在 v1 引入大型全局状态框架。
- 样式使用 CSS Token、CSS Modules/scoped CSS 和少量无样式基础组件。
- 图标可使用 `lucide-svelte` 按需加载。
- OpenAPI 是接口事实来源，`openapi-typescript` 生成请求/响应类型。

前端依赖使用精确版本或受控范围，升级须通过测试，文档不写“最新版本”作为可复现规格。

## 2. 请求与身份链路

```text
浏览器
├── GET 页面 ──► Caddy ──► SvelteKit SSR
│                              └── 转发 Cookie ──► Rust /api/v1/*
└── API/表单 ──► Caddy ──► Rust /api/v1/*
```

- 浏览器直接同源调用 `/api/v1/*` 是默认模式。
- `+page.server.ts` 和 `+layout.server.ts` 在 SSR 中使用 `INTERNAL_API_ORIGIN=http://127.0.0.1:8080`。
- SSR 请求必须转发：`Cookie`、`X-Request-ID`；写请求还要转发 CSRF header。
- Rust 返回的 `Set-Cookie` 必须由浏览器直接接收；若某个 form action 代理登录，action 必须显式复制所有 `Set-Cookie` 属性。
- SvelteKit 的 `locals.user` 只是展示快照，不能作为后端授权依据。
- 不把后端管理员 secret 放入客户端 Bundle。

## 3. 路由建议

```text
frontend/src/
  routes/
    +layout.server.ts          会话、站点配置和主题元数据
    +layout.svelte
    +page.server.ts            首页
    articles/[slug]/           博客文章
    boards/[slug]/             板块列表
    posts/[id]/                讨论详情
    compose/                   发文章/讨论
    users/[username]/          公开主页
    tags/[slug]/               标签归档
    search/                    搜索（v1.0）
    auth/
      login/
      register/
      verify-email/
      forgot-password/
      reset-password/
      sessions/
      consent/[interactionId]/ OIDC 授权同意 UI；只接受 Rust 创建的短期 interaction
    admin/
      +layout.server.ts        展示守卫；Rust API 仍二次鉴权
      dashboard/
      users/
      roles/
      boards/
      posts/
      reports/
      sanctions/
      points/
      levels/
      plugins/
      themes/
      marketplace/
      download-billing/
      ai/
      video/
      settings/
    marketplace/
      checkout/[intentId]/      BBLBB 托管确认页
      result/[code]/            一次性结果码落地页
    downloads/[id]/              下载授权/重签状态页
    ai/tasks/[id]/               用户任务状态页
    rss.xml/+server.ts
    sitemap.xml/+server.ts
    robots.txt/+server.ts
```

建议将功能组织放入：

```text
src/lib/
  api/               生成类型、API client、错误映射
  auth/              登录态与 CSRF 工具
  components/        稳定、无主题业务组件
  features/          按 user/post/moderation 等领域组织
  theme/             Token 与已编译组件 registry
  plugin/            已预编译 UI 扩展 registry
  markdown/          编辑器和安全展示组件
  i18n/              文案资源
```

## 4. API 客户端

- `/api/v1/openapi.json` 是生成输入。
- CI 在生成后执行 `git diff --exit-code`，禁止契约更新但类型未更新。
- API client 统一实现：
  - `credentials: 'same-origin'`。
  - CSRF header 注入。
  - `X-Request-ID` 传播。
  - 统一解析 `application/problem+json`。
  - 401 只在需要登录的页面跳转；公开页面允许匿名。
  - 409 映射为版本冲突/幂等冲突，不笼统显示服务器错误。
  - 429 显示 `Retry-After`。

## 5. 数据加载与表单

### SSR 数据

- 公开首屏使用 `+page.server.ts`，保持 SEO 和无 JavaScript 可读性。
- 根 layout 只加载轻量的站点配置与当前会话；不要每页拉完整权限树。
- 管理后台按页面按需加载数据。

### 表单

- 登录、注册、设置等适合使用 SvelteKit form action + `use:enhance`。
- 发帖、回复等也可直接调用同源 Rust API；选择一种方式后需统一错误格式和 CSRF 流程。
- GET 不得修改状态。
- 草稿自动保存发送 `If-Match`/version，避免覆盖多标签页编辑。
- 高风险动作使用确认对话框并由后端再次验证权限；删除、封禁和积分调整需要原因。

## 6. 缓存分级

| 页面/响应 | Cache-Control |
|---|---|
| 带 Session、管理页、通知 | `private, no-store` |
| 回复/等级/付费可见内容 | `private, no-store`，即使当前用户已解锁 |
| 完全公开文章 | 可使用短期 `public` + `stale-while-revalidate` |
| 公开板块列表 | 仅当响应不含用户、权限和主题偏好时允许 `public` |
| 静态哈希资源 | `public, max-age=31536000, immutable` |

- 个性化响应设置 `Vary: Cookie`，Bearer API 设置 `Vary: Authorization`。
- 主题预览、管理员越权查看和隐藏内容不进入共享缓存。
- 服务端 API 响应中未授权字段必须缺失/替换，不依赖前端 CSS 遮挡。

## 7. Markdown 与内容显示

- 编辑器提交 Markdown 源文。
- Rust 后端负责解析和清洗，数据库保存源文与清洗后的 HTML。
- 前端只显示后端提供的可信清洗结果；若使用 Svelte `{@html}`，输入类型必须是专用 `SanitizedHtml`，普通字符串不能直接传入。
- 禁止原始 HTML、脚本、事件属性、危险协议和任意 iframe。
- 链接添加 `rel="ugc nofollow noopener noreferrer"`；外链在新窗口打开时必须带 `noopener`。
- 隐藏部分由后端独立返回，不在公开 HTML 中以隐藏 DOM 形式携带。

## 8. SEO 与博客体验

- 文章使用稳定 slug，旧 slug 返回 301。
- 每页设置 canonical、title、description、Open Graph 和 Twitter Card。
- 文章输出 `Article` JSON-LD；讨论可输出 `DiscussionForumPosting`。
- 提供 RSS/Atom、sitemap 和 robots。
- 待审核、删除、隐藏、付费/回复可见正文不得进入 RSS、sitemap 摘要或 OpenGraph。
- 分页页面使用稳定 URL；搜索页默认 `noindex`。
- 404 返回真实 404，权限隐藏的资源按威胁模型决定 403 或 404。

## 9. 主题与插件边界

- `import.meta.glob()` 在构建时收集可信、已编译主题与插件 UI。
- 运行时只能切换 registry 中已经编译的模块。
- 数据型主题仅修改经 schema 验证的 CSS Token，可运行时安装和切换。
- 新的 Svelte 主题/插件必须经过重新构建和部署。
- 任意前端代码拥有页面环境权限，不能靠 TypeScript props 声明形成安全沙箱。
- 插件 UI 只能调用用户本就有权限访问的 API；Rust 仍做授权。

详见 [`THEME.md`](THEME.md) 与 [`PLUGIN.md`](PLUGIN.md)。

## 10. 可访问性与国际化

v1 目标为 WCAG 2.2 AA：

- 键盘可完成注册、登录、发帖、回复和后台审核。
- 清晰的焦点样式、跳过导航链接、语义化标题和 landmark。
- 对话框锁定并恢复焦点。
- 表单错误与字段通过 `aria-describedby` 关联。
- 颜色不是唯一状态信号；支持减少动画偏好。
- 主题 Token 需要通过对比度检查。

文案从第一阶段进入 i18n 资源，默认 `zh-CN`；即便 v1 只发布中文，也不在组件中散落后端错误文案。

## 11. 前端测试

- Vitest：工具函数、权限展示、错误映射、主题 token。
- Svelte Component Testing：表单、内容卡片、审核控件。
- Playwright：匿名浏览、注册验证、登录、发帖、回复、审核、解锁和 Session 撤销。
- axe：核心页面自动可访问性检查。
- 视觉回归：默认主题的首页、文章、讨论、管理页和暗色变量。
- 测试必须覆盖无 JavaScript 的核心浏览和表单退化路径。

验收矩阵见 [`TESTING.md`](TESTING.md)。
