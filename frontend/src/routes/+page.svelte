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
  import TopicList, { type TopicListRow } from '$lib/components/forum/TopicList.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import { show } from '$lib/ui/toast';
  import { formatCount } from '$lib/utils';
  import Seo from '$lib/components/Seo.svelte';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';
  import type { HomePageData, HomeSort } from './+page.server';
  import { page } from '$app/state';
  import TopicComposer from '$lib/components/editor/TopicComposer.svelte';

  // data.site：根 layout 注入的全站文案（0065）；隔离渲染时兜底解析。
  let { data }: { data: HomePageData & { site?: SiteCopyView | null } } = $props();

  const user = $derived.by(() => {
    try {
      return page.data?.user ?? null;
    } catch {
      return null;
    }
  });
  const authed = $derived(Boolean(user));
  const composeOpen = $derived(page.url.searchParams.get('compose') === '1');

  type MobileSheet = 'categories' | 'tags' | 'sort' | null;
  let mobileSheet = $state<MobileSheet>(null);

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));

  const boards = $derived(data.boards);
  const totalPosts = $derived(
    data.stats?.posts ?? boards.reduce((n, b) => n + (b.post_count ?? 0), 0)
  );

  /** 板块 id → 板块投影（线程卡脚注用：名称 + 身份图标/颜色；
   *  boards 与 posts 同源取自 SSR load）。 */
  const boardOf = $derived(
    (id: string | null | undefined) => (id ? boards.find((b) => b.id === id) ?? null : null)
  );

  function homeHref(options: { sort?: HomeSort; boardId?: string | null; tag?: string | null } = {}): string {
    const params = new URLSearchParams();
    const nextSort = options.sort ?? data.sort;
    const nextBoardId = Object.prototype.hasOwnProperty.call(options, 'boardId') ? options.boardId : data.boardId;
    const nextTag = Object.prototype.hasOwnProperty.call(options, 'tag') ? options.tag : data.tag;
    if (nextSort) params.set('sort', nextSort);
    if (nextBoardId) params.set('board_id', nextBoardId);
    if (nextTag) params.set('tag', nextTag);
    const query = params.toString();
    return query ? `/?${query}` : '/';
  }

  /** 筛选 tab（M18-HOME-02 对齐原型：最新|精华|已关注|热门；原生链接）。 */
  const sortTabs = $derived<Array<{ value: HomeSort; label: string; href: string }>>([
    { value: '', label: '最新', href: homeHref({ sort: '' }) },
    { value: 'featured', label: '精华', href: homeHref({ sort: 'featured' }) },
    {
      value: 'following',
      label: '已关注',
      href: authed
        ? homeHref({ sort: 'following' })
        : `/login?next=${encodeURIComponent(homeHref({ sort: 'following' }))}`
    },
    { value: 'popular', label: '热门', href: homeHref({ sort: 'popular' }) }
  ]);

  const selectedBoard = $derived(boards.find((board) => board.id === data.boardId) ?? null);
  const selectedTag = $derived(data.tags.find((tag) => tag.slug === data.tag) ?? null);
  const activeSortTab = $derived(sortTabs.find((tab) => tab.value === data.sort) ?? sortTabs[0]);
  const mobileSheetTitle = $derived(
    mobileSheet === 'categories' ? '选择类别' : mobileSheet === 'tags' ? '选择标签' : '排序内容'
  );

  function openMobileSheet(sheet: Exclude<MobileSheet, null>): void {
    mobileSheet = sheet;
  }

  function closeMobileSheet(): void {
    mobileSheet = null;
  }

  function handleMobileSheetKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && mobileSheet) closeMobileSheet();
  }

  // ── 加载更多（游标 = 上一页最后一条 created_at；JS 客户端追加、
  //    无 JS 回退 ?after= 链接整页翻页）──
  type PostRow = HomePageData['posts'][number];
  let extraPages = $state<PostRow[]>([]);
  let loadedCursor = $state<string | null | undefined>(undefined);
  let loadingMore = $state(false);

  const posts = $derived<PostRow[]>([...data.posts, ...extraPages]);
  const cursor = $derived(loadedCursor === undefined ? data.nextCursor : loadedCursor);

  /** 传给共享 TopicList 的行投影（作者/板块脚注在页内解析）。 */
  const listRows = $derived<TopicListRow[]>(
    posts.map((p) => {
      const board = boardOf(p.board_id);
      return {
        id: p.id,
        title: p.title,
        author: p.author_display_name ?? p.author_name ?? '匿名',
        authorUsername: p.author_name ?? null,
        authorPresentation: p.author_presentation_tokens ?? null,
        authorAvatarAttachmentId: p.author_avatar_attachment_id ?? null,
        boardLabel: board?.name ?? null,
        boardSlug: board?.slug ?? null,
        boardIcon: board?.icon ?? null,
        likeCount: p.like_count ?? 0,
        replyCount: p.reply_count,
        viewCount: p.view_count,
        pinned: p.pinned,
        featured: p.is_featured,
        createdAt: p.created_at,
        lastReplyAt: p.last_reply_at,
        participants: p.participants ?? []
      };
    })
  );

  /** 空态标题按排序区分（共享 TopicList 空态）。 */
  const emptyTitle = $derived(
    data.sort === 'featured'
      ? '暂无精华帖'
      : data.sort === 'following'
        ? '暂无关注动态'
        : data.sort === 'popular'
          ? '暂无热门帖'
          : '暂无帖子'
  );
  const loadMoreHref = $derived(
    cursor
      ? `${homeHref()}${homeHref() === '/' ? '?' : '&'}after=${encodeURIComponent(cursor)}`
      : homeHref()
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
      is_featured: p.is_featured ?? Boolean(p.featured_at),
      reply_count: p.reply_count,
      view_count: p.view_count,
      like_count: p.like_count ?? 0,
      pinned: p.pinned ?? Boolean(p.pinned_at),
      created_at: p.created_at,
      last_reply_at: p.last_reply_at ?? null,
      author_presentation_tokens: p.author?.presentation_tokens ?? null,
      author_avatar_attachment_id: p.author?.avatar_attachment_id ?? null,
      participants: (p.participants ?? []).map((u) => ({
        id: u.id,
        username: u.username ?? null,
        display_name: u.display_name ?? null,
        avatar_attachment_id: u.avatar_attachment_id ?? null,
        presentation_tokens: u.presentation_tokens ?? null
      }))
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
      if (data.boardId) params.set('board_id', data.boardId);
      if (data.tag) params.set('tag', data.tag);
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
  description={site.siteDescription}
  og={{ type: 'website', siteName: site.siteName }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'WebSite',
    name: site.siteName,
    description: site.siteDescription
  }}
