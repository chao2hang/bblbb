<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { getPost, listComments, createComment, getMe, type PostDetail, type Comment, type User, type Problem } from '$lib/api/client';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatTime, formatRelative, renderSafeMarkdown } from '$lib/utils';

  let post = $state<PostDetail | null>(null);
  let comments = $state<Comment[]>([]);
  let loading = $state(true);
  let error = $state('');
  let user = $state<User | null>(null);
  let newComment = $state('');
  let submitting = $state(false);
  let commentError = $state('');

  let id = $derived(page.params.id);

  onMount(async () => {
    user = await getMe(fetch);
    await loadData();
    loading = false;
  });

  async function loadData() {
    if (!id) return;
    try {
      post = await getPost(fetch, id);
      const result = await listComments(fetch, id);
      comments = result.items;
    } catch (err: unknown) {
      const problem = err as Problem;
      error = problem?.detail || problem?.title || '加载失败';
    }
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!user) {
      goto('/login');
      return;
    }
    if (!id || !newComment.trim()) return;
    submitting = true;
    commentError = '';
    try {
      const comment = await createComment(fetch, id, newComment.trim());
      comments = [...comments, comment];
      newComment = '';
      if (post) post.reply_count += 1;
    } catch (err: unknown) {
      const problem = err as Problem;
      commentError = problem?.detail || problem?.title || '回复失败';
    }
    submitting = false;
  }
</script>

<svelte:head>
  <title>{post ? post.title : '帖子'} — BBLBB</title>
  {#if post}<meta name="description" content={post.title} />{/if}
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">{post ? post.title : '帖子'}</span>
  </nav>

  {#if loading}
    <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
  {:else if error}
    <div class="empty-state">
      <div class="empty-state-title">{error}</div>
      <div class="empty-state-desc">帖子可能已被删除或你无权查看</div>
      <a href="/" class="text-link">返回首页</a>
    </div>
  {:else if post}
    <div class="card">
      <div class="card-body" style="padding:var(--space-6);">
        <div class="post-title-row" style="margin-bottom:var(--space-3);">
          <h1 style="font-size:var(--text-2xl);">{post.title}</h1>
        </div>
        <div style="display:flex;align-items:center;gap:var(--space-3);flex-wrap:wrap;padding-bottom:var(--space-4);border-bottom:var(--border-default);margin-bottom:var(--space-4);">
          <Avatar name={post.author_name ?? '匿名'} size="sm" />
          <span class="text-secondary" style="font-size:var(--text-sm);">
            {post.author_name || '匿名'} · {formatTime(post.created_at)} · {post.view_count} 浏览 · {post.reply_count} 回复
          </span>
          {#if post.visibility !== 'public'}
            <span class="badge badge-neutral">{post.visibility}</span>
          {/if}
        </div>
        <div class="prose">
          {@html renderSafeMarkdown(post.content)}
        </div>
      </div>
    </div>

    <section style="margin-top:var(--space-5);" aria-labelledby="comments-title">
      <h2 id="comments-title" style="font-size:var(--text-lg);margin-bottom:var(--space-3);">回复 ({comments.length})</h2>

      {#if comments.length > 0}
        <div class="comment-list" style="display:flex;flex-direction:column;gap:var(--space-3);">
          {#each comments as comment}
            <div class="card">
              <div class="card-body" style="padding:var(--space-4);">
                <div style="display:flex;align-items:center;gap:var(--space-2);margin-bottom:var(--space-2);">
                  <Avatar name={comment.author_name ?? '匿名'} size="xs" />
                  <span class="text-secondary" style="font-size:var(--text-sm);">
                    <strong style="color:var(--color-text-primary);">{comment.author_name || '匿名'}</strong>
                    <span style="margin:0 var(--space-1);">·</span>
                    {formatRelative(comment.created_at)}
                  </span>
                  <span class="badge badge-neutral" style="margin-left:auto;">#{comment.floor}</span>
                </div>
                <div class="prose" style="font-size:var(--text-base);">
                  {@html renderSafeMarkdown(comment.content)}
                </div>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="card"><div class="card-body"><EmptyState icon="message-square" title="暂无回复" desc="快来抢沙发！" /></div></div>
      {/if}

      {#if user}
        <form class="card" style="margin-top:var(--space-4);" onsubmit={handleSubmit}>
          <div class="card-body">
            <label class="input-label" for="comment-input">发表回复</label>
            <textarea
              id="comment-input"
              class="input-field editor-textarea"
              bind:value={newComment}
              placeholder="写下你的回复…"
              rows="4"
              maxlength="10000"
            ></textarea>
            {#if commentError}<p class="input-hint is-error" role="alert">{commentError}</p>{/if}
            <div style="display:flex;align-items:center;justify-content:space-between;margin-top:var(--space-3);">
              <span class="text-tertiary" style="font-size:var(--text-xs);">{newComment.length} / 10000</span>
              <Button text={submitting ? '发送中…' : '回复'} variant="primary" size="sm" type="submit" disabled={submitting || !newComment.trim()} />
            </div>
          </div>
        </form>
      {:else}
        <div class="card" style="margin-top:var(--space-4);">
          <div class="card-body" style="display:flex;align-items:center;gap:var(--space-2);">
            <Icon name="lock" size={16} />
            <span class="text-secondary">登录后即可回复。</span>
            <a href="/login" class="text-link" style="margin-left:auto;">登录</a>
          </div>
        </div>
      {/if}
    </section>
  {/if}
</div>
