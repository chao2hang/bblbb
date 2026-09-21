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
  // - 侧栏「关于作者」卡 + 「板块导航」（共享 BoardNav：所在板块高亮，
  //   读帖时可跳转其他板块）。
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
    getPostReactions,
    recordTrustReadTime,
    type Comment,
    type User
  } from '$lib/api/client';
  import {
    problemMessage,
    problemRecovery,
    postStatusNotice,
    type Problem
  } from '$lib/errors';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import type { PublicPresentationTokens } from '$lib/api/types';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import ReactionBar from '$lib/components/ReactionBar.svelte';
  import SafeHtml from '$lib/components/SafeHtml.svelte';
  import UserCard from '$lib/components/UserCard.svelte';
  import SimpleCommentEditor from '$lib/components/editor/SimpleCommentEditor.svelte';
  // M14-SEO-01/02/03：文章/讨论页统一 SEO；未发布/未解锁内容 noindex。
  import Seo from '$lib/components/Seo.svelte';
  import BoardNav from '$lib/components/forum/BoardNav.svelte';
  import { markPostRead } from '$lib/readState.svelte';
  import { show } from '$lib/ui/toast';
  import { formatDate, formatRelative, charCount, formatCount } from '$lib/utils';
  import type { PostDetailPageData } from './+page.server';

  let { data }: { data: PostDetailPageData } = $props();

  const post = $derived(data.post);
  const loadError = $derived(data.error);
  const locked = $derived(Boolean(post?.closed_at));
  const authorName = $derived(post?.author?.display_name || post?.author?.username || '匿名');
  const statusNotice = $derived(postStatusNotice(post?.status));
  const authorCard = $derived(data.author);
  const authorPresentation = $derived<PublicPresentationTokens | null>(
    (authorCard?.presentation_tokens as PublicPresentationTokens | null | undefined) ??
    (post?.author?.presentation_tokens as PublicPresentationTokens | null | undefined) ??
    null
  );

  function commentPresentation(c: Comment): PublicPresentationTokens | null {
    if (c.author?.username && c.author.username === post?.author?.username) {
      return authorPresentation;
    }
    if (c.author?.username && (c.author.username === user?.username || c.author.username === sessionUser?.username)) {
      return (user?.presentation_tokens ?? sessionUser?.presentation_tokens) ?? null;
    }
    return (c.author?.presentation_tokens as PublicPresentationTokens | null | undefined) ?? null;
  }
  const boardCard = $derived(data.board);
  /** 侧栏板块导航行（与 board 同源反查；空数组 → BoardNav 整卡不渲染）。
   *  帖子所在板块经 activeSlug 高亮，读帖时可直接跳转其他板块。 */
  const navBoards = $derived(data.boards ?? []);

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

  // ── 会话判定（修复「未登录也渲染回复表单」）──
  // 只信任 /me 验证过的会话：SSR 首帧用根 layout 的服务端 user（
  // +layout.server.ts 调 /me 校验，失效/被吊销的 Cookie 得 null，SvelteKit
  // 将其并入页面 data）；hydration 后叠加客户端 getMe 结果。不再用「会话
  // Cookie 是否存在」判定（旧 data.authed）——过期 Cookie 曾让匿名访客
  // 看到完整回复表单（导航栏已是登录/注册，表单提交必 401）。
  const sessionUser = $derived(data.user ?? null);
  const authed = $derived(Boolean(sessionUser || user));
  const replyHref = $derived(
    !authed
      ? `/login?next=${encodeURIComponent(`/posts/${post?.id ?? ''}#comment-input`)}`
      : locked
        ? '#comments-title'
        : '#comment-input'
  );

  const isAuthor = $derived(Boolean(user && post?.author?.username && user.username === post.author.username));
  const canEdit = $derived(Boolean(user && (isAuthor || user.roles?.includes('admin') || user.roles?.includes('moderator') || user.roles?.includes('administrator'))));
  let comments = $state<Comment[]>([]);
  let commentsLoaded = $state(false);
  let commentsProblem = $state<Problem | null>(null);
  /** 本次会话内新增/删除的回复数（对服务端 reply_count 的增量修正）。 */
  let replyDelta = $state(0);
  const replyCount = $derived((post?.reply_count ?? 0) + replyDelta);

  // ── 主贴底部统计行：回复过的用户（头像栈）──
  // 浏览量/回复数/参与用户从标题行下移到主贴底部（截图对齐）；回复者从
  // 已加载评论去重派生（保序、客户端加载后出现，属渐进增强）。头像最多
  // 展示 8 个，超出折叠为 +N。key 用 id/username 兜底，匿名聚合为一个；
  // 同时保留公开装扮投影，确保统计行的头像框与评论区/导航栏一致。
  const MAX_REPLY_AVATARS = 8;
  type PostReplier = {
    key: string;
    name: string;
    username: string | null;
    level?: number;
    avatarAttachmentId?: string | null;
    presentation?: PublicPresentationTokens | null;
  };
  const repliers = $derived.by<PostReplier[]>(() => {
    const seen = new Set<string>();
    const list: PostReplier[] = [];
    for (const c of comments) {
      const a = c.author;
      const key = a?.id || a?.username || a?.display_name || '匿名';
      if (seen.has(key)) continue;
      seen.add(key);
      list.push({
        key,
        name: a?.display_name || a?.username || '匿名',
        username: a?.username || null,
        level: a?.level,
        avatarAttachmentId: a?.avatar_attachment_id ?? null,
        // 与评论头部/作者卡共用同一解析路径：作者和当前用户的装扮
        // 可能来自页面级投影，而不一定重复出现在评论作者字段中。
        presentation: commentPresentation(c)
      });
    }
    return list;
  });
  const shownRepliers = $derived(repliers.slice(0, MAX_REPLY_AVATARS));
  const hiddenRepliers = $derived(Math.max(0, repliers.length - shownRepliers.length));

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

  // ── M20-TRUST：阅读时长心跳（best-effort）──
  // 登录用户在详情页每 30s 上报一次阅读时长（页面不可见时暂停）；服务端
  // 钳制（单请求 ≤60s、每人每日 ≤7200s），失败静默。SSR 阶段不执行。
  onMount(() => {
    let timer: ReturnType<typeof setInterval> | undefined;
    void (async () => {
      try {
        const me = await getMe(fetch);
        if (!me) return;
        timer = setInterval(() => {
          if (document.visibilityState === 'visible') {
            void recordTrustReadTime(fetch, 30);
          }
        }, 30_000);
      } catch {
        /* 心跳为增强功能；任何失败忽略 */
      }
    })();
    return () => {
      if (timer) clearInterval(timer);
    };
  });

  async function copyShare(event?: MouseEvent) {
    event?.preventDefault();
    const url = shareUrl || (typeof window !== 'undefined' ? window.location.href : '');
    if (!url) return;

    if (typeof navigator !== 'undefined' && typeof navigator.share === 'function') {
      try {
        await navigator.share({ title: post?.title ?? '帖子', url });
        show('已打开分享面板', 'success');
        return;
      } catch (error) {
        // 用户关闭系统分享面板时不再弹出失败提示，继续尝试复制链接。
        if (error instanceof DOMException && error.name === 'AbortError') return;
      }
    }

    try {
      await navigator.clipboard.writeText(url);
      show('链接已复制', 'success');
    } catch {
      show('复制失败，请长按分享按钮复制链接', 'warning');
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

  // ── 引用回复展示（M04-UI-06 补全）──
  // 「引用」创建的回复带 parent_id，但列表此前只渲染 body_html，被引用楼层
  // 完全不可见（引用无效果）。这里按 parent_id 反查同帖评论，在回复卡片
  // 顶部渲染被引用楼层的紧凑引用块；被引用楼层不在当前数据（软删/受限）
  // 时不渲染，避免悬空引用。
  const commentsById = $derived(new Map(comments.map((c) => [c.id, c])));
  function quotedParentOf(c: Comment): Comment | null {
    if (!c.parent_id) return null;
    return commentsById.get(c.parent_id) ?? null;
  }

  // ── 互动点赞与狗头表态（点赞、狗头等反应支持）：本地乐观更新 ──
  let commentLikes = $state<Record<string, { active: boolean; count: number }>>({});
  let commentDoges = $state<Record<string, { active: boolean; count: number }>>({});
  let likeBusyId = $state<string | null>(null);

  // 单激活语义（一人只保留一个激活反应，可切换）：评论反应变更后重算 like/doge 两个状态。
  function applyCommentReactionMutation(
    cid: string,
    p: MutationPayload
  ) {
    const likeTarget = p.reaction === 'like' || p.reaction === '👍';
    const dogeTarget = p.reaction === 'doge' || p.reaction === '🐶';
    const like = commentLikes[cid] ?? { active: false, count: 0 };
    const doge = commentDoges[cid] ?? { active: false, count: 0 };
    commentLikes[cid] = {
      active: p.active && likeTarget,
      count:
        countOfIn(p.counts, 'like', '👍') ??
        (likeTarget ? p.count : p.active && like.active ? Math.max(0, like.count - 1) : like.count)
    };
    commentDoges[cid] = {
      active: p.active && dogeTarget,
      count:
        countOfIn(p.counts, 'doge', '🐶') ??
        (dogeTarget ? p.count : p.active && doge.active ? Math.max(0, doge.count - 1) : doge.count)
    };
  }

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
        const result = await removeCommentReaction(fetch, c.id, 'like');
        applyCommentReactionMutation(c.id, {
          reaction: 'like',
          active: false,
          count: typeof result?.count === 'number' ? result.count : Math.max(0, (current?.count ?? 1) - 1),
          counts: result?.counts
        });
      } else {
        // 单激活：狗头激活中时先乐观切换（失败回滚）
        const dogeSnapshot = commentDoges[c.id] ? { ...commentDoges[c.id] } : null;
        if (commentDoges[c.id]?.active) {
          commentDoges[c.id] = { active: false, count: Math.max(0, commentDoges[c.id].count - 1) };
        }
        try {
          const result = await addCommentReaction(fetch, c.id, 'like');
          applyCommentReactionMutation(c.id, {
            reaction: 'like',
            active: result.active,
            count: result.count,
            counts: result.counts
          });
        } catch (err) {
          if (dogeSnapshot) commentDoges[c.id] = dogeSnapshot;
          throw err;
        }
      }
    } catch (err: unknown) {
      const problem = err as Problem;
      if (problem?.status === 401) {
        goto('/login');
        return;
      }
      show(problemMessage(problem) || '点赞失败，请稍后重试', 'danger');
    } finally {
      likeBusyId = null;
    }
  }

  async function toggleCommentDoge(c: Comment) {
    if (!user && !authed) {
      goto('/login');
      return;
    }
    if (likeBusyId) return;
    const current = commentDoges[c.id];
    likeBusyId = c.id;
    try {
      if (current?.active) {
        const result = await removeCommentReaction(fetch, c.id, 'doge');
        applyCommentReactionMutation(c.id, {
          reaction: 'doge',
          active: false,
          count: typeof result?.count === 'number' ? result.count : Math.max(0, (current?.count ?? 1) - 1),
          counts: result?.counts
        });
      } else {
        // 单激活：点赞激活中时先乐观切换（失败回滚）
        const likeSnapshot = commentLikes[c.id] ? { ...commentLikes[c.id] } : null;
        if (commentLikes[c.id]?.active) {
          commentLikes[c.id] = { active: false, count: Math.max(0, commentLikes[c.id].count - 1) };
        }
        try {
          const result = await addCommentReaction(fetch, c.id, 'doge');
          applyCommentReactionMutation(c.id, {
            reaction: 'doge',
            active: result.active,
            count: result.count,
            counts: result.counts
          });
        } catch (err) {
          if (likeSnapshot) commentLikes[c.id] = likeSnapshot;
          throw err;
        }
      }
    } catch (err: unknown) {
      const problem = err as Problem;
      if (problem?.status === 401) {
        goto('/login');
        return;
      }
      show(problemMessage(problem) || '表态失败，请稍后重试', 'danger');
    } finally {
      likeBusyId = null;
    }
  }

  // ── 帖子反应状态：侧栏快捷赞与正文 ReactionBar 联动 ──
  let postLike = $state<{ active: boolean; count: number }>({ active: false, count: 0 });
  let postLikeBusy = $state(false);
  let postDoge = $state<{ active: boolean; count: number }>({ active: false, count: 0 });
  // 本地是否已发起过帖子反应变更（为 true 时跳过挂载水合，防止旧快照覆盖）
  let postReactionsTouched = false;

  // 单激活语义（一人只保留一个激活反应，可切换）：
  // 任一反应变更成功后，用服务端结果重算行内 like/doge 两个状态。
  type MutationPayload = {
    reaction: string;
    active: boolean;
    count: number;
    counts?: Record<string, number>;
  };
  function countOfIn(
    counts: Record<string, number> | undefined,
    name: string,
    emoji: string
  ): number | undefined {
    if (!counts) return undefined;
    if (counts[name] !== undefined) return Number(counts[name]);
    if (counts[emoji] !== undefined) return Number(counts[emoji]);
    return undefined;
  }
  function applyPostReactionMutation(p: MutationPayload) {
    const likeTarget = p.reaction === 'like' || p.reaction === '👍';
    const dogeTarget = p.reaction === 'doge' || p.reaction === '🐶';
    postLike = {
      active: p.active && likeTarget,
      count:
        countOfIn(p.counts, 'like', '👍') ??
        (likeTarget ? p.count : p.active && postLike.active ? Math.max(0, postLike.count - 1) : postLike.count)
    };
    postDoge = {
      active: p.active && dogeTarget,
      count:
        countOfIn(p.counts, 'doge', '🐶') ??
        (dogeTarget ? p.count : p.active && postDoge.active ? Math.max(0, postDoge.count - 1) : postDoge.count)
    };
  }

  async function togglePostLike() {
    if (!post) return;
    if (!user && !authed) {
      goto('/login');
      return;
    }
    if (postLikeBusy) return;
    postLikeBusy = true;
    postReactionsTouched = true;
    try {
      if (postLike.active) {
        const result = await removePostReaction(fetch, post.id, 'like');
        applyPostReactionMutation({
          reaction: 'like',
          active: false,
          count: typeof result?.count === 'number' ? result.count : Math.max(0, postLike.count - 1),
          counts: result?.counts
        });
      } else {
        // 单激活：狗头激活中时先乐观切换（失败回滚）
        const dogeSnapshot = { ...postDoge };
        if (postDoge.active) postDoge = { active: false, count: Math.max(0, postDoge.count - 1) };
        try {
          const result = await addPostReaction(fetch, post.id, 'like');
          applyPostReactionMutation({
            reaction: 'like',
            active: result.active,
            count: result.count,
            counts: result.counts
          });
        } catch (err) {
          postDoge = dogeSnapshot;
          throw err;
        }
      }
    } catch (err: unknown) {
      const problem = err as Problem;
      if (problem?.status === 401) {
        goto('/login');
        return;
      }
      show(problemMessage(problem) || '点赞失败，请稍后重试', 'danger');
    } finally {
      postLikeBusy = false;
    }
  }

  onMount(async () => {
    user = await getMe(fetch);
    if (post) {
      // 进入详情页即本地标记已读（首页列表左侧色条「未读亮 / 已读暗」依据）
      markPostRead(post.id);
      await loadComments();
    }
    // 行内赞/狗头初始态水合（ ReactionBar 的 Pill 由自身 loadDetail 独立水合）。
    // 单激活：viewer_reactions 至多一项；若本地已发起过变更则跳过，避免旧快照覆盖。
    if (post && !postReactionsTouched) {
      try {
        const detail = await getPostReactions(fetch, post.id);
        const likeCount = detail.counts?.['like'] ?? detail.counts?.['👍'] ?? 0;
        const dogeCount = detail.counts?.['doge'] ?? detail.counts?.['🐶'] ?? 0;
        const activeLike = (detail.viewer_reactions ?? []).some((r) => r === 'like' || r === '👍');
        const activeDoge = (detail.viewer_reactions ?? []).some((r) => r === 'doge' || r === '🐶');
        postLike = { active: activeLike, count: likeCount };
        postDoge = { active: activeDoge && !activeLike, count: dogeCount };
      } catch {
        /* 水合失败保持初值；后续交互以服务端响应为准 */
      }
    }
  });

  async function loadComments() {
    if (!post) return;
    commentsLoaded = false;
    commentsProblem = null;
    try {
      const result = await listComments(fetch, post.id);
      comments = result.items;
    } catch (err: unknown) {
      comments = [];
      commentsProblem = err as Problem;
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

  // 楼层锚点平滑滚动（楼层编号链接与引用块共用）：替代浏览器瞬时跳转。
  // - 目标楼层不在 DOM（如「只看作者」过滤后）→ 不拦截，退回默认锚点行为；
  // - prefers-reduced-motion 用户保持瞬时定位（不做动画）；
  // - 用 history.replaceState 同步 URL hash（不产生瞬时跳转、不污染历史）。
  function scrollToFloor(event: MouseEvent, floor: number) {
    const el = document.getElementById(`floor-${floor}`);
    if (!el) return;
    event.preventDefault();
    const reduced =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    el.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' });
    history.replaceState(null, '', `#floor-${floor}`);
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

  /** 内容访问已解锁判定：当受限策略主题（after_reply / paid / level）
   *  已解锁正文时，以专属解锁样式展示，向读者明确该内容为权限解锁区块。 */
  const isRestrictedUnlocked = $derived(
    Boolean(
      post?.body_html &&
        post?.access_summary?.unlocked !== false &&
        post?.access_summary?.policy &&
        (post.access_summary.policy === 'after_reply' ||
          post.access_summary.policy === 'paid' ||
          post.access_summary.policy === 'level')
    )
  );

  const unlockedInfo = $derived.by(() => {
    const policy = post?.access_summary?.policy;
    if (policy === 'after_reply') {
      return {
        title: '回复可见内容已解锁',
        desc: isAuthor
          ? '你是本文作者，拥有完整内容查看权限'
          : canEdit
            ? '你拥有管理审核权限，已解锁完整内容'
            : '你已参与本帖回复，以下为解锁后的隐藏内容'
      };
    }
    if (policy === 'paid') {
      return {
        title: '付费内容已解锁',
        desc: isAuthor
          ? '你是本文作者，拥有完整内容查看权限'
          : canEdit
            ? '你拥有管理审核权限，已解锁完整内容'
            : '你已成功购买，以下为解锁后的完整内容'
      };
    }
    if (policy === 'level') {
      return {
        title: '等级限制内容已解锁',
        desc: isAuthor
          ? '你是本文作者，拥有完整内容查看权限'
          : canEdit
            ? '你拥有管理审核权限，已解锁完整内容'
            : '你的账号等级已达到要求，以下为解锁后的完整内容'
      };
    }
    return {
      title: '受限内容已解锁',
      desc: '以下为解锁后的完整内容'
    };
  });
</script>

<Seo
  title={post ? post.title : '帖子'}
  description={post ? post.title : '帖子内容'}
  noindex={!indexable}
  og={{ type: post?.post_type === 'article' ? 'article' : 'website' }}
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
    {#if loadError && !post}
      <p class="input-hint is-error" role="alert">{loadError}</p>
    {:else if post}
      <nav class="topic-mobile-context" aria-label="帖子导航">
        <a
          href={boardCard ? `/boards/${encodeURIComponent(boardCard.slug)}` : '/'}
          class="topic-mobile-context__back"
        >
          <Icon name="chevron-left" size={17} />
          <span>{boardCard?.name ?? '返回'}</span>
        </a>
        <span class="topic-mobile-context__label">帖子</span>
        <a href={replyHref} class="topic-mobile-context__reply">
          <Icon name="message-circle" size={15} />
          <span>{locked ? '查看回复' : '回复'}</span>
        </a>
      </nav>
      <article class="topic-head-card topic-reading-card">
          {#if post.status === 'deleted' || post.deleted_at}
            <div
              class="admin-preview-banner is-deleted"
              role="status"
              style="display:flex;align-items:center;gap:8px;padding:10px 14px;border-radius:var(--radius-md, 6px);background:rgba(239, 68, 68, 0.12);border:1px solid rgba(239, 68, 68, 0.35);color:#ef4444;font-size:13px;font-weight:600;margin-bottom:var(--space-4);"
            >
              <Icon name="alert-triangle" size={16} />
              <span>【管理员预览】此帖子已被软删除，当前仅具备管理权限的人员可见。</span>
            </div>
          {:else if post.status === 'pending_review'}
            <div
              class="admin-preview-banner is-pending"
              role="status"
              style="display:flex;align-items:center;gap:8px;padding:10px 14px;border-radius:var(--radius-md, 6px);background:rgba(245, 158, 11, 0.12);border:1px solid rgba(245, 158, 11, 0.35);color:#d97706;font-size:13px;font-weight:600;margin-bottom:var(--space-4);"
            >
              <Icon name="shield-alert" size={16} />
              <span>【待审核预览】此帖子处于待审核状态（pending_review），尚未公开。</span>
            </div>
          {:else if post.status === 'draft'}
            <div
              class="admin-preview-banner is-draft"
              role="status"
              style="display:flex;align-items:center;gap:8px;padding:10px 14px;border-radius:var(--radius-md, 6px);background:rgba(100, 116, 139, 0.12);border:1px solid rgba(100, 116, 139, 0.35);color:#64748b;font-size:13px;font-weight:600;margin-bottom:var(--space-4);"
            >
              <Icon name="file-text" size={16} />
              <span>【草稿预览】此帖子为草稿状态，尚未公开发布。</span>
            </div>
          {:else if post.status === 'hidden'}
            <div
              class="admin-preview-banner is-hidden"
              role="status"
              style="display:flex;align-items:center;gap:8px;padding:10px 14px;border-radius:var(--radius-md, 6px);background:rgba(107, 114, 128, 0.12);border:1px solid rgba(107, 114, 128, 0.35);color:#6b7280;font-size:13px;font-weight:600;margin-bottom:var(--space-4);"
            >
              <Icon name="eye-off" size={16} />
              <span>【已隐藏预览】此帖子已被下架隐藏，仅管理人员可见。</span>
            </div>
          {/if}
          <div class="post-title-row" style="margin-bottom:var(--space-3);">
            <h1 style="font-size:var(--text-2xl);">{post.title}</h1>
            <!-- 原型对齐：文章类型徽标移除，统一展示「内容」徽标。间距由 .post-title-row 的 flex gap 提供。 -->
            <span class="sbadge sb-brand">内容</span>
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
                  level: post.author.level,
                  avatar_attachment_id: post.author.avatar_attachment_id ?? null
                }}
                presentation={{ presentation_tokens: authorPresentation }}
                label="查看 {authorName} 的个人资料"
              >
                <span style="display:inline-flex;align-items:center;gap:var(--space-2);">
                  <CosmeticAvatar name={authorName} size="sm" presentation={authorPresentation} avatarAttachmentId={post.author?.avatar_attachment_id ?? null} seed={post.author?.username ?? post.author?.id ?? authorName} />
                  <span class="text-link" style="font-size:var(--text-sm);">
                    <CosmeticName name={authorName} presentation={authorPresentation} />
                  </span>
                </span>
              </UserCard>
            {:else}
              <CosmeticAvatar name={authorName} size="sm" presentation={authorPresentation} avatarAttachmentId={post.author?.avatar_attachment_id ?? null} seed={post.author?.username ?? post.author?.id ?? authorName} />
              <span class="text-secondary" style="font-size:var(--text-sm);">
                <CosmeticName name={authorName} presentation={authorPresentation} />
              </span>
            {/if}
            {#if post.access_summary && post.access_summary.policy !== 'public'}
              <span class="sbadge sb-hot">可见性：{policyLabel(post.access_summary.policy)}</span>
            {/if}
            {#if locked}
              <span class="sbadge sb-danger">已锁定</span>
            {/if}
            <!-- 右侧组：编辑入口 + 发布时间（时间排在「编辑内容」之后）。
                 浏览量/回复数已下移到主贴底部统计行，不再出现在标题行。 -->
            <span style="margin-left:auto;display:inline-flex;align-items:center;gap:var(--space-3);">
              {#if canEdit}
                <a
                  href="/editor?post_id={encodeURIComponent(post.id)}"
                  class="btn ghost sm"
                  style="text-decoration:none;display:inline-flex;align-items:center;gap:4px;padding:2px 10px;height:26px;font-size:12px;"
                >
                  <Icon name="pen-line" size={13} />
                  <span>{isAuthor ? '编辑内容' : '管理代改'}</span>
                </a>
              {/if}
              <time class="text-secondary" style="font-size:var(--text-sm);" datetime={new Date(post.created_at).toISOString()}>
                {formatDate(post.created_at)}
              </time>
            </span>
          </div>

          {#if post.body_html && post.access_summary?.unlocked !== false}
            {#if isRestrictedUnlocked}
              <!-- 权限解锁内容展示：非 public 策略（after_reply / paid / level）解锁后，
                   以专属解锁容器与提示呈现，明确标记该区块为已解锁权限内容。 -->
              <div class="restricted-unlocked topic-unlocked" role="region" aria-label={unlockedInfo.title}>
                <div class="topic-unlocked__header">
                  <span class="topic-unlocked__icon">
                    <Icon name="unlock" size={16} />
                  </span>
                  <div class="topic-unlocked__meta">
                    <span class="topic-unlocked__title">{unlockedInfo.title}</span>
                    <span class="topic-unlocked__desc">{unlockedInfo.desc}</span>
                  </div>
                  <span class="topic-unlocked__badge">
                    <Icon name="check" size={12} />
                    <span>已解锁</span>
                  </span>
                </div>
                <div class="topic-unlocked__divider"></div>
                <div class="prose topic-unlocked__prose" bind:this={proseEl}>
                  <SafeHtml html={post.body_html} />
                </div>
              </div>
            {:else}
              <div class="prose" bind:this={proseEl}>
                <!-- M04-MARKDOWN-08/UI-01：{@html} 仅经 SafeHtml（唯一 sink）；
                     正文为后端渲染清洗的 body_html，前端不做再裁剪。
                     页面级兜底：access_summary.unlocked === false 时即使数据混入
                     body_html 也绝不渲染（白名单在 +page.server.ts 内已做第一层）。 -->
                <SafeHtml html={post.body_html} />
              </div>
            {/if}
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

          {#if post.tags && post.tags.length > 0}
            <div class="topic-tags" aria-label="帖子标签">
              {#each post.tags as tag}
                <a href="/tags/{encodeURIComponent(tag)}" class="app-tag">#{tag}</a>
              {/each}
            </div>
          {/if}

          {#if post}
            <!-- 表态入口：无背景、无标题，仅保留反应条（用户要求去掉灰底与提示文案） -->
            <div style="margin-top: var(--space-4);">
              <ReactionBar
                targetType="post"
                targetId={post.id}
                reactions={[
                  { reaction: 'like', count: postLike.count, active: postLike.active },
                  { reaction: 'doge', count: postDoge.count, active: postDoge.active }
                ]}
                authed={authed}
                currentUser={user}
                fetchFn={fetch}
                onReactionMutated={applyPostReactionMutation}
              />
            </div>

            <!-- 移动端主操作：把收藏/赞/分享/举报放在正文末尾，
                 不让用户滚完整个评论区后才找到主题动作；桌面由侧栏承载同一组操作。 -->
            <div class="topic-mobile-actions" aria-label="帖子操作">
              {#if authed}
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
              {:else}
                <Button
                  text="登录后可收藏 / 点赞"
                  variant="ghost"
                  size="sm"
                  icon="bookmark"
                  href="/login?next={encodeURIComponent(`/posts/${post.id}`)}"
                  extraClass="topic-login-cta"
                />
              {/if}
              <a
                href={`/posts/${encodeURIComponent(post.id)}`}
                class="btn ghost btn-ghost sm"
                onclick={(event) => void copyShare(event)}
                aria-label="分享帖子"
              >
                <Icon name="share-2" size={14} />
                <span>分享</span>
              </a>
              <a
                href={authed
                  ? `/moderation/report?post=${encodeURIComponent(post.id)}`
                  : `/login?next=${encodeURIComponent(`/moderation/report?post=${post.id}`)}`}
                class="btn btn-ghost btn-sm"
                style="text-decoration:none;display:inline-flex;align-items:center;justify-content:center;gap:4px;"
              >
                <Icon name="flag" size={14} />
                <span>{authed ? '举报' : '登录后举报'}</span>
              </a>
            </div>

            <!-- 主贴底部统计行（截图对齐）：浏览量 / 回复数 / 参与用户 +
                 回复者头像栈。用户数与头像来自已加载评论（客户端渐进增强，
                 加载完成后出现）；头像最多 8 个，超出折叠为 +N。 -->
            <footer class="topic-stats" aria-label="帖子数据">
              <div class="topic-stats__item" aria-label="{formatCount(post.view_count ?? 0)} 次浏览">
                <strong class="topic-stats__num">{formatCount(post.view_count ?? 0)}</strong>
                <span class="topic-stats__label">浏览量</span>
              </div>
              <div class="topic-stats__item" aria-label="{formatCount(replyCount)} 条回复">
                <strong class="topic-stats__num">{formatCount(replyCount)}</strong>
                <span class="topic-stats__label">回复</span>
              </div>
              {#if commentsLoaded}
                <div class="topic-stats__item" aria-label="{formatCount(repliers.length)} 位用户参与回复">
                  <strong class="topic-stats__num">{formatCount(repliers.length)}</strong>
                  <span class="topic-stats__label">用户</span>
                </div>
                {#if repliers.length > 0}
                  <div class="topic-stats__repliers" aria-label="回复过的用户">
                    {#each shownRepliers as r (r.key)}
                      {#if r.username}
                        <UserCard
                          user={{ username: r.username, display_name: r.name, level: r.level, avatar_attachment_id: r.avatarAttachmentId }}
                          presentation={r.presentation}
                          label="查看 {r.name} 的个人资料"
                        >
                          <CosmeticAvatar
                            name={r.name}
                            size="sm"
                            presentation={r.presentation}
                            seed={r.username ?? r.name}
                            avatarAttachmentId={r.avatarAttachmentId}
                          />
                        </UserCard>
                      {:else}
                        <CosmeticAvatar
                          name={r.name}
                          size="sm"
                          presentation={r.presentation}
                          seed={r.username ?? r.name}
                          avatarAttachmentId={r.avatarAttachmentId}
                        />
                      {/if}
                    {/each}
                    {#if hiddenRepliers > 0}
                      <span class="topic-stats__more" title="还有 {hiddenRepliers} 位用户参与回复">+{hiddenRepliers}</span>
                    {/if}
                  </div>
                {/if}
              {/if}
            </footer>
          {/if}
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

        {#if !commentsLoaded}
          <div class="app-empty" role="status" aria-live="polite" style="min-height:150px;">
            <aui-spinner aria-label="正在加载回复"></aui-spinner>
            <span>正在加载回复…</span>
          </div>
        {:else if commentsProblem}
          <div class="app-notice is-danger" role="alert">
            <span>{problemMessage(commentsProblem) || '回复加载失败，请稍后重试'}</span>
            <Button text="重新加载" variant="secondary" size="sm" onclick={() => void loadComments()} />
          </div>
        {:else if comments.length > 0}
          {#if visibleComments.length === 0}
            <div class="card"><div class="card-body"><EmptyState icon="message-square" title="没有作者回复" desc="换个排序看看全部回复" /></div></div>
          {:else}
            <div class="comment-list" style="display:flex;flex-direction:column;gap:var(--space-3);">
              {#each visibleComments as comment (comment.id)}
                <div class="card" id="floor-{comment.floor}" style="scroll-margin-top:calc(var(--header-h, 56px) + 12px);">
                  <div class="card-body" style="padding:var(--space-4);">
                    <div style="display:flex;align-items:center;gap:var(--space-2);margin-bottom:var(--space-2);">
                      {#if comment.author?.username}
                        <!-- 头像+名称挂 UserCard 悬浮卡（与侧栏「关于作者」一致）；
                             数据用评论投影内的公开字段（username/display_name/level），
                             bio/signature 缺省由 UserHoverCard 自然省略。 -->
                        <UserCard
                          user={{
                            username: comment.author.username,
                            display_name: comment.author.display_name ?? null,
                            level: comment.author.level
                          }}
                          presentation={{ presentation_tokens: commentPresentation(comment) }}
                          label="查看 {authorLabel(comment)} 的个人资料"
                        >
                          <span style="display:inline-flex;align-items:center;gap:var(--space-2);">
                            <CosmeticAvatar name={authorLabel(comment)} size="sm" presentation={commentPresentation(comment)} avatarAttachmentId={comment.author?.avatar_attachment_id ?? null} seed={comment.author?.username ?? comment.author?.id ?? authorLabel(comment)} />
                            <strong style="color:var(--color-text-primary);font-size:var(--text-sm);">
                              <CosmeticName name={authorLabel(comment)} presentation={commentPresentation(comment)} />
                            </strong>
                          </span>
                        </UserCard>
                      {:else}
                        <CosmeticAvatar name={authorLabel(comment)} size="sm" presentation={commentPresentation(comment)} avatarAttachmentId={comment.author?.avatar_attachment_id ?? null} seed={comment.author?.username ?? comment.author?.id ?? authorLabel(comment)} />
                        <strong style="color:var(--color-text-primary);font-size:var(--text-sm);">
                          <CosmeticName name={authorLabel(comment)} presentation={commentPresentation(comment)} />
                        </strong>
                      {/if}
                      <span class="text-secondary" style="font-size:var(--text-sm);">
                        <span style="margin:0 var(--space-1);">·</span>
                        {formatRelative(comment.created_at)}
                      </span>
                      <a
                        href="#floor-{comment.floor}"
                        class="badge badge-neutral floor-badge"
                        style="margin-left:auto;"
                        onclick={(e) => scrollToFloor(e, comment.floor)}
                        title="第 {comment.floor} 楼"
                        aria-label="第 {comment.floor} 楼"
                      >
                        #{comment.floor}
                      </a>
                    </div>

                    {#if editingId === comment.id}
                      <div class="input-wrapper">
                        <label class="input-label" for="edit-comment-{comment.id}">编辑回复</label>
                        <SimpleCommentEditor
                          id="edit-comment-{comment.id}"
                          bind:value={editText}
                          placeholder="编辑回复内容…（支持直接粘贴或拖入图片）"
                          rows={4}
                          maxChars={10000}
                        />
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
                      {@const quoted = quotedParentOf(comment)}
                      {#if quoted}
                        <a
                          class="comment-quote"
                          href="#floor-{quoted.floor}"
                          aria-label="查看被引用的第 {quoted.floor} 楼（{authorLabel(quoted)}）"
                          onclick={(e) => scrollToFloor(e, quoted.floor)}
                        >
                          <span class="comment-quote-head">
                            <Icon name="quote" size={12} />
                            <span>回复 @{authorLabel(quoted)} · #{quoted.floor}楼</span>
                          </span>
                          {#if quoted.body_html}
                            <div class="prose comment-quote-body"><SafeHtml html={quoted.body_html} /></div>
                          {/if}
                        </a>
                      {/if}
                      <div class="prose" style="font-size:var(--text-base);">
                        {#if comment.body_html}
                          <SafeHtml html={comment.body_html} />
                        {:else}
                          <p class="text-tertiary" style="font-size:var(--text-sm);">内容不可见</p>
                        {/if}
                      </div>
                      <div class="comment-reaction-bar-wrap" style="margin-top:var(--space-2);width:100%;">
                        <ReactionBar
                          targetType="comment"
                          targetId={comment.id}
                          reactions={[
                            { reaction: 'like', count: commentLikes[comment.id]?.count ?? 0, active: commentLikes[comment.id]?.active ?? false },
                            { reaction: 'doge', count: commentDoges[comment.id]?.count ?? 0, active: commentDoges[comment.id]?.active ?? false }
                          ]}
                          authed={authed}
                          currentUser={user}
                          fetchFn={fetch}
                          onReactionMutated={(p) => applyCommentReactionMutation(comment.id, p)}
                        >
                          {#snippet rightActions()}
                            <Button text="引用" variant="ghost" size="sm" icon="quote" onclick={() => quoteComment(comment)} disabled={locked} />
                            <!-- M18-MISC-02：逐条回复举报入口（对齐原型） -->
                            <a
                              href={authed
                                ? `/moderation/report?target_type=comment&target_id=${encodeURIComponent(comment.id)}`
                                : `/login?next=${encodeURIComponent(`/moderation/report?target_type=comment&target_id=${comment.id}`)}`}
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
                          {/snippet}
                        </ReactionBar>
                      </div>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        {:else}
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
        {:else if authed}
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
              <SimpleCommentEditor
                id="comment-input"
                bind:value={newComment}
                placeholder="写下你的回复…（支持直接 Ctrl+V 粘贴或拖入图片）"
                rows={4}
                maxChars={10000}
                disabled={submitting}
              />
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
                signature: authorCard.signature
              }}
              presentation={{ presentation_tokens: authorCard.presentation_tokens }}
              label="查看 {authorCard.display_name || authorCard.username} 的个人资料"
            >
              <span style="display:flex;align-items:center;gap:var(--space-3);">
                <CosmeticAvatar name={authorCard.display_name || authorCard.username} size="md" presentation={authorCard.presentation_tokens} avatarAttachmentId={authorCard.avatar_attachment_id} seed={authorCard.username} />
                <span>
                  <span style="display:block;font-weight:var(--weight-semibold);">
                    <CosmeticName name={authorCard.display_name || authorCard.username} presentation={authorCard.presentation_tokens} />
                  </span>
                  <span class="text-secondary" style="font-size:var(--text-sm);">LV.{authorCard.level}</span>
                </span>
              </span>
            </UserCard>
            {#if authorCard.signature}
              <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">{authorCard.signature}</p>
            {/if}
            <!-- 统计 → 对应页面（与用户悬浮卡/用户主页统计一致）。 -->
            <div style="display:flex;gap:var(--space-4);font-size:var(--text-sm);">
              <a
                class="profile-stat-link"
                href="/users/{encodeURIComponent(authorCard.username)}?tab=posts"
              >
                <strong>{formatCount(authorCard.post_count)}</strong> <span class="text-secondary">帖子</span>
              </a>
              <a
                class="profile-stat-link"
                href="/users/{encodeURIComponent(authorCard.username)}/followers"
              >
                <strong>{formatCount(authorCard.followers)}</strong> <span class="text-secondary">粉丝</span>
              </a>
              <a
                class="profile-stat-link"
                href="/users/{encodeURIComponent(authorCard.username)}/following"
              >
                <strong>{formatCount(authorCard.following)}</strong> <span class="text-secondary">关注</span>
              </a>
            </div>
          {:else if post.author?.username}
            <!-- 作者卡数据拉取失败：退回帖子投影内的最小作者信息（无统计）。 -->
            <UserCard
              user={{
                username: post.author.username,
                display_name: post.author.display_name ?? null,
                level: post.author.level,
                avatar_attachment_id: post.author.avatar_attachment_id ?? null
              }}
              presentation={{ presentation_tokens: authorPresentation }}
              label="查看 {authorName} 的个人资料"
            >
              <span style="display:flex;align-items:center;gap:var(--space-3);">
                <CosmeticAvatar name={authorName} size="md" presentation={authorPresentation} avatarAttachmentId={post.author?.avatar_attachment_id ?? null} seed={post.author?.username ?? post.author?.id ?? authorName} />
                <span style="font-weight:var(--weight-semibold);">
                  <CosmeticName name={authorName} presentation={authorPresentation} />
                </span>
              </span>
            </UserCard>
          {:else}
            <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">匿名作者</p>
          {/if}

          <!-- 原型对齐：主题功能按钮位于侧栏作者卡（.topic-actions--side 2×2）。 -->
          <div class="topic-actions topic-actions--side">
            {#if authed}
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
            {:else}
              <!-- 匿名：收藏/点赞是登录操作，不渲染按钮，展示登录引导
                   （?next= 登录后回跳本帖）。用 Button ghost（与下方
                   分享/举报同款无边框样式），长文案独占整行不换行。 -->
              <Button
                text="登录后可收藏 / 点赞"
                variant="ghost"
                size="sm"
                icon="bookmark"
                href="/login?next={encodeURIComponent(`/posts/${post.id}`)}"
                extraClass="topic-login-cta"
              />
              <!-- 编辑入口收敛到正文卡右上角「编辑内容」（同样 canEdit 门控），
                   侧栏不再重复放置编辑按钮。 -->
            {/if}
            <a
                href={`/posts/${encodeURIComponent(post.id)}`}
                class="btn ghost btn-ghost sm"
                onclick={(event) => void copyShare(event)}
                aria-label="分享帖子"
              >
                <Icon name="share-2" size={14} />
                <span>分享</span>
              </a>
            <Button
              text={authed ? '举报' : '登录后举报'}
              variant="ghost"
              size="sm"
              icon="flag"
              href={authed
                ? `/moderation/report?post=${encodeURIComponent(post.id)}`
                : `/login?next=${encodeURIComponent(`/moderation/report?post=${post.id}`)}`}
            />
          </div>
          {#if favoriteCount > 0}
            <p class="app-muted" style="margin:8px 0 0;font-size:12px;">{formatCount(favoriteCount)} 人收藏</p>
          {/if}
        </div>
      </div>

      <!-- 板块导航（共享 BoardNav）：帖子所在板块高亮，读帖时可直接跳转
           其他板块/全部板块；数据与「所在板块」同源（GET /boards 反查），
           无 board_id/拉取失败 → 空列表，整卡不渲染。 -->
      <div style="margin-top:var(--space-4);">
        <BoardNav boards={navBoards} activeSlug={boardCard?.slug ?? null} />
      </div>
    </div>
  {/if}
</div>

<style>
  /* 未登录引导 CTA：长文案在侧栏操作区独占整行。
     - 主题网格布局（chinese-elegance 的 2 列 grid）→ grid-column 跨全行；
     - 兜底 flex 布局（其他主题）→ flex-basis 100% 同样独占一行；
     - nowrap 防止「登录后可收藏 / 点赞」在格子里断成两截。
     类名经 Button extraClass 运行时注入，须用 :global 才不会被作用域剪枝。 */
  .topic-actions--side :global(.topic-login-cta) {
    grid-column: 1 / -1;
    flex: 1 1 100%;
    justify-content: center;
    white-space: nowrap;
  }

  /* 楼层编号链接徽标：单元素展示，悬停提供平滑反馈。 */
  .floor-badge {
    margin-left: auto;
    text-decoration: none !important;
    cursor: pointer;
  }
  .floor-badge:hover {
    color: var(--color-brand) !important;
    background: var(--color-surface-hover, var(--color-bg-subtle)) !important;
    border-color: var(--color-border-strong, var(--aui-border)) !important;
    text-decoration: none !important;
  }

  /* 引用回复块：紧凑引用卡（左竖线 + 弱化底色），点击跳转被引用楼层。
     颜色全部走主题 token，日夜模式自动适配。 */
  .comment-quote {
    display: block;
    margin-bottom: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-left: 3px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
    text-decoration: none;
  }
  .comment-quote:hover {
    background: var(--color-surface-hover, var(--color-bg-subtle));
    text-decoration: none;
  }
  .comment-quote-head {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium, 500);
    color: var(--color-text-tertiary);
  }
  .comment-quote-body {
    margin-top: var(--space-1);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  /* 侧栏主题点赞按钮（.btn.ghost 基座）的激活态。 */
  .topic-like-btn.is-active {
    color: var(--color-brand);
    border-color: var(--color-brand);
  }

  /* 主贴底部统计行：数字+标签纵排（对齐截图），头像栈轻微重叠。
     颜色全部走主题 token，日夜模式自动适配；flex-wrap 兜底窄屏。 */
  .topic-stats {
    display: flex;
    align-items: center;
    gap: var(--space-5);
    flex-wrap: wrap;
    margin-top: var(--space-4);
    padding-top: var(--space-4);
    border-top: var(--border-default);
  }
  .topic-stats__item {
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    min-width: 48px;
  }
  .topic-stats__num {
    font-size: var(--text-lg);
    line-height: 1.2;
    font-weight: var(--weight-semibold, 600);
    color: var(--color-brand);
  }
  .topic-stats__label {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
  }
  .topic-stats__repliers {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
  }
  /* 相邻头像/折叠徽标重叠（触发器与头像本体都可能是直接子节点）。 */
  .topic-stats__repliers > :global(* + *) {
    margin-left: -8px;
  }
  .topic-stats__repliers :global(.avatar),
  .topic-stats__more {
    box-shadow: 0 0 0 2px var(--color-bg-card);
  }
  .topic-stats__more {
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: var(--radius-full);
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
    font-size: 10px;
    font-weight: var(--weight-semibold, 600);
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
    background: var(--color-bg-raised);
    color: var(--color-text-secondary);
    cursor: pointer;
  }
  :global(.code-copy-btn:hover) {
    color: var(--color-text-primary);
    border-color: var(--color-border-strong);
  }

  @media (max-width: 768px) {
    :global(.topic-layout) {
      grid-template-columns: minmax(0, 1fr) !important;
      gap: var(--space-4) !important;
    }
    :global(.topic-head-card) {
      padding: var(--space-3) !important;
    }
    :global(.topic-reading-card) {
      border-radius: var(--aui-radius-sm, 4px) !important;
    }
  }
</style>
