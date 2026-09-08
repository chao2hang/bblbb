<script lang="ts">
  // 发现页（公开 SSR）：1:1 对齐原型 prototype/pages/discover.html
  // 三栏高密度社区布局：
  //   左：热门话题（category-card side-hot）
  //   中：信息流（feed-toolbar + 线程卡列表）
  //   右：推荐栏（热门板块 + 社区服务快捷链接）
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Seo from '$lib/components/Seo.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import { formatCount, formatRelative } from '$lib/utils';
  import type { DiscoverPageData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';

  // data.site：根 layout 注入的全站文案（0065）；隔离渲染时兜底解析。
  let { data }: { data: DiscoverPageData & { site?: SiteCopyView | null } } = $props();

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));

  const tags = $derived(data.tags);
  const posts = $derived(data.posts);
  const boards = $derived(data.boards);
  const error = $derived(data.error);

  let activeTab = $state<'hot' | 'latest' | 'featured'>('hot');
</script>

<Seo
  title="发现"
  description={`发现${site.siteName}里的热门内容与活跃成员`}
  og={{ type: 'website' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: `发现 · ${site.siteName}`
  }}
/>

<div class="container" id="page-discover">
  <h1 class="sr-only">发现</h1>
  <p class="sr-only">发现社区里的热门内容与活跃成员</p>
  <div class="proto-discover">
    <!-- 左栏：热门话题（原型 aside.category-card.side-hot） -->
    <aside class="category-card side-hot" aria-label="热门话题">
      <h2 class="side-title fire-c">
        <Icon name="flame" size={18} />热门话题
      </h2>
      {#each tags.slice(0, 6) as tag (tag.id)}
        <a href="/tags/{tag.slug}">
          <b># {tag.name}</b>
          <em>{formatCount(tag.usage_count)} 讨论</em>
        </a>
      {/each}
      {#if tags.length === 0}
        <div style="padding:14px;color:var(--color-text-tertiary);font-size:13px;">暂无热门标签</div>
      {/if}
    </aside>

    <!-- 中栏：信息流（原型 section.feed） -->
    <section class="feed" aria-label="发现内容流">
      <div class="feed-toolbar">
        <div class="filters">
          <button
            type="button"
            class="filter-btn {activeTab === 'hot' ? 'active' : ''}"
            onclick={() => (activeTab = 'hot')}
          >
            热门
          </button>
          <button
            type="button"
            class="filter-btn {activeTab === 'latest' ? 'active' : ''}"
            onclick={() => (activeTab = 'latest')}
          >
            最新
          </button>
          <button
            type="button"
            class="filter-btn {activeTab === 'featured' ? 'active' : ''}"
            onclick={() => (activeTab = 'featured')}
          >
            精华
          </button>
        </div>
        <a class="publish" href="/editor">发布</a>
      </div>

      <!-- 线程列表 -->
      <div class="thread-list">
        {#each posts as post (post.id)}
          {@const author = post.author?.username || '匿名'}
          <article class="thread">
            <Avatar name={author} size="lg" />
            <div class="thread-body">
              <div class="thread-meta">
                <b>{author}</b>
                <span>· {formatRelative(post.created_at)}</span>
              </div>
              <a class="thread-detail-link" href="/posts/{encodeURIComponent(post.id)}">
                <h2>{post.title}</h2>
                {#if post.summary}
                  <p>{post.summary}</p>
                {/if}
              </a>
              <div class="thread-footer">
                {#if post.board_name}
                  <span>{post.board_name}</span>
                {/if}
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
            <div class="empty-state-title">还没有活跃内容</div>
            <p class="empty-state-desc">社区的第一批讨论正等着你来发起</p>
            <a class="empty-state-cta" href="/editor" style="display:inline-flex;align-items:center;gap:6px;margin-top:14px;padding:8px 18px;border-radius:var(--radius-sm);background:var(--color-brand);color:#fff;font-size:var(--text-sm);font-weight:var(--weight-medium);text-decoration:none;">
              <Icon name="plus" size={15} />
              <span>发布第一篇内容</span>
            </a>
          </div>
        {/if}
      </div>

      <div class="feed-end">— 没有更多内容了 —</div>
    </section>

    <!-- 右栏：推荐板块与社区服务（原型 aside.right-rail） -->
    <aside class="right-rail">
      {#if boards.length > 0}
        <section class="recommend" aria-label="推荐板块">
          <h2>推荐板块</h2>
          {#each boards.slice(0, 4) as board (board.id)}
            {@const visuals = boardVisuals(board.slug)}
            <div class="recommend-item">
              <span class="app-board-card__icon" style="margin-bottom:0;width:32px;height:32px;flex:0 0 32px;">
                <Icon name={visuals.icon || 'workflow'} size={16} />
              </span>
              <div style="min-width:0;flex:1;">
                <a href="/boards/{board.slug}"><b>{board.name}</b></a>
                <small>{board.description || `${formatCount(board.post_count)} 讨论`}</small>
              </div>
            </div>
          {/each}
        </section>
      {/if}

      <section class="stats-card community-service" aria-label="社区服务">
        <h2>社区服务</h2>
        <nav class="service-links" aria-label="快捷入口">
          <a href="/editor">
            <span class="service-links__icon"><Icon name="edit-3" size={16} /></span>
            <span><b>发布内容</b><small>分享观点与创作</small></span>
            <span class="service-links__arrow"><Icon name="chevron-right" size={14} /></span>
          </a>
          <a href="/boards">
            <span class="service-links__icon"><Icon name="layout-dashboard" size={16} /></span>
            <span><b>浏览板块</b><small>发现感兴趣的讨论</small></span>
            <span class="service-links__arrow"><Icon name="chevron-right" size={14} /></span>
          </a>
          <a href="/achievements">
            <span class="service-links__icon"><Icon name="trophy" size={16} /></span>
            <span><b>成就墙</b><small>查看成长与勋章</small></span>
            <span class="service-links__arrow"><Icon name="chevron-right" size={14} /></span>
          </a>
        </nav>
      </section>

      <footer class="rail-foot">
        © {new Date().getFullYear()}<br />Powered By {site.siteName}
      </footer>
    </aside>
  </div>
</div>

<style>
  /* ===== 三栏骨架（原型 #page-discover：260px | 1fr | 300px，gap 20）===== */
  .container {
    padding-inline: 10px;
  }
  .proto-discover {
    display: grid;
    grid-template-columns: minmax(220px, 260px) minmax(0, 1fr) minmax(250px, 300px);
    gap: var(--space-5);
    align-items: start;
    padding: var(--space-5) 0 var(--space-6);
  }
  .proto-discover > * {
    min-width: 0;
  }

  /* ===== 左栏：热门话题 ===== */
  .side-hot {
    background: var(--color-bg-card);
    border: var(--border-default);
    border-radius: 0;
    box-shadow: none;
    height: max-content;
    padding: 6px 16px;
  }
  .side-title {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary);
    margin: 14px 0 8px;
    padding: 0 14px;
    font-family: var(--font-family-base);
  }
  .side-hot a {
    height: 52px;
    border-bottom: var(--border-default);
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--color-text-secondary);
    text-decoration: none;
    font-size: 14px;
    padding: 0 14px;
  }
  .side-hot a:last-child {
    border-bottom: 0;
  }
  .side-hot a:hover b {
    color: var(--color-brand);
  }
  .side-hot b {
    font-size: 14px;
    color: var(--color-text-primary);
  }
  .side-hot em {
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
  }
  .filters {
    display: flex;
    align-items: center;
    gap: 22px;
    height: 100%;
  }
  .filter-btn {
    border: 0;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    padding: 21px 0;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
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

  /* 线程列表 */
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
    font-size: 13px;
    color: var(--color-text-secondary);
  }
  .thread-meta span {
    color: var(--color-text-tertiary);
    margin-left: 5px;
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
  .feed-end {
    text-align: center;
    color: var(--color-text-tertiary);
    font-size: var(--text-xs);
    padding: var(--space-4) 0;
  }

  /* ===== 右栏：推荐卡与社区服务 ===== */
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
    align-items: center;
    gap: 11px;
    border-bottom: var(--border-default);
  }
  .recommend-item:last-child {
    border: 0;
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
    margin-top: 3px;
    font-size: var(--text-xs);
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

  /* 响应式断点 */
  @media (max-width: 999px) {
    .proto-discover {
      grid-template-columns: 200px minmax(0, 1fr);
    }
    .right-rail {
      display: none;
    }
  }
  @media (max-width: 767px) {
    .container {
      padding-inline: 0 !important;
    }
    .proto-discover {
      grid-template-columns: minmax(0, 1fr);
      padding: 0;
      gap: 0;
    }
    .side-hot {
      display: none;
    }
    .feed-toolbar {
      height: auto;
      min-height: 48px;
      padding: 6px 14px;
      border-radius: 0;
      border-bottom: 1px solid var(--color-border);
    }
    .filters {
      gap: 8px;
    }
    .filter-btn {
      padding: 8px 12px;
      font-size: 14px;
      border-bottom: 2px solid transparent;
    }
    .filter-btn.active {
      border-bottom-color: var(--color-brand);
    }
    .publish {
      height: 34px;
      padding: 0 14px;
      font-size: 13px;
    }
    .thread-list {
      margin-top: 0;
      border-radius: 0;
      border-left: 0;
      border-right: 0;
    }
    .thread {
      padding: 16px 14px;
    }
  }
</style>
