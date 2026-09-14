<script lang="ts">
  // M03-UI-06：板块总览 SSR——板块树（父级 + 子板块分组）、空状态、
  // 权限提示（members/restricted/hidden 可见性徽标）。
  // 原型对齐：prototype/pages/boards.html
  // - 页面首屏是「索引」而不是装饰性 hero：可见标题 + mono 汇总行 + 发布动作，
  //   底部发丝线收束首屏，主区交给 L1 面板 + L2 发丝线行。
  // - 签名元素：板块色识别（board-visuals 的 slug→色板）。行内图标片常驻
  //   板块色的淡染，hover/聚焦时行底泛起同色、图标片填充为实色——与帖子
  //   列表 category-badge 的 --cat-color 是同一套身份色。
  // - 计数诚实性：契约对匿名投影隐藏 post_count，缺失时不渲染「0 个主题」。
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import Seo from '$lib/components/Seo.svelte';
  import { formatCount } from '$lib/utils';
  import type { Board } from '$lib/api/types';
  import type { BoardsPageData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';
  import { page } from '$app/state';

  // data.site：根 layout 注入的全站文案（0065）；隔离渲染时兜底解析。
  let { data }: { data: BoardsPageData & { site?: SiteCopyView | null } } = $props();

  const user = $derived.by(() => {
    try {
      return page.data?.user ?? null;
    } catch {
      return null;
    }
  });
  const authed = $derived(Boolean(user));

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));

  const boards = $derived(data.boards);
  const error = $derived(data.error);

  /** 可见性文案（权限提示，M03-BOARDS-03：hidden 对匿名与成员均 404，
   *  列表投影中不出现，因此这里只有 public/members/restricted）。 */
  function visibilityHint(board: Board): { label: string; icon: string; tone: string } | null {
    const visibility = board.visibility;
    if (!visibility || visibility === 'public') return null;
    const map: Record<string, { label: string; icon: string; tone: string }> = {
      members: { label: '仅登录成员可见', icon: 'lock', tone: 'badge-warning' },
      restricted: { label: '需加入板块可见', icon: 'lock', tone: 'badge-warning' }
    };
    return map[visibility] ?? null;
  }

  /** 索引汇总行：板块数恒可数；主题总数仅在有任一计数投影时汇总
   *  （匿名投影缺 post_count，不得假报 0）。 */
  const totalTopics = $derived.by(() => {
    if (!boards.some((b) => typeof b.post_count === 'number')) return null;
    return boards.reduce((acc, b) => acc + (b.post_count ?? 0), 0);
  });

  // 板块树：parent_id 为空的为根，其余按父板块分组（保持服务端稳定排序）。
  const roots = $derived(boards.filter((b) => !b.parent_id));
  const childrenOf = $derived(
    new Map<string, Board[]>(
      [...new Set(boards.map((b) => b.parent_id).filter(Boolean) as string[])].map((pid) => [
        pid,
        boards.filter((b) => b.parent_id === pid)
      ])
    )
  );
  const childCount = $derived(
    [...childrenOf.entries()].reduce((acc, [pid, kids]) => {
      acc.set(pid, kids.length);
      return acc;
    }, new Map<string, number>())
  );
  const hasSubBoards = $derived([...childCount.values()].some((c) => c > 0));
</script>

<Seo
  title="板块"
  description="按兴趣进入社区的不同讨论空间"
  og={{ type: 'website' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: `板块 · ${site.siteName}`
  }}
/>

