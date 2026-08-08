<script lang="ts">
  // M04-UI-01：帖子详情 SSR——核心内容（标题/作者/正文/访问占位）由
  // +page.server.ts 在服务端取回后端安全投影并渲染，无 JS 也可完整阅读；
  // 前端**不做**浏览器再裁剪（正文只渲染后端 body_html，经 SafeHtml 注入）。
  // M04-UI-06：回复/引用/编辑/删除/楼层定位/锁帖状态。
  // M04-UI-08：可恢复流程（version_conflict 重新加载 / 422 等级 / 429 限流）。
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import {
    listComments,
    createComment,
    updateComment,
    deleteComment,
    getMe,
    newClientRequestId,
    type Comment,
    type User
  } from '$lib/api/client';
  import {
    problemMessage,
    problemRecovery,
    postStatusNotice,
    type Problem
  } from '$lib/errors';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import SafeHtml from '$lib/components/SafeHtml.svelte';
  // M14-SEO-01/02/03：文章/讨论页统一 SEO；未发布/未解锁内容 noindex。
  import Seo from '$lib/components/Seo.svelte';
  import { formatTime, formatRelative, charCount } from '$lib/utils';
  import type { PostDetailPageData } from './+page.server';

  let { data }: { data: PostDetailPageData } = $props();

  const post = $derived(data.post);
  const authed = $derived(data.authed);
  const loadError = $derived(data.error);
  const locked = $derived(Boolean(post?.closed_at));
  const authorName = $derived(post?.author?.display_name || post?.author?.username || '匿名');
  const statusNotice = $derived(postStatusNotice(post?.status));

  /** 正文不可见时的可访问占位（M04-UI-07 的简化版；正文绝不放进 DOM）。 */
  const accessPlaceholder = $derived.by(() => {
    const s = post?.access_summary;
    if (!s) return '内容暂不可见';
    switch (s.policy) {
      case 'logged_in':
        return '内容仅对登录用户开放';
      case 'after_reply':
        return '回复后可解锁剩余内容';
      case 'level':
        return s.required_level ? `内容需达到 LV.${s.required_level} 后开放` : '内容需达到更高等级后开放';
      case 'paid':
        return '付费内容，解锁后可查看';
      default:
        return '内容暂不可见';
    }
  });

  // ── 客户端态（SSR 阶段为空；hydration 后 onMount 拉取，渐进增强） ──
  let user = $state<User | null>(null);
  let comments = $state<Comment[]>([]);
  let commentsLoaded = $state(false);
  /** 本次会话内新增/删除的回复数（对服务端 reply_count 的增量修正）。 */
  let replyDelta = $state(0);
  const replyCount = $derived((post?.reply_count ?? 0) + replyDelta);

  // 回复表单
  let newComment = $state('');
  let parentId = $state<string | null>(null);
  let quoteOf = $state<Comment | null>(null);
  let submitting = $state(false);
  let commentProblem = $state<Problem | null>(null);

  // 编辑/删除
  let editingId = $state<string | null>(null);
  let editText = $state('');
  let editProblem = $state<Problem | null>(null);
  let deletingId = $state<string | null>(null);

  onMount(async () => {
    user = await getMe(fetch);
    if (post) await loadComments();
  });

  async function loadComments() {
    if (!post) return;
    try {
      const result = await listComments(fetch, post.id);
      comments = result.items;
    } catch {
      comments = [];
    } finally {
      commentsLoaded = true;
    }
  }

  function authorLabel(c: Comment): string {
    return c.author?.display_name || c.author?.username || '匿名';
  }

  function quoteComment(c: Comment) {
    parentId = c.id;
    quoteOf = c;
    newComment = '';
    commentProblem = null;
  }

  function clearQuote() {
    parentId = null;
    quoteOf = null;
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!user) {
      goto('/login');
      return;
    }
    if (!post || !newComment.trim()) return;
    submitting = true;
    commentProblem = null;
    try {
      const comment = await createComment(fetch, post.id, {
        markdown: newComment.trim(),
        parent_id: parentId,
        client_request_id: newClientRequestId()
      });
      comments = [...comments, comment];
      replyDelta += 1;
      newComment = '';
      clearQuote();
    } catch (err: unknown) {
      commentProblem = err as Problem;
    }
    submitting = false;
  }

  function startEdit(c: Comment) {
    editingId = c.id;
    editText = c.markdown ?? '';
    editProblem = null;
  }

  function cancelEdit() {
    editingId = null;
    editText = '';
    editProblem = null;
  }

  async function saveEdit(c: Comment) {
    if (!editText.trim()) return;
    editProblem = null;
    try {
      const updated = await updateComment(fetch, c.id, { markdown: editText.trim() }, c.version);
      comments = comments.map((x) => (x.id === c.id ? updated : x));
      cancelEdit();
    } catch (err: unknown) {
      editProblem = err as Problem;
    }
  }

  async function handleDelete(c: Comment) {
    if (!window.confirm(`确定删除 #${c.floor} 楼回复吗？删除后不可恢复。`)) return;
    deletingId = c.id;
    editProblem = null;
    try {
      await deleteComment(fetch, c.id);
      comments = comments.filter((x) => x.id !== c.id);
      if (replyDelta > 0) replyDelta -= 1;
    } catch (err: unknown) {
      editProblem = err as Problem;
    }
    deletingId = null;
  }

  /** M04-UI-08：429 限流的冷却提示（Retry-After 秒数）。 */
  const commentRecovery = $derived(commentProblem ? problemRecovery(commentProblem) : null);
  const editRecovery = $derived(editProblem ? problemRecovery(editProblem) : null);

  /** M14-SEO-03：只有「已发布 + 公开可解锁」的内容可被索引；审核中/草稿/
   * 等级/登录/付费未解锁一律 noindex（后端投影决定可看内容，前端只按
   * 投影状态输出索引策略，隐藏正文永不进入 SSR）。 */
  const indexable = $derived(
    Boolean(
      post &&
        (post.status ?? 'published') === 'published' &&
        (post.access_summary?.policy ?? 'public') === 'public' &&
        post.access_summary?.unlocked !== false
    )
  );

  /** 访问策略展示文案（与契约 AccessSummary.policy 枚举一致）。 */
  function policyLabel(policy: string): string {
    switch (policy) {
      case 'logged_in':
        return '登录可见';
      case 'after_reply':
        return '回复解锁';
      case 'level':
        return '等级可见';
      case 'paid':
        return '付费可见';
      default:
        return policy;
    }
  }
