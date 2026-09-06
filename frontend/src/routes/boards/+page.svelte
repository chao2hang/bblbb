<script lang="ts">
  // M03-UI-06：板块总览 SSR——板块树（父级 + 子板块分组）、空状态、
  // 权限提示（members/restricted/hidden 可见性徽标）。
  // 原型对齐：prototype/pages/boards.html
  // - 统一 .app-route-head（COMMUNITY / BOARDS）
  // - .app-toolbar 工具条（发布讨论）
  // - .app-board-grid 3 列高密度卡片网格
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import Seo from '$lib/components/Seo.svelte';
  import type { Board } from '$lib/api/types';
  import type { BoardsPageData } from './+page.server';

  let { data }: { data: BoardsPageData } = $props();

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
  title="板块 · BBLBB 社区"
  description="按兴趣进入社区的不同讨论空间"
  og={{ type: 'website', siteName: 'BBLBB' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: 'BBLBB 板块'
  }}
/>

<div class="container app-page">
  <section class="page app-page app-route-boards" id="page-boards">
    <div class="app-route-head">
      <div class="app-route-head__copy">
        <span class="app-kicker">COMMUNITY / BOARDS</span>
        <h1 tabindex="-1">板块</h1>
        <p>按兴趣进入社区的不同讨论空间</p>
      </div>
    </div>

    <div class="app-toolbar" style="margin-bottom:14px">
      <a href="/editor" class="btn primary">发布讨论</a>
    </div>

    {#if error && boards.length === 0}
      <p class="input-hint is-error" role="alert">{error}</p>
    {/if}

    {#if boards.length === 0 && !error}
      <EmptyState icon="message-square" title="暂无板块" desc="社区还没有板块" />
    {:else if hasSubBoards}
      <!-- 存在层级子板块时：按父板块分组展示 -->
      <div style="display:flex;flex-direction:column;gap:24px;">
        {#each roots as root (root.id)}
          {@const kids = childrenOf.get(root.id) ?? []}
          {@const rootVisuals = boardVisuals(root.slug)}
          {@const hint = visibilityHint(root)}
          {@const kCount = childCount.get(root.id) ?? 0}
          <section aria-label={root.name}>
            <div style="display:flex;align-items:center;gap:8px;margin-bottom:10px;">
              <h2 style="margin:0;font-size:18px;font-weight:600;font-family:var(--font-family-serif);">
                <a href="/boards/{root.slug}" style="text-decoration:none;color:inherit;">{root.name}</a>
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

            <div class="app-board-grid">
              <!-- 根板块自身卡片 -->
              <a class="app-board-card" href="/boards/{root.slug}">
                <span class="app-board-card__icon">
                  <Icon name={rootVisuals.icon || 'workflow'} size={18} />
                </span>
                <h3>{root.name}</h3>
                <p>{root.description || '深入记录工程实践与技术取舍。'}</p>
                <div class="app-board-card__meta">
                  <span>{root.post_count ?? 0} 个主题</span>
                </div>
              </a>

              <!-- 子板块卡片 -->
              {#each kids as child (child.id)}
                {@const childVisuals = boardVisuals(child.slug)}
                {@const childHint = visibilityHint(child)}
                <a class="app-board-card" href="/boards/{child.slug}">
                  <span class="app-board-card__icon">
                    <Icon name={childVisuals.icon || 'workflow'} size={18} />
                  </span>
                  <h3>{child.name}</h3>
                  <p>{child.description || '暂无描述'}</p>
                  <div class="app-board-card__meta">
                    <span>{child.post_count ?? 0} 个主题</span>
                    {#if childHint}
                      <span>· {childHint.label}</span>
                    {/if}
                  </div>
                </a>
              {/each}
            </div>
          </section>
        {/each}
      </div>
    {:else}
      <!-- 平铺模式（全部为一级板块）：1:1 对齐原型 3 列网格 -->
      <div class="app-board-grid">
        {#each boards as board (board.id)}
          {@const visuals = boardVisuals(board.slug)}
          {@const hint = visibilityHint(board)}
          <a class="app-board-card" href="/boards/{board.slug}">
            <span class="app-board-card__icon">
              <Icon name={visuals.icon || 'workflow'} size={18} />
            </span>
            <h3>{board.name}</h3>
            <p>{board.description || '按兴趣进入社区的不同讨论空间。'}</p>
            <div class="app-board-card__meta">
              <span>{board.post_count ?? 0} 个主题</span>
              {#if hint}
                <span>· {hint.label}</span>
              {/if}
            </div>
          </a>
        {/each}
      </div>
    {/if}
  </section>
</div>
