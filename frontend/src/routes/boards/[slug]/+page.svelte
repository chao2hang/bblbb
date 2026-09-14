<script lang="ts">
  // M03-UI-06 + M18-BOARD-05：板块分类页与首页完全同构——
  //   左：板块分类导航（category-card，当前板块高亮）
  //   中：信息流（feed-toolbar 筛选 + TopicList + 加载更多，按板块裁剪）
  //   右：推荐位（推荐内容 = 本板块热门前 3，每个分区推荐内容不同 + 社区服务）
  //
  // - 数据层与首页共用 $lib/api/feed（GET /api/v1/posts?board_id=…，cursor
  //   分页、参与者预览、白名单投影一致）；排序 tab 与首页一致
  //   （最新 | 精华 | 已关注 | 热门）；「加载更多」JS 追加、无 JS ?after= 回退。
  // - 发布入口与首页同款：compose=1 唤起 TopicComposer（组件自读 URL 参数）。
  // - 关注板块保留（工具条内；真实鉴权由 ?/follow action 服务端裁决）。
  // - 小屏（<1000px）：左栏/右栏隐藏，顶部板块 chip 条承接导航（与首页一致）。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import { page } from '$app/state';
  import TopicList, { type TopicListRow } from '$lib/components/forum/TopicList.svelte';
  import BoardNav from '$lib/components/forum/BoardNav.svelte';
  import TopicComposer from '$lib/components/editor/TopicComposer.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import { formatCount } from '$lib/utils';
  import { show } from '$lib/ui/toast';
  import Seo from '$lib/components/Seo.svelte';
  import type { PostSummary } from '$lib/api/client';
  import type { BoardDetailData, BoardFollowActionData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';

  let { data, form }: { data: BoardDetailData & { site?: SiteCopyView | null }; form?: BoardFollowActionData | null } = $props();

  // 会话态以根 layout 服务端 /me 验证结果为准（与首页同源），不再用
  // 「Cookie 是否存在」判定（失效 Cookie 曾让匿名访客看到回复表单）。
  const user = $derived.by(() => {
    try {
      return page.data?.user ?? null;
    } catch {
      return null;
    }
  });
  const authed = $derived(Boolean(user));
  // compose=1 唤起发布器；page.url 在隔离渲染（SSR 单测）外不可达 → 兜底 false。
  const composeOpen = $derived.by(() => {
    try {
      return page.url.searchParams.get('compose') === '1';
    } catch {
      return false;
    }
  });

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));

  const board = $derived(data.board);
  const error = $derived(data.error);
  const slug = $derived(board?.slug ?? '');

  /** 侧栏「板块导航」数据：全量板块列表（服务端已按请求方可见性裁剪）。 */
  const navBoards = $derived(data.boards ?? []);

  /** 权限提示：非公开板块对匿名/非成员不可见（members 需登录、
   *  restricted 需角色）；readonly/closed 提示只读。 */
  const permissionHint = $derived.by(() => {
    if (!board) return null;
    const hints: string[] = [];
    if (board.visibility === 'members') hints.push('该板块仅对登录成员可见');
    if (board.visibility === 'restricted') hints.push('该板块需加入后可见');
    if (board.visibility === 'hidden') hints.push('该板块仅对具备权限的管理员/版主可见');
    if (board.posting_mode === 'readonly') hints.push('该板块当前为只读，不能发布新帖');
    if (board.posting_mode === 'closed') hints.push('该板块已关闭发帖');
    if (board.posting_mode === 'approval') hints.push('发帖需审核后展示');
    return hints.length ? hints : null;
  });

  const indexable = $derived(
    Boolean(
      board &&
        (board.visibility ?? 'public') === 'public' &&
        board.is_active !== 0
    )
  );

  // ── 加载更多（与首页同构）：游标 = 上一页最后一条 created_at；
  //    JS 客户端追加、无 JS 回退 ?after= 链接整页翻页。──
  type FeedPost = BoardDetailData['posts'][number];
  let extraPages = $state<FeedPost[]>([]);
  let loadedCursor = $state<string | null | undefined>(undefined);
  let loadingMore = $state(false);

  const posts = $derived<FeedPost[]>([...data.posts, ...extraPages]);
  const cursor = $derived(loadedCursor === undefined ? data.nextCursor : loadedCursor);

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
  function toRow(p: PostSummary): FeedPost {
    return {
      id: p.id,
      title: p.title,
      author_id: p.author_id ?? p.author?.id,
      author_name: p.author_name ?? p.author?.username ?? null,
      author_display_name: p.author_display_name ?? p.author?.display_name ?? null,
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
    if (!cursor || loadingMore || !board) return;
    loadingMore = true;
    try {
      const params = new URLSearchParams({ limit: '8' });
      if (data.sort) params.set('sort', data.sort);
      params.set('board_id', board.id);
      params.set('after', cursor);
      const response = await fetch(`/api/v1/posts?${params.toString()}`, {
        credentials: 'same-origin',
        headers: { Accept: 'application/json' }
      });
      if (!response.ok) throw new Error(`posts page ${response.status}`);
      const next = (await response.json()) as {
        items?: PostSummary[];
        next_cursor?: string | null;
        page?: { next_cursor?: string | null };
      };
      extraPages = [...extraPages, ...(next.items ?? []).map(toRow)];
      loadedCursor = next.page?.next_cursor ?? next.next_cursor ?? null;
    } catch {
      show('加载更多失败，请重试', 'danger');
    }
    loadingMore = false;
  }

  /** 排序 tab（与首页同款：最新 | 精华 | 已关注 | 热门），留在本板块内筛选。 */
  const feedBase = $derived(`/boards/${encodeURIComponent(slug)}`);
  const sortTabs = $derived<Array<{ value: BoardDetailData['sort']; label: string; href: string }>>([
    { value: '', label: '最新', href: feedBase },
    { value: 'featured', label: '精华', href: `${feedBase}?sort=featured` },
    {
      value: 'following',
      label: '已关注',
      href: authed
        ? `${feedBase}?sort=following`
        : `/login?next=${encodeURIComponent(`${feedBase}?sort=following`)}`
    },
    { value: 'popular', label: '热门', href: `${feedBase}?sort=popular` }
  ]);

  /** 发布入口（与首页同款 compose=1 唤起 TopicComposer）。 */
  const publishHref = $derived(
    authed ? `${feedBase}?compose=1` : `/login?next=${encodeURIComponent(`${feedBase}?compose=1`)}`
  );

  /** 无 JS「加载更多」整页回退链接（保留当前排序 + after 游标）。 */
  const loadMoreHref = $derived.by(() => {
    const params = new URLSearchParams();
    if (data.sort) params.set('sort', data.sort);
    if (cursor) params.set('after', cursor);
    const qs = params.toString();
    return qs ? `${feedBase}?${qs}` : feedBase;
  });

  /** 板块 id → 板块投影（线程卡脚注用：名称 + 身份图标/颜色；同源取自
   *  boards 与当前 board；id 未命中时回退当前板块，与旧行为一致）。 */
  const boardOf = $derived((id: string | null | undefined) => {
    if (id) {
      const nav = navBoards.find((b) => b.id === id);
      if (nav) return nav;
    }
    return board ?? null;
  });

  /** 传给共享 TopicList 的行投影（与首页同构，显示板块标识以保持列表一致）。 */
  const listRows = $derived<TopicListRow[]>(
    posts.map((p) => {
      const rowBoard = boardOf(p.board_id);
      return {
        id: p.id,
        title: p.title,
        author: p.author_display_name ?? p.author_name ?? '匿名',
        authorUsername: p.author_name ?? null,
        authorPresentation: p.author_presentation_tokens ?? null,
        authorAvatarAttachmentId: p.author_avatar_attachment_id ?? null,
        boardLabel: rowBoard?.name ?? null,
        boardSlug: rowBoard?.slug ?? null,
        boardIcon: rowBoard?.icon ?? null,
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

  /** 空态文案按排序区分（共享 TopicList 空态，与首页同规则）。 */
  const emptyTitle = $derived(
    data.sort === 'featured'
      ? '暂无精华帖'
      : data.sort === 'following'
        ? '暂无关注动态'
        : data.sort === 'popular'
          ? '暂无热门帖'
          : '暂无帖子'
  );

  function followEnhance() {
    return async ({ result, update }: { result: { type: string; data?: unknown }; update: () => Promise<void> }) => {
      if (result.type === 'success') {
        const actionData = result.data as BoardFollowActionData | undefined;
        show(actionData?.message ?? '操作成功', 'success');
        await update();
        await invalidateAll();
      } else if (result.type === 'failure') {
        const actionData = result.data as BoardFollowActionData | undefined;
        show(actionData?.message ?? '操作失败，请稍后重试', 'danger');
        await update();
      } else {
        await update();
      }
    };
  }
</script>

<Seo
  title={board?.name ?? slug}
  description={board?.description ?? '板块：' + (board?.name ?? slug)}
  noindex={!indexable}
  og={{ type: 'website' }}
  jsonLd={
    indexable
      ? {
          '@context': 'https://schema.org',
          '@type': 'CollectionPage',
          name: board!.name,
          description: board!.description
        }
      : null
  }
/>

<div class="container" id="page-board">
  {#if error && !board}
    <div class="app-notice is-danger" role="alert">
      <span>{error}</span>
      <a href={feedBase}>重新加载</a>
    </div>
  {/if}

  {#if board}
    <h1 class="sr-only">{board.name}</h1>

      <div class="board-mobile-context" aria-label="当前板块">
        <a href="/boards" class="board-mobile-context__back" aria-label="返回板块列表">
          <Icon name="chevron-left" size={17} />
          <span>板块</span>
        </a>
        <div class="board-mobile-context__identity">
          <strong>{board.name}</strong>
          <span>{typeof board.post_count === 'number' ? `${formatCount(board.post_count)} 个主题` : '讨论板块'}</span>
        </div>
        <a href={publishHref} class="board-mobile-context__action" aria-label="发布内容">
          <Icon name="plus" size={16} />
        </a>
      </div>

      <!-- 小屏板块快捷滑动条（与首页 home-category-bar 同款：<1000px 展示，
           大屏由左栏分类卡承接）。「全部」→ 首页全部信息流（与左栏同语义）。 -->
      <nav class="board-category-bar" aria-label="板块快捷导航">
        <a href="/" class="board-cat-chip" class:active={!slug} aria-current={!slug ? 'page' : undefined}>
          <span>全部</span>
        </a>
        {#each navBoards as chipBoard (chipBoard.id)}
          {@const chipVisuals = boardVisuals(chipBoard.slug, chipBoard.icon)}
          <a
            href="/boards/{chipBoard.slug}"
            class="board-cat-chip"
            class:active={chipBoard.slug === slug}
            aria-current={chipBoard.slug === slug ? 'page' : undefined}
          >
            <span class="board-cat-chip__icon" style="color:{chipVisuals.color};" aria-hidden="true">
              <Icon name={chipVisuals.icon || 'workflow'} size={13} />
            </span>
            <span>{chipBoard.name}</span>
            {#if typeof chipBoard.post_count === 'number'}
              <em class="board-cat-chip__count">{formatCount(chipBoard.post_count)}</em>
            {/if}
          </a>
        {/each}
      </nav>

      <!-- 三栏骨架（与首页 proto-home 完全同构）：左栏分类导航 | 信息流 | 推荐位 -->
      <div class="proto-home">
        <!-- 左栏：板块分类导航（首页同款 category-card，≥1000px 展示） -->
        <BoardNav boards={navBoards} activeSlug={slug} />

        <!-- 中栏：信息流（按板块裁剪，工具条/列表/加载更多与首页一致） -->
        <section class="feed" aria-label="板块讨论">
          {#if permissionHint}
            <div class="app-notice" role="note" style="margin-bottom:14px;">
              <div>
                {#each permissionHint as hint}
                  <p style="margin:0;">{hint}</p>
                {/each}
              </div>
            </div>
          {/if}

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
            <div class="toolbar-side">
              {#if authed}
                <form
                  method="POST"
                  action={data.following ? '?/unfollow' : '?/follow'}
                  use:enhance={followEnhance}
                >
                  <button type="submit" class="btn {data.following ? 'secondary' : 'ghost'} sm">
                    <Icon name={data.following ? 'check' : 'plus'} size={14} />
                    {data.following ? '已关注' : '关注板块'}
                  </button>
                </form>
              {:else}
                <a href={`/login?next=${encodeURIComponent(`${feedBase}`)}`} class="btn ghost sm">
                  <Icon name="log-in" size={14} />
                  登录后关注
                </a>
              {/if}
              <a class="publish" href={publishHref}>
                {authed ? '发布内容' : '登录后发布'}
              </a>
            </div>
            <a class="feed-search" href="/search" aria-label="搜索" title="搜索">
              <Icon name="search" size={18} />
            </a>
          </div>

          <TopicList
            rows={listRows}
            {emptyTitle}
            emptyDesc="成为第一个发帖的人吧！"
            emptyCta={{ href: publishHref, label: authed ? '发布第一篇内容' : '登录后发布内容' }}
          />

          {#if cursor}
            <a class="load-more" href={loadMoreHref} onclick={(event) => void loadMore(event)}>
              {loadingMore ? '加载中…' : '加载更多'}
            </a>
          {:else if posts.length > 0}
            <div class="feed-end">— 已经到底了 —</div>
          {/if}
        </section>

        <!-- 右栏：推荐位（与首页 right-rail 同构；推荐内容按当前板块过滤） -->
        <aside class="right-rail">
          <section class="recommend" aria-label="本板块热门推荐">
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
              <a href={publishHref}>
                <span class="service-links__icon"><Icon name="edit-3" size={16} /></span>
                <span><b>发布内容</b><small>在本板块发起讨论</small></span>
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
    {/if}
</div>

{#if composeOpen}
  <TopicComposer />
{/if}

<style>
  /* 原型 .layout：内容宽 1420、左右 10px 内边距（高密度，与首页一致） */
  .container {
    padding-inline: 10px;
  }

  .board-mobile-context {
    display: none;
  }

  /* ===== 三栏骨架（与首页 proto-home 保持一致：260px | 1fr | 300px，gap 20）===== */
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

  /* ===== 小屏板块快捷滑动条（与首页 home-category-bar 同款）=====
     大屏由左栏分类卡承接，默认隐藏；≤999px 与左栏互换。 */
  .board-category-bar {
    display: none;
    align-items: center;
    gap: var(--space-2);
    overflow-x: auto;
    scrollbar-width: none;
    -webkit-overflow-scrolling: touch;
    padding: var(--space-2) 0;
    margin-bottom: var(--space-3);
  }

  .board-category-bar::-webkit-scrollbar {
    display: none;
  }

  .board-cat-chip {
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

  .board-cat-chip:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }

  .board-cat-chip.active {
    background: var(--color-brand);
    color: var(--color-text-on-brand);
    font-weight: 600;
  }

  .board-cat-chip__count {
    font-style: normal;
    font-size: 11px;
    opacity: 0.8;
  }

  .board-cat-chip__icon {
    display: inline-flex;
    align-items: center;
  }

  .board-cat-chip__icon :global(svg) {
    flex: 0 0 auto;
  }

  /* ===== 中栏工具条（逐字对齐首页 .feed-toolbar；关注板块为本页保留项）==== */
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

  .toolbar-side {
    display: flex;
    align-items: center;
    gap: 10px;
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

  .feed-search {
    display: none;
  }

  /* ===== 加载更多（与首页同款）==== */
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

  /* ===== 右栏推荐位（逐字对齐首页 .right-rail；推荐内容按板块过滤）==== */
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

  @media (max-width: 999px) {
    .board-category-bar {
      display: flex;
    }

    .proto-home {
      grid-template-columns: minmax(0, 1fr);
      padding: 0;
      gap: 0;
    }

    :global(#page-board .category-card) {
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