</script>

<Seo
  title={post ? post.title : '帖子'}
  description={post ? post.title : '帖子内容'}
  noindex={!indexable}
  og={{ type: post?.post_type === 'article' ? 'article' : 'website', siteName: 'BBLBB' }}
  jsonLd={
    indexable
      ? {
          '@context': 'https://schema.org',
          '@type': post?.post_type === 'article' ? 'Article' : 'DiscussionForumPosting',
          headline: post!.title,
          datePublished: new Date(post!.created_at).toISOString(),
          author: {
            '@type': 'Person',
            name: post!.author?.display_name || post!.author?.username || '匿名'
          }
        }
      : null
  }
/>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">{post ? post.title : '帖子'}</span>
  </nav>

  {#if loadError && !post}
    <p class="input-hint is-error" role="alert">{loadError}</p>
  {:else if post}
    <div class="card">
      <div class="card-body" style="padding:var(--space-6);">
        <div class="post-title-row" style="margin-bottom:var(--space-3);">
          <h1 style="font-size:var(--text-2xl);">{post.title}</h1>
          {#if post.post_type === 'article'}
            <span class="badge badge-neutral" style="margin-left:var(--space-2);">文章</span>
          {/if}
        </div>
        {#if statusNotice}
          <p class="input-hint" role="status" style="margin-bottom:var(--space-3);">{statusNotice}</p>
        {/if}
        <div style="display:flex;align-items:center;gap:var(--space-3);flex-wrap:wrap;padding-bottom:var(--space-4);border-bottom:var(--border-default);margin-bottom:var(--space-4);">
          <Avatar name={authorName} size="sm" />
          {#if post.author?.username}
            <a href="/users/{post.author.username}" class="text-link" style="font-size:var(--text-sm);">{authorName}</a>
          {:else}
            <span class="text-secondary" style="font-size:var(--text-sm);">{authorName}</span>
          {/if}
          <span class="text-secondary" style="font-size:var(--text-sm);">
            {formatTime(post.created_at)} · {post.view_count ?? 0} 浏览 · {replyCount} 回复
          </span>
          {#if post.access_summary && post.access_summary.policy !== 'public'}
            <span class="badge badge-warning">可见性：{policyLabel(post.access_summary.policy)}</span>
          {/if}
          {#if locked}
            <span class="badge badge-danger">已锁定</span>
          {/if}
        </div>

        {#if post.body_html && post.access_summary?.unlocked !== false}
          <div class="prose">
            <!-- M04-MARKDOWN-08/UI-01：{@html} 仅经 SafeHtml（唯一 sink）；
                 正文为后端渲染清洗的 body_html，前端不做再裁剪。
                 页面级兜底：access_summary.unlocked === false 时即使数据混入
                 body_html 也绝不渲染（白名单在 +page.server.ts 内已做第一层）。 -->
            <SafeHtml html={post.body_html} />
          </div>
        {:else}
          <div class="card" role="note" aria-label="正文不可见" style="border-color:var(--color-warning);">
            <div class="card-body" style="display:flex;gap:var(--space-2);align-items:flex-start;">
              <Icon name="lock" size={16} />
              <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">{accessPlaceholder}</p>
            </div>
          </div>
        {/if}
      </div>
    </div>

    <section style="margin-top:var(--space-5);" aria-labelledby="comments-title">
      <h2 id="comments-title" style="font-size:var(--text-lg);margin-bottom:var(--space-3);">回复 ({commentsLoaded ? comments.length : ''})</h2>

      {#if comments.length > 0}
        <div class="comment-list" style="display:flex;flex-direction:column;gap:var(--space-3);">
          {#each comments as comment (comment.id)}
            <div class="card" id="floor-{comment.floor}">
              <div class="card-body" style="padding:var(--space-4);">
                <div style="display:flex;align-items:center;gap:var(--space-2);margin-bottom:var(--space-2);">
                  <Avatar name={authorLabel(comment)} size="xs" />
                  <span class="text-secondary" style="font-size:var(--text-sm);">
                    {#if comment.author?.username}
                      <a href="/users/{comment.author.username}" class="text-link"><strong style="color:var(--color-text-primary);">{authorLabel(comment)}</strong></a>
                    {:else}
                      <strong style="color:var(--color-text-primary);">{authorLabel(comment)}</strong>
                    {/if}
                    <span style="margin:0 var(--space-1);">·</span>
                    {formatRelative(comment.created_at)}
                  </span>
                  <span style="margin-left:auto;display:inline-flex;align-items:center;gap:var(--space-2);">
                    <span class="badge badge-neutral">#{comment.floor}</span>
                    <a href="#floor-{comment.floor}" class="text-link" style="font-size:var(--text-xs);">楼层</a>
                  </span>
                </div>

                {#if editingId === comment.id}
                  <div class="input-wrapper">
                    <label class="input-label" for="edit-comment-{comment.id}">编辑回复</label>
                    <textarea
                      id="edit-comment-{comment.id}"
                      class="input-field editor-textarea"
                      bind:value={editText}
                      rows="4"
                      maxlength="10000"
                    ></textarea>
                    {#if editRecovery && editRecovery.action !== 'none'}
                      <p class="input-hint is-error" role="alert">{editRecovery.message}</p>
                    {:else if editProblem}
                      <p class="input-hint is-error" role="alert">{problemMessage(editProblem)}</p>
                    {/if}
                    <div style="display:flex;align-items:center;gap:var(--space-2);margin-top:var(--space-2);">
                      <span class="text-tertiary" style="font-size:var(--text-xs);">{charCount(editText)} / 10000</span>
                      <div style="margin-left:auto;display:flex;gap:var(--space-2);">
                        <Button text="取消" variant="ghost" size="sm" onclick={cancelEdit} />
                        <Button text="保存" variant="primary" size="sm" onclick={() => saveEdit(comment)} disabled={!editText.trim()} />
                      </div>
                    </div>
                  </div>
                {:else}
                  <div class="prose" style="font-size:var(--text-base);">
                    {#if comment.body_html}
                      <SafeHtml html={comment.body_html} />
                    {:else}
                      <p class="text-tertiary" style="font-size:var(--text-sm);">内容不可见</p>
                    {/if}
                  </div>
                  <div style="display:flex;gap:var(--space-2);margin-top:var(--space-2);">
                    <Button text="引用" variant="ghost" size="sm" icon="quote" onclick={() => quoteComment(comment)} disabled={locked} />
                    {#if user && comment.author?.id && comment.author.id === user.id}
                      <Button text="编辑" variant="ghost" size="sm" icon="edit-3" onclick={() => startEdit(comment)} disabled={locked} />
                      <Button
                        text={deletingId === comment.id ? '删除中…' : '删除'}
                        variant="ghost"
                        size="sm"
                        onclick={() => handleDelete(comment)}
                        disabled={locked || deletingId === comment.id}
                      />
                    {/if}
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {:else if commentsLoaded}
        <div class="card"><div class="card-body"><EmptyState icon="message-square" title="暂无回复" desc="快来抢沙发！" /></div></div>
      {/if}

      {#if locked}
        <div class="card" style="margin-top:var(--space-4);border-color:var(--color-warning);">
          <div class="card-body" style="display:flex;align-items:center;gap:var(--space-2);">
            <Icon name="lock" size={16} />
            <span class="text-secondary">该帖已锁定，不能继续回复。</span>
          </div>
        </div>
      {:else if authed || user}
        <form class="card" style="margin-top:var(--space-4);" method="POST" onsubmit={handleSubmit}>
          <div class="card-body">
            <label class="input-label" for="comment-input">发表回复</label>
            {#if quoteOf}
              <div class="card" role="note" style="margin-bottom:var(--space-2);border-color:var(--color-border);">
                <div class="card-body" style="padding:var(--space-3);">
                  <div style="display:flex;align-items:center;gap:var(--space-2);margin-bottom:var(--space-1);">
                    <span class="badge badge-neutral">引用 #{quoteOf.floor}</span>
                    <strong class="text-secondary" style="font-size:var(--text-sm);">{authorLabel(quoteOf)}</strong>
                    <button type="button" class="text-link" style="margin-left:auto;background:none;border:none;cursor:pointer;" onclick={clearQuote}>取消引用</button>
                  </div>
                  {#if quoteOf.body_html}
                    <div class="prose" style="font-size:var(--text-sm);"><SafeHtml html={quoteOf.body_html} /></div>
                  {/if}
                </div>
              </div>
            {/if}
            <textarea
              id="comment-input"
              class="input-field editor-textarea"
              bind:value={newComment}
              placeholder="写下你的回复…"
              rows="4"
              maxlength="10000"
            ></textarea>
            {#if commentRecovery && commentRecovery.action !== 'none'}
              <p class="input-hint is-error" role="alert">{commentRecovery.message}</p>
            {:else if commentProblem}
              <p class="input-hint is-error" role="alert">{problemMessage(commentProblem)}</p>
            {/if}
            <div style="display:flex;align-items:center;justify-content:space-between;margin-top:var(--space-3);">
              <span class="text-tertiary" style="font-size:var(--text-xs);">{charCount(newComment)} / 10000</span>
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
