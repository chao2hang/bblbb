<script lang="ts">
  import { onMount } from 'svelte';
  import { listBoards, type Board } from '$lib/api/client';
  import BoardCard from '$lib/components/BoardCard.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import LoadingState from '$lib/components/ui/LoadingState.svelte';
  import { boardVisuals } from '$lib/board-visuals';

  let boards = $state<Board[]>([]);
  let loading = $state(true);

  onMount(async () => {
    try {
      const result = await listBoards(fetch);
      boards = result.items;
    } catch {
      boards = [];
    }
    loading = false;
  });
</script>

<svelte:head>
  <title>板块总览 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">板块总览</span>
  </nav>

  <div class="card">
    <div class="card-header">
      <span class="card-title">全部板块</span>
      {#if !loading}<span class="text-secondary" style="font-size:var(--text-sm);">共 {boards.length} 个板块</span>{/if}
    </div>
    <div class="card-body">
      {#if loading}
        <LoadingState />
      {:else if boards.length === 0}
        <EmptyState icon="message-square" title="暂无板块" desc="社区还没有板块" />
      {:else}
        <div class="boards-grid">
          {#each boards as board}
            {@const visuals = boardVisuals(board.slug)}
            <BoardCard
              slug={board.slug}
              name={board.name}
              description={board.description ?? ''}
              post_count={board.post_count}
              icon={visuals.icon}
              color={visuals.color}
            />
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
