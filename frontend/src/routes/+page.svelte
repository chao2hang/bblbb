<script lang="ts">
  // 首页 —— 与原型（prototype/pages/home.html #page-home）同构的三栏布局：
  //   左：分类导航（category-card，板块 + 帖数）
  //   中：信息流（feed：筛选工具条 + 线程卡列表 + 加载更多）
  //   右：推荐栏（right-rail：推荐内容 + 页脚，与原型一致）
  //
  // M00-FRONTEND-08：无 JS 基线——公开数据全部来自 SSR load（+page.server.ts），
  // 筛选 tab 与加载更多均为原生链接（?sort= / ?after=）。
  // M00-FRONTEND-09：load 输出只保留公开字段白名单（见 +page.server.ts）。
  import { type PostSummary } from '$lib/api/client';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show } from '$lib/ui/toast';
  import { formatCount, formatRelative } from '$lib/utils';
  import Seo from '$lib/components/Seo.svelte';
  import type { HomePageData, HomeSort } from './+page.server';

  let { data }: { data: HomePageData } = $props();

  const boards = $derived(data.boards);
  const totalPosts = $derived(
    data.stats?.posts ?? boards.reduce((n, b) => n + (b.post_count ?? 0), 0)
  );

  /** 板块 id → 名称（线程卡脚注用；boards 与 posts 同源取自 SSR load）。 */
  const boardName = $derived((id: string | null | undefined) => {
    if (!id) return null;
    return boards.find((b) => b.id === id)?.name ?? null;
  });

  /** 筛选 tab（M18-HOME-02 对齐原型：所有|精华|已关注|热门；原生链接）。 */
  const sortTabs: Array<{ value: HomeSort; label: string; href: string }> = [
    { value: '', label: '所有', href: '/' },
    { value: 'featured', label: '精华', href: '/?sort=featured' },
    { value: 'following', label: '已关注', href: '/?sort=following' },
    { value: 'popular', label: '热门', href: '/?sort=popular' }
  ];

  // ── 加载更多（游标 = 上一页最后一条 created_at；JS 客户端追加、
  //    无 JS 回退 ?after= 链接整页翻页）──
  type PostRow = HomePageData['posts'][number];
  let extraPages = $state<PostRow[]>([]);
  let loadedCursor = $state<string | null | undefined>(undefined);
  let loadingMore = $state(false);

  const posts = $derived<PostRow[]>([...data.posts, ...extraPages]);
  const cursor = $derived(loadedCursor === undefined ? data.nextCursor : loadedCursor);
  const loadMoreHref = $derived(
    cursor
      ? `/?${new URLSearchParams({
          ...(data.sort ? { sort: data.sort } : {}),
          after: cursor
        }).toString()}`
      : '/'
  );

  // load 数据变化（tab 切换导航 / invalidateAll）后重置客户端累积页。
  $effect(() => {
    void data.posts;
    void data.sort;
    void data.after;
    extraPages = [];
    loadedCursor = undefined;
  });

  /** 客户端行归一化（与 +page.server.ts pickPostRow 同构；本地实现避免
   *  从 server 文件导入把 $env/private 拖进浏览器包）。 */
  function toRow(p: PostSummary): PostRow {
    return {
      id: p.id,
      title: p.title,
      author_id: p.author_id ?? p.author?.id,
      author_name: p.author_name ?? p.author?.username ?? null,
      board_id: p.board_id ?? null,
      summary: p.summary ?? null,
      is_featured: p.is_featured ?? Boolean(p.featured_at),
      reply_count: p.reply_count,
      view_count: p.view_count,
      like_count: p.like_count ?? 0,
      pinned: p.pinned ?? Boolean(p.pinned_at),
      created_at: p.created_at,
      last_reply_at: p.last_reply_at ?? null
    };
  }

  /** JS 下追加下一页；无 JS 走 ?after= 链接（整页翻页）。 */
  async function loadMore(event: MouseEvent): Promise<void> {
    event.preventDefault();
    if (!cursor || loadingMore) return;
    loadingMore = true;
    try {
      const params = new URLSearchParams({ limit: '8' });
      if (data.sort) params.set('sort', data.sort);
      params.set('after', cursor);
      const response = await fetch(`/api/v1/posts?${params.toString()}`, {
        credentials: 'same-origin',
        headers: { Accept: 'application/json' }
      });
      if (!response.ok) throw new Error(`posts page ${response.status}`);
      const page = (await response.json()) as {
        items?: PostSummary[];
        next_cursor?: string | null;
        page?: { next_cursor?: string | null };
      };
      extraPages = [...extraPages, ...(page.items ?? []).map(toRow)];
      loadedCursor = page.page?.next_cursor ?? page.next_cursor ?? null;
    } catch {
      show('加载更多失败，请重试', 'danger');
    }
    loadingMore = false;
  }
