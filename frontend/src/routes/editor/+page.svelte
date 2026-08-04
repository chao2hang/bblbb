<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { listBoards, createPost, type Board } from '$lib/api/client';
  import { problemText, type Problem } from '$lib/errors';
  import Button from '$lib/components/ui/Button.svelte';
  import Card from '$lib/components/ui/Card.svelte';

  let boards = $state<Board[]>([]);
  let title = $state('');
  let content = $state('');
  let boardSlug = $state('');
  let visibility = $state('public');
  let error = $state('');
  let submitting = $state(false);

  onMount(async () => {
    try {
      const result = await listBoards(fetch);
      boards = result.items;
      if (boards.length > 0) boardSlug = boards[0].slug;
    } catch {
      boards = [];
    }
  });

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!title.trim() || !content.trim() || !boardSlug) return;
    submitting = true;
    error = '';
    try {
      const result = await createPost(fetch, boardSlug, title.trim(), content, visibility);
      goto(`/posts/${result.id}`);
    } catch (err: unknown) {
      error = problemText(err as Problem);
    }
    submitting = false;
  }
</script>

<svelte:head>
  <title>发布 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">发布新帖</span>
  </nav>

  <form class="publish-layout" onsubmit={handleSubmit}>
    <div class="publish-main">
      <div class="publish-title-field">
        <label for="publish-title">讨论标题</label>
        <div class="publish-title-control">
          <input
            type="text"
            class="input-field publish-title-input"
            placeholder="一句话说清你想讨论什么…"
            bind:value={title}
            id="publish-title"
            maxlength="80"
            autocomplete="off"
          />
          <span class="publish-title-hint">{title.length} / 80</span>
        </div>
      </div>
      <div class="card">
        <div class="card-body" style="padding:0;">
          <textarea
            class="editor-textarea"
            id="publish-content"
            placeholder="使用 Markdown 编写内容…"
            bind:value={content}
            rows="16"
          ></textarea>
        </div>
      </div>
      {#if error}<p class="input-hint is-error" role="alert" style="margin-top:var(--space-2);">{error}</p>{/if}
    </div>

    <div class="publish-sidebar">
      <div class="card">
        <div class="card-header"><span class="card-title">发布设置</span></div>
        <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
          <div class="input-wrapper">
            <label class="input-label" for="publish-board">板块</label>
            <select class="input-field" id="publish-board" bind:value={boardSlug}>
              {#each boards as board}
                <option value={board.slug}>{board.name}</option>
              {/each}
            </select>
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="publish-visibility">可见性</label>
            <select class="input-field" id="publish-visibility" bind:value={visibility}>
              <option value="public">公开</option>
              <option value="logged_in">登录可见</option>
            </select>
          </div>
        </div>
      </div>
      <Button text={submitting ? '发布中…' : '立即发布'} variant="primary" size="lg" type="submit" extraClass="btn-block" disabled={submitting} />
    </div>
  </form>
</div>
