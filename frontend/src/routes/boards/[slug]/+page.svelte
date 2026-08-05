<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { listBoardPosts, getBoard, type PostSummary } from '$lib/api/client';
  import PostList from '$lib/components/PostList.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import { formatCount } from '$lib/utils';

  let slug = $derived(page.params.slug);
  let visuals = $derived(boardVisuals(slug ?? ''));
  let posts = $state<PostSummary[]>([]);
  let boardName = $state<string | null>(null);
  let boardDesc = $state<string | null>(null);
  let postCount = $state<number | null>(null);
  let loading = $state(true);

  onMount(async () => {
    if (!slug) {
      loading = false;
      return;
    }
    try {
      const [boardResult, postsResult] = await Promise.allSettled([
        getBoard(fetch, slug),
        listBoardPosts(fetch, slug),
      ]);
      if (boardResult.status === 'fulfilled') {
        boardName = boardResult.value.name;
        boardDesc = boardResult.value.description;
        postCount = boardResult.value.post_count;
      }
      if (postsResult.status === 'fulfilled') {
        posts = postsResult.value.items;
      }
    } catch {
      posts = [];
    }
    loading = false;
  });
</script>

<svelte:head>
  <title>{boardName ?? slug} — BBLBB</title>
</svelte:head>

<div class="container">
  <div class="page-content content-grid">
    <div class="main-col">
      <nav class="breadcrumb" aria-label="面包屑">
        <a href="/" class="breadcrumb-link">首页</a>
        <span class="breadcrumb-sep">/</span>
        <a href="/boards" class="breadcrumb-link">板块</a>
        <span class="breadcrumb-sep">/</span>
        <span class="breadcrumb-current">{boardName ?? slug}</span>
      </nav>

      <div class="board-header">
        <div class="board-icon" style="--cat-color:{visuals.color};">
          <Icon name={visuals.icon} size={28} />
        </div>
        <div class="board-info">
          <h1 class="board-name">{boardName ?? slug}</h1>
          {#if boardDesc}<p class="board-desc">{boardDesc}</p>{/if}
          <div class="board-stats">
            <span><strong>{formatCount(postCount ?? posts.length)}</strong> 帖子</span>
          </div>
        </div>
        <div>
          <Button text="发布新帖" variant="primary" icon="pen-line" href="/editor" />
        </div>
      </div>

      <div class="card">
        <div class="card-body" style="padding:0;">
          {#if loading}
            <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
          {:else}
            <PostList posts={posts} emptyTitle="暂无帖子" emptyDesc="成为第一个发帖的人吧！" />
          {/if}
        </div>
      </div>
    </div>
  </div>
</div>