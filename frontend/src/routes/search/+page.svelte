<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { search, type PostSummary } from '$lib/api/client';
  import PostList from '$lib/components/PostList.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';

  let q = $state(page.url.searchParams.get('q') || '');
  let results = $state<PostSummary[]>([]);
  let loading = $state(false);
  let searched = $state(false);

  async function run(query: string) {
    loading = true;
    searched = true;
    try {
      const result = await search(fetch, query, 20);
      results = result.items;
    } catch {
      results = [];
    }
    loading = false;
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    const form = e.currentTarget as HTMLFormElement;
    const input = form.querySelector('input') as HTMLInputElement | null;
    const query = (input?.value || '').trim();
    goto(query ? `/search?q=${encodeURIComponent(query)}` : '/search');
    run(query);
  }

  onMount(() => {
    const initial = page.url.searchParams.get('q') || '';
    if (initial) run(initial);
  });
</script>

<svelte:head>
  <title>搜索 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">搜索</span>
  </nav>

  <form class="card" role="search" onsubmit={submit}>
    <div class="card-body" style="display:flex;gap:var(--space-3);align-items:center;">
      <div style="position:relative;flex:1;min-width:0;">
        <span style="position:absolute;left:var(--space-3);top:50%;transform:translateY(-50%);color:var(--color-text-tertiary);display:inline-flex;">
          <Icon name="search" size={16} />
        </span>
        <input
          type="search"
          class="input-field"
          placeholder="搜索帖子…"
          style="padding-left:40px;"
          bind:value={q}
          aria-label="搜索帖子"
        />
      </div>
      <button type="submit" class="btn btn-primary btn-md"><span>搜索</span></button>
    </div>
  </form>

  <div style="margin-top:var(--space-4);">
    {#if loading}
      <div class="empty-state"><div class="empty-state-title">搜索中…</div></div>
    {:else if searched && results.length === 0}
      <EmptyState icon="search" title="没有找到相关内容" desc="换个关键词试试" />
    {:else if results.length > 0}
      <div class="card">
        <div class="card-body" style="padding:0;">
          <PostList posts={results} />
        </div>
      </div>
    {/if}
  </div>
</div>
