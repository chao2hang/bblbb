<script lang="ts">
  // 发现页（公开 SSR）：算法推荐流（前端先行，推荐算法后做）。
  // 两栏布局（推荐算法未上线前以热门内容兜底）：
  //   中：推荐信息流（feed-title + TopicList，与首页列表同构）
  //   右：推荐板块 + 社区服务快捷链接（桌面展示，小屏隐藏）
  // 旧版的热门标签 chip / 侧栏话题 / 手动排序 tab（热门|最新|精华）已随
  // 「算法推送」产品决策移除——本页不再提供手动排序与标签入口。
  import Icon from '$lib/components/ui/Icon.svelte';
  import Seo from '$lib/components/Seo.svelte';
  import TopicList, { type TopicListRow } from '$lib/components/forum/TopicList.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import { formatCount } from '$lib/utils';
  import type { DiscoverPageData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';
  import { page } from '$app/state';

  // data.site：根 layout 注入的全站文案（0065）；隔离渲染时兜底解析。
  let { data }: { data: DiscoverPageData & { site?: SiteCopyView | null } } = $props();

  const user = $derived.by(() => {
    try {
      return page.data?.user ?? null;
    } catch {
      return null;
    }
  });
  const authed = $derived(Boolean(user));

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));

  const posts = $derived(data.posts);
  const boards = $derived(data.boards);
  const error = $derived(data.error);

  /** board_id → 持久化板块图标（boards.icon；行徽标用，未命中为 null）。 */
  const boardIconOf = $derived((id: string | null | undefined) => {
    if (!id) return null;
    return boards.find((b) => b.id === id)?.icon ?? null;
  });

  /** 传给共享 TopicList 的行投影（与首页列表同构）。 */
  const listRows = $derived<TopicListRow[]>(
    posts.map((p) => ({
      id: p.id,
      title: p.title,
      author:
        p.author?.display_name || p.author_display_name || p.author?.username ||
        '匿名',
      authorUsername: p.author?.username ?? p.author_name ?? null,
      authorPresentation: p.author?.presentation_tokens ?? null,
      authorAvatarAttachmentId: p.author?.avatar_attachment_id ?? null,
      boardLabel: p.board_name ?? null,
      boardSlug: p.board_slug ?? null,
      boardIcon: boardIconOf(p.board_id),
      likeCount: p.like_count ?? 0,
      replyCount: p.reply_count,
      viewCount: p.view_count,
      pinned: p.pinned ?? false,
      featured: p.is_featured ?? false,
      createdAt: p.created_at,
      lastReplyAt: p.last_reply_at ?? null,
      reasonBadge: p.reason ?? null,
      participants: p.participants ?? []
    }))
  );
</script>

<Seo
  title="发现"
  description={`为你推荐${site.siteName}社区里你可能感兴趣的内容`}
  og={{ type: 'website' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: `发现 · ${site.siteName}`
  }}
/>

<div class="container" id="page-discover">
  <h1 class="sr-only">发现</h1>
  <p class="sr-only">为你推荐社区里你可能感兴趣的内容</p>

  {#if error}
    <div class="discover-problem" role="alert">
      <span>推荐内容加载失败：{error}</span>
      <a href="/discover">重新加载</a>
    </div>
  {/if}

  <div class="proto-discover">
    <!-- 中栏：推荐信息流 -->
    <section class="feed" aria-label="推荐内容流">
      <div class="feed-toolbar">
        <div class="feed-title">
          <span class="feed-title__main">为你推荐</span>
          <span class="feed-title__hint">根据你的浏览与互动持续调整</span>
        </div>
        <a
          class="publish"
          href={authed ? '/editor' : `/login?next=${encodeURIComponent('/editor')}`}
        >
          {authed ? '发布' : '登录后发布'}
        </a>
      </div>

      <!-- 推荐列表（共享 TopicList：表头 + TopicRow 行，与首页列表同构） -->
      <TopicList
        rows={listRows}
        emptyTitle="还没有可推荐的内容"
        emptyDesc="社区的第一批讨论正等着你来发起"
        emptyCta={{
          href: authed ? '/editor' : `/login?next=${encodeURIComponent('/editor')}`,
          label: authed ? '发布第一篇内容' : '登录后发布内容'
        }}
      />

      <div class="feed-end">— 没有更多推荐了 —</div>
    </section>

    <!-- 右栏：推荐板块与社区服务（原型 aside.right-rail） -->
    <aside class="right-rail">
      {#if boards.length > 0}
        <section class="recommend" aria-label="推荐板块">
          <h2>推荐板块</h2>
          {#each boards.slice(0, 4) as board (board.id)}
            {@const visuals = boardVisuals(board.slug, board.icon)}
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
        <h2>{authed ? '社区服务' : '快捷入口'}</h2>
        <nav class="service-links" aria-label="快捷入口">
          <a href={authed ? '/editor' : `/login?next=${encodeURIComponent('/editor')}`}>
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

      <footer class="rail-foot">
        © {new Date().getFullYear()}<br />Powered By {site.siteName}
      </footer>
    </aside>
  </div>
</div>

<style>
  /* ===== 两栏骨架（推荐流 | 300px 右栏，gap 20）===== */
  .container {
    padding-inline: 10px;
  }

  .proto-discover {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(250px, 300px);
    gap: var(--space-5);
    align-items: start;
    padding: var(--space-5) 0 var(--space-6);
  }
  .proto-discover > * {
    min-width: 0;
  }

  /* ===== 中栏：推荐信息流 ===== */
  .feed-toolbar {
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 25px;
    background: var(--color-bg-card);
    border: none;
    border-radius: 2px;
  }
  .discover-problem {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin: var(--space-4) 0;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--aui-danger-border);
    background: var(--aui-danger-faint);
    color: var(--aui-danger);
    font: var(--text-sm)/1.45 var(--aui-font-mono);
  }
  .discover-problem a {
    color: inherit;
    text-decoration: underline;
    text-underline-offset: 3px;
    white-space: nowrap;
  }
  .feed-title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }
  .feed-title__main {
    font-size: 16px;
    font-weight: var(--weight-semibold);
    color: var(--color-text-primary);
    white-space: nowrap;
  }
  .feed-title__hint {
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    flex: 0 0 auto;
  }
  .publish:hover {
    background: var(--color-brand-hover);
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

  /* 响应式断点（≤999px：单栏，右栏隐藏；移动端精修由 mobile.css 承接） */
  @media (max-width: 999px) {
    .proto-discover {
      grid-template-columns: minmax(0, 1fr);
      padding: 0;
      gap: 0;
    }
    .right-rail {
      display: none;
    }
  }
  /* 表头降级与移动端断点由共享 TopicList 组件承接（与 TopicRow 一致） */
  @media (max-width: 767px) {
    .container {
      padding-inline: var(--space-3) !important;
    }
    .feed-toolbar {
      height: 48px;
      min-height: 48px;
      padding: 0 12px;
      border-radius: var(--aui-radius-sm, 4px);
    }
    .feed-title {
      gap: 8px;
    }
    .feed-title__main {
      font-size: 15px;
    }
    .feed-title__hint {
      display: none;
    }
    .publish {
      display: none !important;
    }
  }
</style>