/>

<svelte:window onkeydown={handleMobileSheetKeydown} />

<div class="container" id="page-home">
  <h1 class="sr-only">首页</h1>

  <!-- 顶部板块快捷滑动条（小屏友好，单轨无缝响应） -->
  <nav class="home-category-bar" aria-label="板块快捷导航">
    <a href="/" class="home-cat-chip {!data.sort ? 'active' : ''}">
      <span>全部</span>
      <em class="home-cat-chip__count">{formatCount(totalPosts)}</em>
    </a>
    {#each boards as board (board.id)}
      {@const visuals = boardVisuals(board.slug, board.icon)}
      <a href="/boards/{board.slug}" class="home-cat-chip">
        <span class="home-cat-chip__icon" style="color:{visuals.color};"><Icon name={visuals.icon} size={13} /></span>
        <span>{board.name}</span>
        <em class="home-cat-chip__count">{formatCount(board.post_count)}</em>
      </a>
    {/each}
  </nav>

  <div class="proto-home">
    <!-- 左栏：分类导航（大屏展示，平板/移动端隐藏且由上方滑动条承接） -->
    <aside class="category-card" aria-label="板块分类">
      <a class="selected" href="/">
        <b>全部</b><em>{formatCount(totalPosts)}</em>
      </a>
      {#each boards as board (board.id)}
        {@const visuals = boardVisuals(board.slug, board.icon)}
        <a href="/boards/{board.slug}">
          <span class="cat-name">
            <span class="cat-name__icon" style="color:{visuals.color};"><Icon name={visuals.icon} size={15} /></span>
            {board.name}
          </span>
          <em>{formatCount(board.post_count)}</em>
        </a>
      {/each}
    </aside>

    <!-- 中栏：信息流（原型 section.feed） -->
    <section class="feed" aria-label="最新讨论">
      <div class="feed-toolbar">
        <!-- 移动端：仅保留类别入口 + 排序；标签经「发现」/标签页浏览 -->
        <nav class="mobile-home-controls" aria-label="浏览筛选入口">
          <a
            href="/boards"
            class="mobile-home-control"
            class:active={!!selectedBoard}
            onclick={(event) => {
              event.preventDefault();
              openMobileSheet('categories');
            }}
          >
            <span>{selectedBoard?.name ?? '类别'}</span>
            <Icon name="chevron-down" size={14} />
          </a>
          <span class="mobile-home-controls__divider" aria-hidden="true"></span>
          <a
            href={activeSortTab?.href ?? '/'}
            class="mobile-home-sort-control"
            onclick={(event) => {
              event.preventDefault();
              openMobileSheet('sort');
            }}
          >
            <span>{activeSortTab?.label ?? '最新'}</span>
            <Icon name="chevron-down" size={14} />
          </a>
        </nav>
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
        <a
          class="publish"
          href={authed ? '/?compose=1' : `/login?next=${encodeURIComponent('/?compose=1')}`}
        >
          {authed ? '发布内容' : '登录后发布'}
        </a>
        <!-- 移动端：FAB 是唯一发布入口，工具条右侧保留搜索图标位 -->
        <a class="feed-search" href="/search" aria-label="搜索" title="搜索">
          <Icon name="search" size={18} />
        </a>
      </div>

      <TopicList
        rows={listRows}
        {emptyTitle}
        emptyDesc="成为第一个发帖的人吧！"
        emptyCta={{
          href: authed ? '/?compose=1' : `/login?next=${encodeURIComponent('/?compose=1')}`,
          label: authed ? '发布第一篇内容' : '登录后发布内容'
        }}
      />

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
      <section class="recommend" aria-label="推荐内容">
        <h2>推荐内容</h2>
        {#if data.popular.length > 0}
          {#each data.popular as p, i (p.id)}
            <div class="recommend-item">
              <span class="rank">{String(i + 1).padStart(2, '0')}</span>
              <div>
                <a href="/posts/{encodeURIComponent(p.id)}"><b>{p.title}</b></a>
                <small>{formatCount(p.view_count)} 浏览 · {formatCount(p.reply_count)} 回复</small>
              </div>
            </div>
          {/each}
        {:else}
          <div class="recommend-empty">
            <p>暂无热门推荐</p>
            <small>发布新讨论后将在此展示</small>
          </div>
        {/if}
      </section>

      <section class="stats-card community-service" aria-label="社区服务">
        <h2>{authed ? '社区服务' : '快捷入口'}</h2>
        <nav class="service-links" aria-label="快捷入口">
          <a href={authed ? '/?compose=1' : `/login?next=${encodeURIComponent('/?compose=1')}`}>
            <span class="service-links__icon"><Icon name="edit-3" size={16} /></span>
            <span><b>{authed ? '发布内容' : '登录发布'}</b><small>{authed ? '分享观点与创作' : '登录后开始创作'}</small></span>
            <span class="service-links__arrow"><Icon name="chevron-right" size={14} /></span>
          </a>
          <a href="/boards">
            <span class="service-links__icon"><Icon name="layout-dashboard" size={16} /></span>
            <span><b>浏览板块</b><small>发现感兴趣的讨论</small></span>
            <span class="service-links__arrow"><Icon name="chevron-right" size={14} /></span>
          </a>
          {#if authed}
            <a href="/achievements">
              <span class="service-links__icon"><Icon name="trophy" size={16} /></span>
              <span><b>成就墙</b><small>查看成长与勋章</small></span>
              <span class="service-links__arrow"><Icon name="chevron-right" size={14} /></span>
            </a>
          {:else}
            <a href="/login">
              <span class="service-links__icon"><Icon name="log-in" size={16} /></span>
              <span><b>登录 / 注册</b><small>加入社区交流讨论</small></span>
              <span class="service-links__arrow"><Icon name="chevron-right" size={14} /></span>
            </a>
          {/if}
        </nav>
      </section>

      <footer class="rail-foot">© {new Date().getFullYear()}<br />Powered By {site.siteName}</footer>
    </aside>
  </div>
</div>

{#if mobileSheet}
  <div class="mobile-sheet-root">
    <button type="button" class="mobile-sheet-backdrop" aria-label="关闭筛选" onclick={closeMobileSheet}></button>
    <div class="mobile-sheet" role="dialog" aria-modal="true" aria-labelledby="mobile-sheet-title">
      <div class="mobile-sheet-handle" aria-hidden="true"></div>
      <header class="mobile-sheet-header">
        <h2 id="mobile-sheet-title">{mobileSheetTitle}</h2>
        <button type="button" class="mobile-sheet-close" aria-label="关闭筛选" onclick={closeMobileSheet}>
          <Icon name="x" size={18} />
        </button>
      </header>

      {#if mobileSheet === 'categories'}
        <div class="mobile-sheet-list" role="listbox" aria-label="选择类别">
          <a
            href={homeHref({ boardId: null })}
            class:selected={!data.boardId}
            class="mobile-sheet-option"
            onclick={closeMobileSheet}
          >
            <span class="mobile-sheet-option__main"><Icon name="layout-dashboard" size={17} />全部类别</span>
            <span class="mobile-sheet-option__meta">{formatCount(totalPosts)}</span>
          </a>
          {#each boards as board (board.id)}
            {@const visuals = boardVisuals(board.slug, board.icon)}
            <a
              href={homeHref({ boardId: board.id })}
              class:selected={data.boardId === board.id}
              class="mobile-sheet-option"
              onclick={closeMobileSheet}
            >
              <span class="mobile-sheet-option__main"><span style="display:inline-flex;color:{visuals.color};"><Icon name={visuals.icon} size={17} /></span>{board.name}</span>
              {#if typeof board.post_count === 'number'}
                <span class="mobile-sheet-option__meta">{formatCount(board.post_count)}</span>
              {/if}
            </a>
          {/each}
        </div>
      {:else if mobileSheet === 'tags'}
        <div class="mobile-sheet-list" role="listbox" aria-label="选择标签">
          <a
            href={homeHref({ tag: null })}
            class:selected={!data.tag}
            class="mobile-sheet-option"
            onclick={closeMobileSheet}
          >
            <span class="mobile-sheet-option__main"><Icon name="tag" size={17} />全部标签</span>
            <span class="mobile-sheet-option__meta">{formatCount(data.tags.length)}</span>
          </a>
          {#each data.tags as tag (tag.id)}
            <a
              href={homeHref({ tag: tag.slug })}
              class:selected={data.tag === tag.slug}
              class="mobile-sheet-option"
              onclick={closeMobileSheet}
            >
              <span class="mobile-sheet-option__main"><Icon name="tag" size={17} />{tag.name}</span>
              <span class="mobile-sheet-option__meta">{formatCount(tag.usage_count)}</span>
            </a>
          {:else}
            <div class="mobile-sheet-empty">暂时没有可用标签</div>
          {/each}
        </div>
      {:else}
        <div class="mobile-sheet-list" role="listbox" aria-label="选择排序">
          {#each sortTabs as tab (tab.value)}
            <a
              href={tab.href}
              class:selected={data.sort === tab.value}
              class="mobile-sheet-option"
              onclick={closeMobileSheet}
            >
              <span class="mobile-sheet-option__main"><Icon name={tab.value === 'popular' ? 'flame' : tab.value === 'featured' ? 'star' : tab.value === 'following' ? 'users' : 'clock'} size={17} />{tab.label}</span>
              {#if data.sort === tab.value}<Icon name="check" size={17} />{/if}
            </a>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

{#if composeOpen}
  <TopicComposer />
{/if}

<style>
  /* 原型 .layout：内容宽 1420、左右 10px 内边距（高密度） */
  .container {
    padding-inline: 10px;
  }

  /* 工具条搜索入口：仅移动端展示（≤767px 由 mobile.css 启用），桌面隐藏 */
  .feed-search {
    display: none;
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
  /* 板块行：持久化图标（boards.icon，回退 slug 映射）+ 名称 */
  .category-card .cat-name {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* 图标容器自身为 flex：svg 脱离文本基线排版，与 CJK 文字几何居中
     （inline span 直包 svg 会因基线降部空隙整体偏高）。 */
  .category-card .cat-name__icon {
    display: inline-flex;
    align-items: center;
    line-height: 0;
  }
  .category-card .cat-name :global(svg) {
    flex: 0 0 auto;
    display: block;
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
    color: #07151a;
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
  .recommend-empty {
    padding: 18px 8px;
    text-align: center;
  }
  .recommend-empty p {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }
  .recommend-empty small {
    display: block;
    margin-top: 4px;
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
  }

  .community-service {
    background: var(--color-bg-card);
    border: var(--border-default);
    border-radius: 0;
    padding: 16px;
  }
  .community-service h2 {
    font-size: 17px;
    margin: 0 0 12px;
    color: var(--color-text-primary);
  }
  .service-links {
    display: flex;
    flex-direction: column;
  }
  .service-links a {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr) 14px;
    align-items: center;
    gap: 10px;
    padding: 11px 0;
    border-bottom: var(--border-default);
    color: inherit;
    text-decoration: none;
  }
  .service-links a:last-child {
    border-bottom: 0;
  }
  .service-links a:hover b {
    color: var(--color-brand);
  }
  .service-links__icon {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 2px;
    background: var(--color-bg-page);
    color: var(--color-text-secondary);
  }
  .service-links a:hover .service-links__icon {
    background: var(--color-brand-soft);
    color: var(--color-brand);
  }
  .service-links b {
    color: var(--color-text-primary);
    font-size: 13px;
    display: block;
  }
  .service-links small {
    color: var(--color-text-tertiary);
    font-size: 11px;
    display: block;
    margin-top: 2px;
  }
  .service-links__arrow {
    color: var(--color-text-tertiary);
  }

  .rail-foot {
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    line-height: 1.8;
  }

  /* ===== 板块快捷滑动条（小屏优先展示，大屏有左侧导航栏故隐藏） ===== */
  .home-category-bar {
    display: none;
    align-items: center;
    gap: var(--space-2);
    overflow-x: auto;
    scrollbar-width: none;
    -webkit-overflow-scrolling: touch;
    padding: var(--space-2) 0;
    margin-bottom: var(--space-3);
  }
  .home-category-bar::-webkit-scrollbar {
    display: none;
  }
  .home-cat-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border-radius: var(--radius-full);
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    font-weight: 500;
    text-decoration: none;
    white-space: nowrap;
    flex-shrink: 0;
    transition: all 0.15s ease;
  }
  .home-cat-chip:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }
  .home-cat-chip.active {
    background: var(--color-brand);
    color: var(--color-text-on-brand);
    font-weight: 600;
  }
  .home-cat-chip__count {
    font-style: normal;
    font-size: 11px;
    opacity: 0.8;
  }
  /* chip 板块图标：持久化 boards.icon（回退 slug 映射），active 态保持板块色可辨 */
  .home-cat-chip__icon {
    display: inline-flex;
    align-items: center;
  }
  .home-cat-chip__icon :global(svg) {
    flex: 0 0 auto;
  }

  @media (max-width: 999px) {
    .home-category-bar {
      display: flex;
    }
    .proto-home {
      grid-template-columns: minmax(0, 1fr);
      padding: 0;
      gap: 0;
    }
    .category-card {
      display: none;
    }
    .right-rail {
      display: none;
    }
  }

  @media (max-width: 767px) {
    .container {
      padding-inline: var(--space-3);
    }
    .feed-toolbar {
      height: 48px;
      padding: 0 12px;
      border-radius: var(--aui-radius-sm, 4px);
    }
    .filters {
      gap: 16px;
      overflow-x: auto;
      scrollbar-width: none;
    }
    .filters::-webkit-scrollbar {
      display: none;
    }
    .filter-btn {
      padding: 12px 0;
      font-size: 14px;
      white-space: nowrap;
    }
    .publish {
      display: none !important;
    }
  }
</style>
