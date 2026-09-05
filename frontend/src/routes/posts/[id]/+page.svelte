<script lang="ts">
  // M04-UI-01：帖子详情 SSR——核心内容（标题/作者/正文/访问占位）由
  // +page.server.ts 在服务端取回后端安全投影并渲染，无 JS 也可完整阅读；
  // 前端**不做**浏览器再裁剪（正文只渲染后端 body_html，经 SafeHtml 注入）。
  // M04-UI-06：回复/引用/编辑/删除/楼层定位/锁帖状态。
  // M04-UI-08：可恢复流程（version_conflict 重新加载 / 422 等级 / 429 限流）。
  //
  // GAP-FIX（内容消费增强，本批新增）：
  // - 主题功能按钮（收藏/赞/分享/举报）对齐原型：位于侧栏「关于作者」卡
  //   （.topic-actions--side 2×2 网格），正文卡下方不设独立操作区；
  //   帖子点赞 + 每条评论轻量点赞（reactions.rs，仅 like 一种）；
  // - 收藏（favorite/unfavorite）、分享（复制链接）、举报入口（链接带
  //   /moderation/report?post= 预填）、付费解锁（unlockPost 幂等）；
  // - 评论排序（最新/最早/只看作者——后端 ListQuery 无 sort，纯前端）；
  // - 代码块复制按钮（$effect 挂载，reduced-motion 无动画）；
  // - 侧栏「关于作者」/「所在板块」卡。
  // 交互均为客户端渐进增强（与既有评论编辑/删除同模式）：锁帖 SSR 输出
  // 保持零 POST 表单（M04-UI-09 回归测试约束），收藏/解锁需 JS 属可接受
  // 退化（分享/举报无 JS 仍可用：链接文本 + 普通链接）。
  import { onMount } from 'svelte';
  import { goto, invalidateAll } from '$app/navigation';
  import {
    listComments,
    createComment,
    updateComment,
    deleteComment,
    getMe,
    newClientRequestId,
    favoritePost,
    unfavoritePost,
    unlockPost,
    addCommentReaction,
    removeCommentReaction,
    addPostReaction,
    removePostReaction,
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
  import UserCard from '$lib/components/UserCard.svelte';
  // M14-SEO-01/02/03：文章/讨论页统一 SEO；未发布/未解锁内容 noindex。
  import Seo from '$lib/components/Seo.svelte';
  import { show } from '$lib/ui/toast';
  import { formatTime, formatRelative, charCount, formatCount } from '$lib/utils';
  import type { PostDetailPageData } from './+page.server';

  let { data }: { data: PostDetailPageData } = $props();

  const post = $derived(data.post);
  const authed = $derived(data.authed);
  const loadError = $derived(data.error);
  const locked = $derived(Boolean(post?.closed_at));
  const authorName = $derived(post?.author?.display_name || post?.author?.username || '匿名');
  const statusNotice = $derived(postStatusNotice(post?.status));
  const authorCard = $derived(data.author);
  const boardCard = $derived(data.board);

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

  // ── GAP-FIX：收藏（viewer 态来自 load 投影，交互后乐观更新）──
  let favorited = $state(false);
  let favoriteCount = $state(0);
  let favBusy = $state(false);
  // load 数据变化（导航/invalidateAll）后回同步服务端态。
  $effect(() => {
    const p = data.post;
    favorited = p?.viewer_favorited === true;
    favoriteCount = p?.favorite_count ?? 0;
  });

  async function toggleFavorite() {
    if (!post) return;
    if (!user && !authed) {
      goto('/login');
      return;
    }
    if (favBusy) return;
    favBusy = true;
    try {
      const result = favorited
        ? await unfavoritePost(fetch, post.id)
        : await favoritePost(fetch, post.id, newClientRequestId());
      favorited = result.favorited;
      favoriteCount = result.favorite_count;
      show(result.favorited ? '已收藏' : '已取消收藏', 'success');
      void invalidateAll();
    } catch (err: unknown) {
      const problem = err as Problem;
      if (problem?.status === 401) {
        goto('/login');
        return;
      }
      show(problemMessage(problem) || '收藏操作失败，请稍后重试', 'danger');
    }
    favBusy = false;
  }

  // ── GAP-FIX：分享（复制链接；无 JS 时直接展示链接文本）──
  let shareUrl = $state('');
  onMount(() => {
    shareUrl = window.location.href;
  });

  async function copyShare() {
    const url = shareUrl || (typeof window !== 'undefined' ? window.location.href : '');
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      show('链接已复制', 'success');
    } catch {
      show('复制失败，请手动复制链接文本', 'warning');
    }
  }

  // ── GAP-FIX：付费解锁（unlockPost 幂等；409 余额不足 → danger toast）──
  let unlocking = $state(false);
  const paidLocked = $derived(
    post?.access_summary?.policy === 'paid' && post.access_summary.unlocked === false
  );
  /** 价格投影（posts.price_coin；后端详情未投影时缺失，仅展示兜底文案）。 */
  const unlockPrice = $derived(post?.price_coin);

  async function handleUnlock() {
    if (!post) return;
    if (!user && !authed) {
      goto('/login');
      return;
    }
    if (unlocking) return;
    unlocking = true;
    try {
      await unlockPost(fetch, post.id, newClientRequestId());
      show('解锁成功，正文已更新', 'success');
      await invalidateAll();
    } catch (err: unknown) {
      const problem = err as Problem;
      if (problem?.status === 401) {
        goto('/login');
        return;
      }
      if (problem?.status === 409) {
        show(problemMessage(problem) || 'B 币余额不足，无法解锁', 'danger');
      } else {
        show(problemMessage(problem) || '解锁失败，请稍后重试', 'danger');
      }
    }
    unlocking = false;
  }

  // ── GAP-FIX：评论排序（最新/最早/只看作者）──
  // 后端 GET /posts/{id}/comments 固定 floor ASC（ListQuery 仅 after/limit，
  // 无 sort 参数），故「最新」为前端倒序、「只看作者」为前端过滤。
  type CommentSort = 'latest' | 'earliest' | 'author';
  let commentSort = $state<CommentSort>('latest');
  const commentSortOptions: Array<{ value: CommentSort; label: string }> = [
    { value: 'latest', label: '最新' },
    { value: 'earliest', label: '最早' },
    { value: 'author', label: '只看作者' }
  ];
  const visibleComments = $derived.by(() => {
    let list = comments;
    if (commentSort === 'author') {
      const postAuthor = post?.author?.username;
      list = postAuthor
        ? list.filter((c) => c.author?.username === postAuthor)
        : [];
    }
    return commentSort === 'latest' ? [...list].reverse() : list;
  });

  // ── GAP-FIX：评论轻量点赞（reactions.rs 仅 like；初始计数后端未投影，
  //    首次交互后从 ReactionResult 同步，本地乐观更新）──
  let commentLikes = $state<Record<string, { active: boolean; count: number }>>({});
  let likeBusyId = $state<string | null>(null);

  async function toggleCommentLike(c: Comment) {
    if (!user && !authed) {
      goto('/login');
      return;
    }
    if (likeBusyId) return;
    const current = commentLikes[c.id];
    likeBusyId = c.id;
    try {
      if (current?.active) {
        await removeCommentReaction(fetch, c.id, 'like');
        commentLikes[c.id] = {
          active: false,
          count: Math.max(0, (current?.count ?? 1) - 1)
        };
      } else {
        const result = await addCommentReaction(fetch, c.id, 'like');
        commentLikes[c.id] = { active: result.active, count: result.count };
      }
    } catch (err: unknown) {
      const problem = err as Problem;
      if (problem?.status === 401) {
        goto('/login');
        return;
      }
      show(problemMessage(problem) || '点赞失败，请稍后重试', 'danger');
    }
    likeBusyId = null;
  }

  // ── 原型对齐：主题功能按钮（收藏/赞/分享/举报）位于侧栏「关于作者」卡
  //    （.topic-actions--side），正文卡下方不再单独占一行操作区。
  //    帖子点赞与评论点赞同机制：初始计数后端未投影，首次交互后同步。 ──
  let postLike = $state<{ active: boolean; count: number }>({ active: false, count: 0 });
  let postLikeBusy = $state(false);

  async function togglePostLike() {
    if (!post) return;
    if (!user && !authed) {
      goto('/login');
      return;
    }
    if (postLikeBusy) return;
    postLikeBusy = true;
    try {
      if (postLike.active) {
        await removePostReaction(fetch, post.id, 'like');
        postLike = { active: false, count: Math.max(0, postLike.count - 1) };
      } else {
        const result = await addPostReaction(fetch, post.id, 'like');
        postLike = { active: result.active, count: result.count };
      }
    } catch (err: unknown) {
      const problem = err as Problem;
      if (problem?.status === 401) {
        goto('/login');
        return;
      }
      show(problemMessage(problem) || '点赞失败，请稍后重试', 'danger');
    }
    postLikeBusy = false;
  }

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

  // ── GAP-FIX：代码块复制按钮（正文渲染后 $effect 挂载）──
  // SafeHtml 注入的节点不带 Svelte 作用域属性，样式用 :global；按钮为纯
  // 文本无动画（prefers-reduced-motion 安全），pre 包一层 wrapper 使按钮
  // 不随 pre 的 overflow-x 滚动。
  let proseEl = $state<HTMLDivElement | undefined>(undefined);
  $effect(() => {
    const html = post?.body_html;
    if (!proseEl || !html) return;
    const blocks = proseEl.querySelectorAll('pre > code');
    for (const code of Array.from(blocks)) {
      const pre = code.parentElement;
      if (!pre || pre.dataset.copyMount === '1') continue;
      pre.dataset.copyMount = '1';
      const wrapper = document.createElement('div');
      wrapper.className = 'prose-pre-wrap';
      pre.replaceWith(wrapper);
      wrapper.appendChild(pre);
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'code-copy-btn';
      btn.textContent = '复制';
      btn.setAttribute('aria-label', '复制代码');
      btn.addEventListener('click', async () => {
        const text = code.textContent ?? '';
        try {
          await navigator.clipboard.writeText(text);
          show('代码已复制', 'success');
        } catch {
          show('复制失败，请手动选择复制', 'warning');
        }
      });
      wrapper.appendChild(btn);
    }
  });

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

