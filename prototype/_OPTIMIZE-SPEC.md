# BBLBB 原型 · 全量优化规范

基于 `prototype/messages.html` 和 `prototype/article-101.html` 已建立的暗色双栏体系（forum.css + chaos.css），
所有 90 个 HTML 页面统一向这套设计语言对齐。

## 1. 通用页面骨架

每个公开页面（除 auth/error）必须满足：

```html
<div class="forum-shell">
  <header class="forum-nav" data-forum-nav></header>
  <main class="forum-main" id="main">
    <nav class="breadcrumb" aria-label="位置">
      <a href="index.html">首页</a>
      <span class="sep" data-icon="chevron-right" data-size="10"></span>
      <span class="current">当前页</span>
    </nav>

    <header class="page-head">
      <div class="page-head__meta">
        <h1 class="page-head__title">{标题}</h1>
        <p class="page-head__lede">{副标题/说明}</p>
      </div>
      <div class="page-head__actions">
        <!-- 关键操作 -->
      </div>
    </header>

    <div class="forum-grid forum-grid--with-side">
      <div><!-- 主内容 --></div>
      <aside class="forum-side"><!-- 侧栏 side-card --></aside>
    </div>
  </main>
</div>
<script src="assets/chaos.js"></script>
```

## 2. 暗色模式（默认）

与 messages.html 一致：在 `html` 之前注入 token 覆盖：

```html
<style>
:root {
  color-scheme: dark;
  --canvas: #0a0a0a;
  --surface: #161616;
  --surface-subtle: #1d1d1d;
  --surface-hover: #232323;
  --ink: #ededed;
  --ink-muted: #a5a5a5;
  --ink-faint: #6e6e6e;
  --ink-inverse: #0a0a0a;
  --line: #2a2a2a;
  --line-strong: #3a3a3a;
  --line-soft: #232323;
  --info: #5fb3ff;
  --info-soft: #0f2233;
  --info-ink: #9ed2ff;
  --ok: #4ade80;
  --ok-soft: #0f2a1a;
  --ok-ink: #7adfa3;
  --warn: #f5b15c;
  --warn-soft: #2e1f0c;
  --warn-ink: #f5b15c;
  --bad: #ef5a5a;
  --bad-soft: #2c1414;
  --bad-ink: #f08080;
  --featured: #c9a26b;
  --featured-soft: #2a2010;
  --featured-ink: #d6b97f;
}
</style>
```

放到 `<head>` 内 `<link rel="stylesheet">` 之前。

## 3. 页面元信息

- `<title>`：用页面名 + · BBLBB
- `<meta name="description">`：一句话说明
- 必须有 `<a class="skip-link" href="#main">跳到主内容</a>`

## 4. 面包屑

每个非首页必须有：首页 > {父级} > 当前页（多级）
或  首页 > 当前页（顶级页）

```html
<nav class="breadcrumb" aria-label="位置">
  <a href="index.html">首页</a>
  <span class="sep" data-icon="chevron-right" data-size="10"></span>
  <a href="boards.html">板块</a>
  <span class="sep" data-icon="chevron-right" data-size="10"></span>
  <span class="current">Rust</span>
</nav>
```

## 5. page-head 标题区

每个非详情页必须有：

```html
<header class="page-head">
  <div class="page-head__meta">
    <h1 class="page-head__title">{标题}</h1>
    <p class="page-head__lede">{副标题：说明 + 数量}</p>
  </div>
  <div class="page-head__actions">
    <button class="btn btn--secondary btn--sm">{操作}</button>
  </div>
</header>
```

## 6. 内容卡片

主内容区都用 `background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius-lg);`
已建立的类：`side-card`, `panel`, `topic-head`, `topic-body`, `article-card`, `post-table`, `post-row`, `user-strip`, `board-head`, `filter-bar`, `thread-list`, `topic-reply`, `topic-editor`, `reply-section`, `profile-card`, `me-tabs`, `shop-grid`, `product`, `ach-grid`, `notif-tabs`, `notif-item`, `stat-grid`, `tag-cloud`, `recommend`, `qr`, `stats-card`, `pagination`, `tabs`, `seg`, `switch`, `set-row`, `row-line`, `my-row`, `ent-row`, `forum-side`, `msg-shell` 等

新类（用过的）：`msg-conv`, `msg-chat`, `msg-bubble-row`, `msg-bubble`, `msg-composer`, `article-hero`, `article-body`, `article-prose`, `article-toc`, `article-side`, `article-pager`, `related-card`, `author-card`, `callout`, `reading-progress`, `cover-*`

## 7. 操作按钮

- 主操作：`btn btn--primary btn--sm`
- 次操作：`btn btn--secondary btn--sm`
- 取消/低优先：`btn btn--ghost btn--sm`
- 危险：`btn btn--danger btn--sm`
- 图标按钮：`btn btn--ghost btn--icon btn--sm`

## 8. 状态徽章

使用 `.status-badge` + 修饰：`pinned` / `featured` / `discussion` / `pending` / `resolved` / `rejected` / `banned` / `draft` / `locked` / `article` / `soft`

## 9. 侧栏

用 `<aside class="forum-side">` 包侧栏内容，每块 `<div class="side-card">`。

```html
<aside class="forum-side">
  <div class="side-card">
    <div class="side-card__head"><h3 class="side-card__title">标题</h3></div>
    <div class="side-card__body">...</div>
  </div>
</aside>
```

## 10. 链接

- 内部链接：`xxx.html`
- 不存在的占位：`#`（避免 404）
- 文章详情：用 `article-101.html` 系列（已切换）
- 帖子/讨论：用 `topic-201.html` 系列

## 11. JS

所有页面必须引入 `assets/chaos.js`。需要 `demo-flows.js` 的页面（topic-101 等）也保留。

## 12. 注意事项

- 不要改 forum.css / chaos.css（除非需要新增 class）
- 不要删除已有交互（toast / modal / popover / data-toast / data-like / data-modal / data-confirm）
- 保留 `<a class="skip-link" href="#main">`
- 保留 `<a class="wordmark"...>` 通过 data-forum-nav 自动注入
- 暗色模式是默认，但保留切换按钮（chaos.js 自动注入）