</script>

<Seo
  title="社区论坛"
  description="BBLBB 社区论坛：板块、标签与最新讨论"
  og={{ type: 'website', siteName: 'BBLBB' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'WebSite',
    name: 'BBLBB 社区论坛',
    description: '自由讨论、友善交流的社区论坛'
  }}
/>

<div class="container" id="page-home">
  <h1 class="sr-only">首页</h1>

  <!-- 原型移动端首页 Hero（桌面隐藏） -->
  <div class="mobile-hero">
    <a class="mobile-doc-link" href="/" aria-label="打开社区规范"><Icon name="book-open" size={14} /><span>规范</span></a>
    <a class="mobile-search-link" href="/search" aria-label="搜索帖子、用户或标签"><Icon name="search" size={16} /></a>
    <div class="mobile-logo">BBLBB</div>
    <div class="hero-stats">
      <button type="button">成员 <b>{formatCount(data.stats?.members ?? 128)}</b></button>
      <button type="button">内容 <b>{formatCount(totalPosts)}</b></button>
      <button type="button" onclick={() => navigator.clipboard?.writeText(location.href)}><Icon name="share-2" size={16} /> 分享</button>
    </div>
  </div>

  <div class="proto-home">
    <!-- 左栏：分类导航（原型 aside.category-card） -->
    <aside class="category-card" aria-label="板块分类">
      <a class="selected" href="/">
        <b>全部</b><em>{formatCount(totalPosts)}</em>
      </a>
      {#each boards as board (board.id)}
        <a href="/boards/{board.slug}">
          {board.name}<em>{formatCount(board.post_count)}</em>
        </a>
      {/each}
    </aside>

    <!-- 中栏：信息流（原型 section.feed） -->
    <section class="feed" aria-label="最新讨论">
      <div class="mobile-categories" aria-label="移动端分类">
        <button type="button" class="active">全部</button>
        <button type="button">默认分类</button>
        <button type="button" class="list" aria-label="全部分类"><Icon name="menu" size={18} /></button>
      </div>
      <div class="feed-toolbar">
        <nav class="filters" aria-label="帖子筛选">
          {#each sortTabs as tab (tab.value)}
            <a
              class="filter-btn {data.sort === tab.value ? 'active' : ''}"
              href={tab.href}
              aria-current={data.sort === tab.value ? 'page' : undefined}
            >
              {tab.label}
            </a>
          {/each}
        </nav>
        <a class="publish" href="/editor">发布内容</a>
      </div>

      <div class="thread-list">
        {#each posts as post (post.id)}
          {@const author = post.author_name ?? '匿名'}
          {@const bName = boardName(post.board_id)}
          <article class="thread {post.is_featured ? 'featured' : ''}">
            <Avatar name={author} size="lg" />
            <div class="thread-body">
              <div class="thread-meta">
                <b>{author}</b>
                <span>· {formatRelative(post.created_at)}</span>
                {#if post.is_featured}<i>精华</i>{/if}
              </div>
              <a class="thread-detail-link" href="/posts/{encodeURIComponent(post.id)}">
                <h2>{post.title}</h2>
                {#if post.summary}<p>{post.summary}</p>{/if}
              </a>
              <div class="thread-footer">
                {#if bName}<span>{bName}</span>{/if}
                <span class="thread-likes" style="display:inline-flex;align-items:center;gap:3px;font-size:12px;color:var(--color-text-tertiary);" aria-label="{formatCount(post.like_count ?? 0)} 人点赞">
                  <Icon name="heart" size={13} />
                  {formatCount(post.like_count ?? 0)}
                </span>
                <a
                  class="thread-comment-link"
                  href="/posts/{encodeURIComponent(post.id)}"
                  aria-label="查看回复"
                  style="display:inline-flex;align-items:center;gap:3px;"
                >
                  <Icon name="message-square" size={13} />
                  {formatCount(post.reply_count)}
                </a>
              </div>
            </div>
          </article>
        {/each}
        {#if posts.length === 0}
          <div class="thread-empty">
            <div class="empty-state-title">
              {data.sort === 'featured'
                ? '暂无精华帖'
                : data.sort === 'following'
                  ? '暂无关注动态'
                  : data.sort === 'popular'
                    ? '暂无热门帖'
                    : '暂无帖子'}
            </div>
            <p class="empty-state-desc">成为第一个发帖的人吧！</p>
          </div>
        {/if}
      </div>

      {#if cursor}
        <a class="load-more" href={loadMoreHref} onclick={(event) => void loadMore(event)}>
          {loadingMore ? '加载中…' : '加载更多'}
        </a>
      {:else if posts.length > 0}
        <div class="feed-end">— 已经到底了 —</div>
      {/if}
    </section>

    <!-- 右栏：推荐位（原型 aside.right-rail） -->
    <aside class="right-rail">
      {#if data.popular.length > 0}
        <section class="recommend" aria-label="推荐内容">
          <h2>推荐内容</h2>
          {#each data.popular as p, i (p.id)}
            <div class="recommend-item">
              <span class="rank">{String(i + 1).padStart(2, '0')}</span>
              <div>
                <a href="/posts/{encodeURIComponent(p.id)}"><b>{p.title}</b></a>
                <small>{formatCount(p.view_count)} 浏览 · {formatCount(p.reply_count)} 回复</small>
              </div>
            </div>
          {/each}
        </section>
      {/if}

      <footer class="rail-foot">© 2026<br />Powered By BBLBB Community</footer>
    </aside>
  </div>
</div>

<style>
  /* 原型 .layout：内容宽 1420、左右 10px 内边距（高密度） */
  .container {
    padding-inline: 10px;
  }

  /* ===== 三栏骨架（原型 #page-home：260px | 1fr | 300px，gap 20）===== */
  .proto-home {
    display: grid;
    grid-template-columns: minmax(220px, 260px) minmax(0, 1fr) minmax(250px, 300px);
    gap: var(--space-5);
    align-items: start;
    padding: var(--space-5) 0 var(--space-6);
  }
  .proto-home > * {
    min-width: 0;
  }

  /* ===== 左栏：分类导航 ===== */
  .category-card {
    background: var(--color-bg-card);
    border-radius: 0;
    border: var(--border-default);
    box-shadow: none;
    height: max-content;
    padding: 6px 16px;
  }
  .category-card a {
    height: 55px;
    border-bottom: var(--border-default);
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--color-text-secondary);
    text-decoration: none;
    font-size: 15px;
    padding: 0 14px;
  }
  .category-card a:last-child {
    border: 0;
  }
  .category-card .selected {
    color: var(--color-brand);
    font-weight: var(--weight-semibold);
  }
  .category-card em {
    font-style: normal;
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
  }

  /* ===== 中栏：信息流 ===== */
  .feed-toolbar {
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 25px;
    background: var(--color-bg-card);
    border: none;
    border-radius: 2px;
    box-shadow: none;
  }
  .filters {
    display: flex;
    align-items: center;
    gap: 22px;
    height: 100%;
  }
  .filter-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--color-text-secondary);
    text-decoration: none;
    font-size: var(--text-sm);
    padding: 21px 0;
    border-bottom: 2px solid transparent;
  }
  .filter-btn:hover {
    color: var(--color-text-primary);
  }
  .filter-btn.active {
    font-weight: var(--weight-semibold);
    color: var(--color-brand);
    border-bottom-color: var(--color-brand);
  }
  .publish {
    border: 0;
    border-radius: 2px;
    background: var(--color-brand);
    color: #fff;
    font-size: 14px;
    font-weight: 500;
    padding: 8px 20px;
    text-decoration: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .publish:hover {
    background: var(--color-brand-hover);
  }

  .thread-list {
    margin-top: 12px;
    background: var(--color-bg-card);
    border: var(--border-default);
    border-radius: 0;
    overflow: hidden;
  }
  .thread {
    display: flex;
    gap: 13px;
    padding: 14px 16px;
    border-bottom: var(--border-thin);
    background: transparent;
    border-radius: 0;
    transition: background 0.12s;
  }
  .thread:hover {
    background: var(--color-surface-hover);
  }
  .thread:last-child {
    border-bottom: none;
  }
  .thread-body {
    flex: 1;
    min-width: 0;
  }
  .thread-detail-link {
    display: block;
    color: inherit;
    text-decoration: none;
  }
  .thread-detail-link h2 {
    font-size: 16px;
    font-weight: var(--weight-semibold);
    line-height: 1.6;
    margin: 0 0 6px;
    color: var(--color-text-primary);
  }
  .thread-detail-link:hover h2 {
    color: var(--color-brand);
  }
  .thread-detail-link p {
    color: var(--color-text-secondary);
    line-height: 1.75;
    margin: 0;
    font-size: 14px;
  }
  .thread-meta {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }
  .thread-meta span {
    color: var(--color-text-tertiary);
    margin-left: 5px;
  }
  .thread-meta i {
    background: var(--color-brand-soft);
    color: var(--color-brand);
    font-style: normal;
    font-size: 11px;
    padding: 2px 5px;
    margin-left: 8px;
    border-radius: 4px;
  }
  .thread-footer {
    margin-top: 13px;
    display: flex;
    align-items: center;
    gap: 15px;
    color: var(--color-text-tertiary);
    font-size: 13px;
    white-space: nowrap;
  }
  .thread-footer span {
    color: var(--color-text-tertiary);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .thread-footer > * {
    padding: 5px 0;
  }
  .thread-comment-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--color-text-secondary);
    text-decoration: none;
    flex: 0 0 auto;
  }
  .thread-comment-link:hover {
    color: var(--color-brand);
  }
  .thread-empty {
    padding: 48px 24px;
    text-align: center;
  }
  .empty-state-title {
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
  }
  .empty-state-desc {
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    margin: var(--space-2) 0 0;
  }
  .load-more {
    margin: 4px auto 0;
    display: flex;
    padding: 8px 20px;
    width: max-content;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    color: var(--color-brand);
    text-decoration: none;
    font-size: var(--text-sm);
    background: transparent;
  }
  .load-more:hover {
    background: var(--color-bg-subtle);
  }
  .feed-end {
    text-align: center;
    color: var(--color-text-tertiary);
    font-size: var(--text-xs);
    padding: var(--space-3) 0;
  }

  /* ===== 右栏：推荐位 ===== */
  .right-rail {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .recommend {
    background: var(--color-bg-card);
    border: var(--border-default);
    border-radius: 0;
    padding: 16px;
  }
  .recommend h2 {
    font-size: 17px;
    margin: 0 0 12px;
    color: var(--color-text-primary);
  }
  .recommend-item {
    padding: 10px 0;
    display: flex;
    gap: 11px;
    border-bottom: var(--border-default);
  }
  .recommend-item:last-child {
    border: 0;
  }
  .rank {
    color: var(--color-brand);
    font-size: var(--text-xs);
    font-weight: 600;
    font-family: var(--font-family-serif);
  }
  .recommend-item b {
    font-size: var(--text-sm);
    display: block;
    color: var(--color-text-primary);
  }
  .recommend-item a {
    text-decoration: none;
    color: inherit;
  }
  .recommend-item a:hover b {
    color: var(--color-brand);
  }
  .recommend-item small {
    color: var(--color-text-tertiary);
    display: block;
    margin-top: 4px;
    font-size: var(--text-xs);
  }
  .rail-foot {
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    line-height: 1.8;
  }

  .mobile-hero,
  .mobile-categories {
    display: none;
  }

  @media (max-width: 999px) {
    .proto-home { grid-template-columns: 200px minmax(0, 1fr); }
    .right-rail { display: none; }
  }
  @media (max-width: 767px) {
    .container { padding-inline: 10px !important; }
    .mobile-hero {
      position: relative;
      display: block;
      height: 154px;
      padding: 14px 16px 0;
      box-sizing: border-box;
      background: var(--color-brand);
      color: #fff;
    }
    .mobile-logo { padding-top: 42px; text-align: center; font-size: 33px; font-weight: 800; font-style: italic; line-height: 32px; letter-spacing: -2px; }
    .mobile-doc-link, .mobile-search-link { position: absolute; top: 9px; display: inline-flex; align-items: center; justify-content: center; min-width: 44px; height: 44px; padding: 0 8px; border: 1px solid rgb(255 255 255 / 32%); border-radius: 999px; background: rgb(255 255 255 / 10%); color: #fff; text-decoration: none; }
    .mobile-doc-link { left: 8px; gap: 4px; font-size: 11px; }
    .mobile-search-link { right: 58px; top: 14px; min-width: 34px; width: 34px; height: 34px; padding: 0; }
    .hero-stats { display: flex; justify-content: space-between; margin-top: 19px; }
    .hero-stats button { display: inline-flex; align-items: center; gap: 6px; padding: 0; border: 0; background: transparent; color: #dfe9ff; font: 16px inherit; }
    .hero-stats b { font-weight: 400; }
    .proto-home { grid-template-columns: minmax(0, 1fr); padding: 0; gap: 0; }
    .category-card { display: none; }
    .mobile-categories { display: flex; align-items: stretch; height: 52px; padding: 0 12px; border-bottom: 1px solid var(--color-border); background: var(--color-bg-card); gap: 18px; }
    .mobile-categories button { min-width: 0; padding: 0; border: 0; border-bottom: 2px solid transparent; background: transparent; color: var(--color-text-secondary); font-size: 14px; }
    .mobile-categories button.active { color: var(--color-brand); font-weight: 600; border-bottom-color: var(--color-brand); }
    .mobile-categories button.list { min-width: 44px; margin-left: auto; }
    .feed-toolbar { position: sticky; top: 0; z-index: 14; height: 52px; min-height: 52px; padding: 6px 12px; border-top: 0; border-bottom: 1px solid var(--color-border); border-radius: 0; background: var(--color-bg-card); }
    .filters { gap: 6px; overflow-x: auto; }
    .filter-btn { min-height: 38px; padding: 0 11px; font-size: 14px; }
    .publish { min-height: 38px; height: 38px; padding: 0 13px; font-size: 13px; }
    .thread-list { margin-top: 0; border-radius: 0; border-left: 0; border-right: 0; }
    .thread { padding: 17px 18px; }
    .thread-detail-link h2 { font-size: 15px; }
    .thread-detail-link p { font-size: 13px; }
    .thread-footer { gap: 12px; }
    .thread-footer span:first-child { max-width: 42vw; }
    .mobile-hero { display: block !important; }
    .mobile-logo { position: absolute !important; top: 56px !important; left: 0 !important; right: 0 !important; width: auto !important; height: 32px !important; margin: 0 !important; padding: 0 !important; line-height: 32px !important; }
    .hero-stats { position: absolute !important; left: 16px !important; right: 16px !important; bottom: 0 !important; width: auto !important; height: 58px !important; margin: 0 !important; padding: 0 !important; display: flex !important; align-items: center !important; }
  }
</style>
