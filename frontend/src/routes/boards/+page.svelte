<script lang="ts">
  // M03-UI-06：板块总览 SSR——板块树（父级 + 子板块分组）、空状态、
  // 权限提示（members/restricted/hidden 可见性徽标）。
  import BoardCard from '$lib/components/BoardCard.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  // M14-SEO-01：板块总览统一 SEO。
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
</script>

<Seo
  title="板块总览"
  description="BBLBB 全部公开板块"
  og={{ type: 'website', siteName: 'BBLBB' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: 'BBLBB 板块总览'
  }}
/>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">板块总览</span>
  </nav>

  {#if error && boards.length === 0}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  {#if boards.length === 0 && !error}
    <EmptyState icon="message-square" title="暂无板块" desc="社区还没有板块" />
  {:else}
    <div class="card">
      <div class="card-header">
        <span class="card-title">全部板块</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">共 {boards.length} 个板块</span>
      </div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-5);">
        {#each roots as board}
          {@const rootVisuals = boardVisuals(board.slug)}
          {@const hint = visibilityHint(board)}
          <section aria-label={board.name}>
            <div class="board-tree-root" style="display:flex;align-items:center;gap:var(--space-2);margin-bottom:var(--space-3);">
              <h2 class="board-tree-title" style="margin:0;font-size:var(--text-lg);">
                <a href="/boards/{board.slug}" style="text-decoration:none;color:inherit;">{board.name}</a>
              </h2>
              {#if childCount.get(board.id)}
                <span class="badge badge-neutral">{childCount.get(board.id)} 个子板块</span>
              {/if}
              {#if hint}
                <span class="badge {hint.tone}" title={hint.label}><Icon name={hint.icon} size={12} /> {hint.label}</span>
              {/if}
            </div>
            <div class="boards-grid">
              <BoardCard
                slug={board.slug}
                name={board.name}
                description={board.description ?? ''}
                post_count={board.post_count}
                icon={rootVisuals.icon}
                color={rootVisuals.color}
              />
              {#each childrenOf.get(board.id) ?? [] as child}
                {@const childVisuals = boardVisuals(child.slug)}
                <BoardCard
                  slug={child.slug}
                  name={child.name}
                  description={child.description ?? ''}
                  post_count={child.post_count}
                  icon={childVisuals.icon}
                  color={childVisuals.color}
                />
              {/each}
            </div>
          </section>
        {/each}
      </div>
    </div>
  {/if}
</div>
