<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { listBoardPosts, type PostSummary } from '$lib/api/client';
  import PostList from '$lib/components/PostList.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';

  let slug = $derived(page.params.slug);
  let posts = $state<PostSummary[]>([]);
  let loading = $state(true);

  onMount(async () => {
    if (!slug) {
      loading = false;
      return;
    }
    try {
      const result = await listBoardPosts(fetch, slug);
      posts = result.items;
    } catch {
      posts = [];
    }
    loading = false;
  });
</script>

<svelte:head>
  <title>{slug} — BBLBB</title>
</svelte:head>

<div class="container">
  <div class="page-content content-grid">
    <div class="main-col">
      <nav class="breadcrumb" aria-label="面包屑">
        <a href="/" class="breadcrumb-link">首页</a>
        <span class="breadcrumb-sep">/</span>
        <a href="/boards" class="breadcrumb-link">板块</a>
        <span class="breadcrumb-sep">/</span>
        <span class="breadcrumb-current">{slug}</span>
      </nav>

      <div class="board-header">
        <div class="board-icon">
          <Icon name="message-square" size={28} />
        </div>
        <div class="board-info">
          <h1 class="board-name">{slug}</h1>
          <p class="board-desc">共 {posts.length} 个帖子</p>
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