<div class="container app-page">
  <section class="page app-route-boards" id="page-boards">
    <header class="boards-head">
      <div class="boards-head__copy">
        <h1>板块</h1>
        {#if boards.length > 0}
          <p class="boards-head__summary">
            {boards.length} 个板块{#if totalTopics !== null}&ensp;·&ensp;{formatCount(totalTopics)} 个主题{/if}
          </p>
        {/if}
      </div>
      <a
        href={authed ? '/editor' : `/login?next=${encodeURIComponent('/editor')}`}
        class="btn primary"
      >
        {authed ? '发布讨论' : '登录后发布讨论'}
      </a>
    </header>

    {#if error && boards.length === 0}
      <div class="board-error app-notice is-danger" role="alert">
        <span>{error}</span>
        <a href="/boards">重新加载</a>
      </div>
    {/if}

    {#if boards.length === 0 && !error}
      <EmptyState icon="message-square" title="暂无板块" desc="社区还没有板块" />
    {:else if hasSubBoards}
      <!-- 存在层级子板块时：按父板块分组展示 -->
      <div class="board-groups">
        {#each roots as root (root.id)}
          {@const kids = childrenOf.get(root.id) ?? []}
          {@const rootVisuals = boardVisuals(root.slug, root.icon)}
          {@const hint = visibilityHint(root)}
          {@const kCount = childCount.get(root.id) ?? 0}
          <section aria-label={root.name}>
            <div class="board-group__head">
              <span class="board-group__chip" style="--board-color:{rootVisuals.color}">
                <Icon name={rootVisuals.icon || 'workflow'} size={14} />
              </span>
              <h2>
                <a href="/boards/{root.slug}">{root.name}</a>
              </h2>
              {#if kCount > 0}
                <span class="badge badge-neutral">{kCount} 个子板块</span>
              {/if}
              {#if hint}
                <span class="badge {hint.tone}" title={hint.label}>
                  <Icon name={hint.icon} size={12} /> {hint.label}
                </span>
              {/if}
            </div>

            <div class="board-index">
              <!-- 根板块自身条目 -->
              <a
                class="board-row"
                style="--board-color:{rootVisuals.color}"
                href="/boards/{root.slug}"
              >
                <span class="board-row__icon" aria-hidden="true">
                  <Icon name={rootVisuals.icon || 'workflow'} size={18} />
                </span>
                <div class="board-row__body">
                  <div class="board-row__title-row">
                    <h3>{root.name}</h3>
                  </div>
                  <p>{root.description || '深入记录工程实践与技术取舍。'}</p>
                </div>
                <div class="board-row__meta">
                  {#if typeof root.post_count === 'number'}
                    <span class="board-row__count">
                      <b>{formatCount(root.post_count)}</b> 个主题
                    </span>
                  {/if}
                  <span class="board-row__go" aria-hidden="true">
                    <Icon name="chevron-right" size={16} />
                  </span>
                </div>
              </a>

              <!-- 子板块条目 -->
              {#each kids as child (child.id)}
                {@const childVisuals = boardVisuals(child.slug, child.icon)}
                {@const childHint = visibilityHint(child)}
                <a
                  class="board-row"
                  style="--board-color:{childVisuals.color}"
                  href="/boards/{child.slug}"
                >
                  <span class="board-row__icon" aria-hidden="true">
                    <Icon name={childVisuals.icon || 'workflow'} size={18} />
                  </span>
                  <div class="board-row__body">
                    <div class="board-row__title-row">
                      <h3>{child.name}</h3>
                      {#if childHint}
                        <span class="badge {childHint.tone}" title={childHint.label}>
                          <Icon name={childHint.icon} size={12} /> {childHint.label}
                        </span>
                      {/if}
                    </div>
                    <p>{child.description || '暂无简介'}</p>
                  </div>
                  <div class="board-row__meta">
                    {#if typeof child.post_count === 'number'}
                      <span class="board-row__count">
                        <b>{formatCount(child.post_count)}</b> 个主题
                      </span>
                    {/if}
                    <span class="board-row__go" aria-hidden="true">
                      <Icon name="chevron-right" size={16} />
                    </span>
                  </div>
                </a>
              {/each}
            </div>
          </section>
        {/each}
      </div>
    {:else}
      <!-- 平铺模式（全部为一级板块）：统一索引行，便于快速扫描比较。 -->
      <div class="board-index">
        {#each boards as board (board.id)}
          {@const visuals = boardVisuals(board.slug, board.icon)}
          {@const hint = visibilityHint(board)}
          <a
            class="board-row"
            style="--board-color:{visuals.color}"
            href="/boards/{board.slug}"
          >
            <span class="board-row__icon" aria-hidden="true">
              <Icon name={visuals.icon || 'workflow'} size={18} />
            </span>
            <div class="board-row__body">
              <div class="board-row__title-row">
                <h3>{board.name}</h3>
                {#if hint}
                  <span class="badge {hint.tone}" title={hint.label}>
                    <Icon name={hint.icon} size={12} /> {hint.label}
                  </span>
                {/if}
              </div>
              <p>{board.description || '按兴趣进入社区的不同讨论空间。'}</p>
            </div>
            <div class="board-row__meta">
              {#if typeof board.post_count === 'number'}
                <span class="board-row__count">
                  <b>{formatCount(board.post_count)}</b> 个主题
                </span>
              {/if}
              <span class="board-row__go" aria-hidden="true">
                <Icon name="chevron-right" size={16} />
              </span>
            </div>
          </a>
        {/each}
      </div>
    {/if}
  </section>
</div>

<style>
  /* ---- 首屏索引头：标题 + 汇总 + 动作，发丝线收束 ---- */
  .boards-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-3) var(--space-4);
    padding-bottom: var(--space-4);
    margin-bottom: var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }

  .boards-head h1 {
    margin: 0;
    color: var(--color-text-primary);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    line-height: var(--text-xl-leading);
  }

  .boards-head__summary {
    margin: 2px 0 0;
    color: var(--color-text-tertiary);
    font: var(--weight-medium) 12px/18px var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--meta-letter-spacing);
  }

  .board-error {
    margin-bottom: var(--space-5);
  }

  /* ---- L1 面板：板块索引 ---- */
  .board-groups {
    display: grid;
    gap: var(--space-6);
  }

  .board-group__head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    margin-bottom: var(--space-3);
  }

  .board-group__chip {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 1px solid color-mix(in srgb, var(--board-color) 32%, var(--color-border));
    background: color-mix(in srgb, var(--board-color) 10%, transparent);
    color: var(--board-color);
  }

  .board-group__head h2 {
    margin: 0;
    font-size: var(--text-lg);
  }

  .board-group__head h2 a {
    color: var(--color-text-primary);
    text-decoration: none;
  }

  .board-group__head h2 a:hover {
    color: var(--color-brand);
  }

  .board-index {
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
  }

  /* ---- L2 行：板块身份色 + 发丝线分隔 ---- */
  .board-row {
    position: relative;
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-4);
    padding: 15px 18px;
    color: inherit;
    text-decoration: none;
    transition: background var(--duration-fast);
  }

  .board-row + .board-row {
    border-top: 1px solid var(--color-border-muted);
  }

  /* 悬停/聚焦：行底泛起板块色，左缘以 2px 实色划线（板块色的「信号」时刻） */
  .board-row::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 2px;
    background: var(--board-color);
    opacity: 0;
    transition: opacity var(--duration-fast);
  }

  .board-row:hover,
  .board-row:focus-visible {
    background: color-mix(in srgb, var(--board-color) 6%, transparent);
    text-decoration: none;
  }

  .board-row:hover::before,
  .board-row:focus-visible::before {
    opacity: 1;
  }

  .board-row__icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 1px solid color-mix(in srgb, var(--board-color) 32%, var(--color-border));
    background: color-mix(in srgb, var(--board-color) 10%, transparent);
    color: var(--board-color);
    transition:
      background var(--duration-fast),
      color var(--duration-fast);
  }

  .board-row:hover .board-row__icon,
  .board-row:focus-visible .board-row__icon {
    background: var(--board-color);
    color: var(--color-text-on-solid);
  }

  .board-row__body {
    min-width: 0;
  }

  .board-row__title-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    min-width: 0;
  }

  .board-row h3 {
    margin: 0;
    overflow: hidden;
    color: var(--color-text-primary);
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    line-height: var(--text-md-leading);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .board-row p {
    margin: 2px 0 0;
    overflow: hidden;
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    line-height: var(--text-sm-leading);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .board-row__meta {
    display: inline-flex;
    align-items: center;
    gap: var(--space-3);
    justify-self: end;
  }

  .board-row__count {
    min-width: 76px;
    color: var(--color-text-tertiary);
    font: var(--weight-normal) 12px/18px var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    text-align: right;
    white-space: nowrap;
  }

  .board-row__count b {
    color: var(--color-text-secondary);
    font-weight: var(--weight-semibold);
  }

  /* 方向提示：常驻隐藏，悬停/聚焦时浮现（触屏端直接隐藏） */
  .board-row__go {
    display: inline-grid;
    place-items: center;
    color: var(--color-text-tertiary);
    opacity: 0;
    transform: translateX(-3px);
    transition:
      opacity var(--duration-fast),
      transform var(--duration-fast);
  }

  .board-row:hover .board-row__go,
  .board-row:focus-visible .board-row__go {
    opacity: 1;
    transform: translateX(0);
  }

  @media (max-width: 640px) {
    .board-row__go {
      display: none;
    }
  }

  @media (max-width: 560px) {
    .board-row {
      grid-template-columns: 36px minmax(0, 1fr) auto;
      gap: var(--space-3);
      padding: 13px 14px;
    }

    .board-row__icon {
      width: 34px;
      height: 34px;
    }

    .board-row p {
      white-space: normal;
    }

    .board-row__count {
      min-width: 0;
    }
  }
</style>