<div class="container app-page app-topic topic-layout">
  <div class="main-col">
    <nav class="topic-context" aria-label="讨论位置">
      <a href="/">首页</a>
      <span aria-hidden="true">/</span>
      {#if boardCard}
        <a href="/boards/{encodeURIComponent(boardCard.slug)}">{boardCard.name}</a>
        <span aria-hidden="true">/</span>
      {/if}
      <span class="topic-context__current">内容详情</span>
    </nav>

    {#if loadError && !post}
      <p class="input-hint is-error" role="alert">{loadError}</p>
    {:else if post}
      <article class="topic-head-card topic-reading-card">
          <div class="post-title-row" style="margin-bottom:var(--space-3);">
            <h1 style="font-size:var(--text-2xl);">{post.title}</h1>
            <!-- 原型对齐：文章类型徽标移除，统一展示「内容」徽标。 -->
            <span class="sbadge sb-brand" style="margin-left:var(--space-2);">内容</span>
          </div>
          {#if statusNotice}
            <p class="input-hint" role="status" style="margin-bottom:var(--space-3);">{statusNotice}</p>
          {/if}
          <div style="display:flex;align-items:center;gap:var(--space-3);flex-wrap:wrap;padding-bottom:var(--space-4);border-bottom:var(--border-default);margin-bottom:var(--space-4);">
            {#if post.author?.username}
              <!-- 全局壳·UserHoverCard 接线：作者头像+用户名合并为触发链接
                   （hover/focus 出公开资料卡；窄屏点击出底部卡）。 -->
              <UserCard
                user={{
                  username: post.author.username,
                  display_name: post.author.display_name ?? null,
                  level: post.author.level
                }}
                label="查看 {authorName} 的个人资料"
              >
                <span style="display:inline-flex;align-items:center;gap:var(--space-2);">
                  <Avatar name={authorName} size="sm" />
                  <span class="text-link" style="font-size:var(--text-sm);">{authorName}</span>
                </span>
              </UserCard>
            {:else}
              <Avatar name={authorName} size="sm" />
              <span class="text-secondary" style="font-size:var(--text-sm);">{authorName}</span>
            {/if}
            <span class="text-secondary" style="font-size:var(--text-sm);">
              {formatTime(post.created_at)} · {post.view_count ?? 0} 浏览 · {replyCount} 回复
            </span>
            {#if post.access_summary && post.access_summary.policy !== 'public'}
              <span class="sbadge sb-hot">可见性：{policyLabel(post.access_summary.policy)}</span>
            {/if}
            {#if locked}
              <span class="sbadge sb-danger">已锁定</span>
            {/if}
          </div>

          {#if post.body_html && post.access_summary?.unlocked !== false}
            <div class="prose" bind:this={proseEl}>
              <!-- M04-MARKDOWN-08/UI-01：{@html} 仅经 SafeHtml（唯一 sink）；
                   正文为后端渲染清洗的 body_html，前端不做再裁剪。
                   页面级兜底：access_summary.unlocked === false 时即使数据混入
                   body_html 也绝不渲染（白名单在 +page.server.ts 内已做第一层）。 -->
              <SafeHtml html={post.body_html} />
            </div>
          {:else}
            <aside class="topic-restricted" role="note" aria-label="正文不可见">
              <span class="topic-restricted__icon">
                <Icon name="lock" size={16} />
              </span>
              <div class="topic-restricted__body">
                <h3 style="margin:0;font-size:14px;font-weight:600;color:var(--color-text-primary);">
                  {accessPlaceholder}
                </h3>
                <p class="text-secondary" style="margin:4px 0 0;font-size:13px;">
                  解锁前正文不会进入页面 DOM；服务端授权通过后才会渲染完整内容。
                </p>
                {#if paidLocked}
                  <!-- GAP-FIX 付费解锁：unlockPost（幂等键）成功后 invalidateAll
                       重载正文；余额不足 409 → danger toast。价格来自详情投影
                       price_coin（后端未投影时展示兜底文案）。 -->
                  <div style="margin-top:12px;">
                    <Button
                      text={unlocking ? '解锁中…' : unlockPrice !== undefined ? `解锁（${unlockPrice} B币）` : '解锁（消耗 B币）'}
                      variant="primary"
                      size="sm"
                      icon="coins"
                      onclick={handleUnlock}
                      disabled={unlocking}
                    />
                  </div>
                {/if}
              </div>
            </aside>
          {/if}

          <!-- 原型对齐：收藏/赞/分享/举报 位于侧栏「关于作者」卡
               （.topic-actions--side），正文卡下方不设独立操作区。 -->
      </article>

      <section class="topic-comments app-card" style="margin-top:14px;" aria-labelledby="comments-title">
        <div class="topic-comments__head" style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-3);flex-wrap:wrap;margin-bottom:10px;">
          <h2 id="comments-title" style="font-size:18px;margin:0;">回复 · {commentsLoaded ? comments.length : ''} 条</h2>
          {#if commentsLoaded}
            <!-- 评论排序（M18-MISC-02 对齐原型）：最新 / 最早 / 只看作者。 -->
            <div class="tabs" role="tablist" aria-label="评论排序" style="padding:0;border-bottom:var(--border-default);">
              {#each commentSortOptions as opt (opt.value)}
                <button
                  type="button"
                  role="tab"
                  class="tab {commentSort === opt.value ? 'is-active' : ''}"
                  aria-selected={commentSort === opt.value}
                  onclick={() => (commentSort = opt.value)}
                >
                  {opt.label}
                </button>
              {/each}
            </div>
          {/if}
        </div>

        {#if commentsLoaded && comments.length > 0}
          {#if visibleComments.length === 0}
            <div class="card"><div class="card-body"><EmptyState icon="message-square" title="没有作者回复" desc="换个排序看看全部回复" /></div></div>
          {:else}
            <div class="comment-list" style="display:flex;flex-direction:column;gap:var(--space-3);">
              {#each visibleComments as comment (comment.id)}
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
                      <div style="display:flex;gap:var(--space-2);margin-top:var(--space-2);align-items:center;flex-wrap:wrap;">
                        {#if authed || user}
                          <!-- GAP-FIX 评论轻量点赞：reactions.rs 仅 like 一种反应；
                               初始计数未投影（首次交互后同步），本地乐观更新。 -->
                          <button
                            type="button"
                            class="comment-like-btn {commentLikes[comment.id]?.active ? 'is-active' : ''}"
                            aria-pressed={commentLikes[comment.id]?.active === true}
                            aria-label={commentLikes[comment.id]?.active ? '取消点赞' : '点赞'}
                            onclick={() => toggleCommentLike(comment)}
                            disabled={likeBusyId === comment.id}
                          >
                            <Icon name="thumbs-up" size={14} />
                            {#if (commentLikes[comment.id]?.count ?? 0) > 0}
                              <span>{commentLikes[comment.id].count}</span>
                            {/if}
                          </button>
                        {/if}
                        <Button text="引用" variant="ghost" size="sm" icon="quote" onclick={() => quoteComment(comment)} disabled={locked} />
                        <!-- M18-MISC-02：逐条回复举报入口（对齐原型） -->
                        <a
                          href="/moderation/report?target_type=comment&target_id={encodeURIComponent(comment.id)}"
                          class="btn btn-ghost btn-sm"
                          style="text-decoration:none;display:inline-flex;align-items:center;gap:4px;"
                          aria-label="举报 {authorLabel(comment)} 的回复"
                        >
                          <Icon name="flag" size={14} />
                          举报
                        </a>
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
          {/if}
        {:else if commentsLoaded}
          <div class="app-empty" style="min-height:190px;">
            <Icon name="message-square" size={34} />
            <b>暂无回复</b>
            <span>快来抢沙发！</span>
          </div>
        {/if}

        {#if locked}
          <div class="card" style="margin-top:var(--space-4);border-color:var(--color-warning);">
            <div class="card-body" style="display:flex;align-items:center;gap:var(--space-2);">
              <Icon name="lock" size={16} />
              <span class="text-secondary">该帖已锁定，不能继续回复。</span>
            </div>
          </div>
        {:else if authed || user}
          <form class="topic-editor" style="margin-top:14px;" method="POST" onsubmit={handleSubmit}>
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

  {#if post}
    <!-- GAP-FIX：右侧栏（桌面）「关于作者」+「所在板块」卡；数据由 load 组合
         （作者公开投影 / 板块列表按 board_id 反查），失败静默降级不渲染。 -->
    <div class="side-col">
      <div class="card">
        <div class="card-header"><span class="card-title">关于作者</span></div>
        <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
          {#if authorCard}
            <UserCard
              user={{
                username: authorCard.username,
                display_name: authorCard.display_name,
                level: authorCard.level,
                bio: authorCard.bio,
                signature: authorCard.signature
              }}
              label="查看 {authorCard.display_name || authorCard.username} 的个人资料"
            >
              <span style="display:flex;align-items:center;gap:var(--space-3);">
                <Avatar name={authorCard.display_name || authorCard.username} size="md" />
                <span>
                  <span style="display:block;font-weight:var(--weight-semibold);">{authorCard.display_name || authorCard.username}</span>
                  <span class="text-secondary" style="font-size:var(--text-sm);">LV.{authorCard.level}</span>
                </span>
              </span>
            </UserCard>
            {#if authorCard.bio}
              <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">{authorCard.bio}</p>
            {/if}
            <div style="display:flex;gap:var(--space-4);font-size:var(--text-sm);">
              <span><strong>{formatCount(authorCard.post_count)}</strong> <span class="text-secondary">帖子</span></span>
              <span><strong>{formatCount(authorCard.followers)}</strong> <span class="text-secondary">粉丝</span></span>
              <span><strong>{formatCount(authorCard.following)}</strong> <span class="text-secondary">关注</span></span>
            </div>
          {:else if post.author?.username}
            <!-- 作者卡数据拉取失败：退回帖子投影内的最小作者信息（无统计）。 -->
            <UserCard
              user={{
                username: post.author.username,
                display_name: post.author.display_name ?? null,
                level: post.author.level
              }}
              label="查看 {authorName} 的个人资料"
            >
              <span style="display:flex;align-items:center;gap:var(--space-3);">
                <Avatar name={authorName} size="md" />
                <span style="font-weight:var(--weight-semibold);">{authorName}</span>
              </span>
            </UserCard>
          {:else}
            <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">匿名作者</p>
          {/if}

          <!-- 原型对齐：主题功能按钮位于侧栏作者卡（.topic-actions--side 2×2）。 -->
          <div class="topic-actions topic-actions--side">
            <Button
              text={favBusy ? '处理中…' : favorited ? '已收藏' : '收藏'}
              variant={favorited ? 'secondary' : 'ghost'}
              size="sm"
              icon="bookmark"
              onclick={toggleFavorite}
              disabled={favBusy}
            />
            <button
              type="button"
              class="btn ghost btn-ghost sm topic-like-btn {postLike.active ? 'is-active' : ''}"
              aria-pressed={postLike.active}
              aria-label={postLike.active ? '取消点赞' : '点赞'}
              disabled={postLikeBusy}
              onclick={togglePostLike}
            >
              <Icon name="thumbs-up" size={14} />
              <span>{postLike.active ? '已赞' : '赞'}{postLike.count > 0 ? ` ${formatCount(postLike.count)}` : ''}</span>
            </button>
            <Button text="分享" variant="ghost" size="sm" icon="share-2" onclick={copyShare} />
            <Button
              text="举报"
              variant="ghost"
              size="sm"
              icon="flag"
              href={`/moderation/report?post=${encodeURIComponent(post.id)}`}
            />
          </div>
          {#if favoriteCount > 0}
            <p class="app-muted" style="margin:8px 0 0;font-size:12px;">{formatCount(favoriteCount)} 人收藏</p>
          {/if}
        </div>
      </div>

      {#if boardCard}
        <div class="card" style="margin-top:var(--space-4);">
          <div class="card-header"><span class="card-title">所在板块</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
            <a href="/boards/{encodeURIComponent(boardCard.slug)}" class="text-link" style="font-weight:var(--weight-semibold);">{boardCard.name}</a>
            {#if boardCard.description}
              <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">{boardCard.description}</p>
            {/if}
            <div class="text-secondary" style="font-size:var(--text-sm);">
              {formatCount(boardCard.post_count)} 帖子
              {#if boardCard.today_post_count !== undefined}
                <!-- 今日新增：UTC 日界内新帖（authed 投影才返回）。 -->
                <span>· 今日 +{formatCount(boardCard.today_post_count)}</span>
              {/if}
            </div>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* 评论轻量点赞（SafeHtml 之外的页面级按钮，scoped 正常生效）。 */
  .comment-like-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 30px;
    padding: 0 10px;
    border: var(--border-default);
    border-radius: 999px;
    background: var(--color-surface);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .comment-like-btn:hover {
    color: var(--color-text-primary);
    border-color: var(--color-border-strong);
  }
  .comment-like-btn.is-active {
    color: var(--color-brand);
    border-color: var(--color-brand);
  }
  .comment-like-btn:focus-visible {
    outline: 2px solid var(--color-brand);
    outline-offset: 1px;
  }

  /* 侧栏主题点赞按钮（.btn.ghost 基座）的激活态。 */
  .topic-like-btn.is-active {
    color: var(--color-brand);
    border-color: var(--color-brand);
  }

  /* 代码块复制按钮（$effect 注入的 DOM 无作用域属性 → :global）。
     pre 外包一层 wrapper（position:relative），使按钮不随 pre 的
     overflow-x 滚动；纯文本无动画，prefers-reduced-motion 安全。 */
  :global(.prose-pre-wrap) {
    position: relative;
  }
  :global(.code-copy-btn) {
    position: absolute;
    top: var(--space-2);
    right: var(--space-2);
    padding: 2px 10px;
    font-size: var(--text-xs);
    border: var(--border-default);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-secondary);
    cursor: pointer;
  }
  :global(.code-copy-btn:hover) {
    color: var(--color-text-primary);
    border-color: var(--color-border-strong);
  }
</style>
