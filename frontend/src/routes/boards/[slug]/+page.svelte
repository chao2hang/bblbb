<script lang="ts">
  // M03-UI-06：板块详情 SSR——板块信息 + 帖子列表 + 权限提示 + 空状态。
  import PostList from '$lib/components/PostList.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import { formatCount } from '$lib/utils';
  // M14-SEO-01/02/03：板块页统一 SEO；非公开板块 noindex。
  import Seo from '$lib/components/Seo.svelte';
  import type { BoardDetailData } from './+page.server';

  let { data }: { data: BoardDetailData } = $props();

  const board = $derived(data.board);
  const posts = $derived(data.posts);
  const error = $derived(data.error);
  const slug = $derived(board?.slug ?? '');

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
  /** M14-SEO-02/03：只有公开且激活的板块可索引；members/restricted/
   * hidden 与非激活板块 noindex（与后端公开投影一致）。 */
  const indexable = $derived(
    Boolean(
      board &&
        (board.visibility ?? 'public') === 'public' &&
        board.is_active !== 0
    )
  );
</script>

<Seo
  title={board?.name ?? slug}
  description={board?.description ?? '板块：' + (board?.name ?? slug)}
  noindex={!indexable}
  og={{ type: 'website', siteName: 'BBLBB' }}
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

<div class="container">
  <div class="page-content content-grid">
    <div class="main-col">
      <nav class="breadcrumb" aria-label="面包屑">
        <a href="/" class="breadcrumb-link">首页</a>
        <span class="breadcrumb-sep">/</span>
        <a href="/boards" class="breadcrumb-link">板块</a>
        <span class="breadcrumb-sep">/</span>
        <span class="breadcrumb-current">{board?.name ?? slug}</span>
      </nav>

      {#if error && !board}
        <p class="input-hint is-error" role="alert">{error}</p>
      {/if}

      {#if board}
        {@const visuals = boardVisuals(board.slug)}
        <div class="board-header">
          <div class="board-icon" style="--cat-color:{visuals.color};">
            <Icon name={visuals.icon} size={28} />
          </div>
          <div class="board-info">
            <h1 class="board-name">{board.name}</h1>
            {#if board.description}<p class="board-desc">{board.description}</p>{/if}
            <div class="board-stats">
              <span><strong>{formatCount(board.post_count)}</strong> 帖子</span>
            </div>
          </div>
          <div>
            <Button text="发布新帖" variant="primary" icon="pen-line" href="/editor" />
          </div>
        </div>

        {#if permissionHint}
          <div class="card" role="note" style="margin-top:var(--space-4);border-color:var(--color-warning);">
            <div class="card-body" style="display:flex;gap:var(--space-2);align-items:flex-start;">
              <Icon name="lock" size={16} />
              <div>
                {#each permissionHint as hint}<p style="margin:0;">{hint}</p>{/each}
              </div>
            </div>
          </div>
        {/if}

        <div class="card" style="margin-top:var(--space-4);">
          <div class="card-body" style="padding:0;">
            <PostList posts={posts} emptyTitle="暂无帖子" emptyDesc="成为第一个发帖的人吧！" />
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
