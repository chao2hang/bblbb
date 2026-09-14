<script lang="ts">
  // M03-UI-01：用户主页 SSR——公开资料安全投影
  //
  // - SSR 主路径：+page.server.ts load 服务端取公开投影（九字段 allowlist +
  //   GAP-FIX 社交统计 post_count/followers/following/is_following），页面
  //   直接渲染 data.user（无 JS 也可读）；
  // - 不存在/已注销/匿名化 → load 抛 error(404)（不泄漏存在性）；
  // - banned/pending_delete → 后端 200 安全降级投影（signature/头像/
  //   Cover 置空），页面隐藏缺失字段，不输出任何状态字段；
  // - 资料隐私：页面只渲染 allowlist 公开字段；对抗性响应（混入邮箱/状态/
  //   凭据）也不会进入 DOM（客户端兜底路径同守卫，见 user-page-privacy.test）；
  // - GAP-FIX 既有页面增强：统计 meta 行 + 关注/取关按钮（?/follow、
  //   ?/unfollow 原生 form + use:enhance → toast + invalidateAll；本人页
  //   客户端识别后显示「编辑资料」）+ 内容 tabs（?tab= posts/replies/
  //   favorites/activity；内容 tab 客户端直连 GET /posts?author_username=；
  //   回复/收藏/动态 后端暂无端点 →「即将上线」占位说明）。
  //   原型对齐：文章/话题类型筛选已移除（产品不再区分文章类型），原「帖子」
  //   tab 改名「内容」，列表不按 post_type 过滤。
  import { onMount, untrack } from 'svelte';
  import { page } from '$app/state';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import type { SubmitFunction } from '@sveltejs/kit';
  import { getUser, getMe, type PublicProfile } from '$lib/api/client';
  import type { PostSummary } from '$lib/api/types';
  import { isTransientProblem, type Problem } from '$lib/errors';
  import { announceTransientProblem } from '$lib/ui/problem-toast';
  import { show } from '$lib/ui/toast';
  import { formatCount, formatRelative } from '$lib/utils';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import ProblemState from '$lib/components/ProblemState.svelte';
  import LoadFailureState from '$lib/components/LoadFailureState.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  // M14-SEO-01/02：作者页统一 SEO；banned/pending_delete 降级投影 → noindex。
  import Seo from '$lib/components/Seo.svelte';
  import type { UserFollowActionData, UserMessageActionData, UserPageData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';

  /** GAP-FIX 社交统计扩展：后端 PublicProfile 已附带（BE-1），前端
   * PublicProfile 契约类型尚未收口——在此局部扩展，字段缺失时安全降级。 */
  type PublicProfileWithStats = PublicProfile & {
    post_count?: number | null;
    followers?: number | null;
    following?: number | null;
    is_following?: boolean | null;
  };

  let {
    data = { user: null },
    form
  }: {
    data?: (UserPageData | { user: null }) & { site?: SiteCopyView | null };
    form?: (UserFollowActionData | UserMessageActionData) | null;
  } = $props();

  let username = $derived(page.params.username ?? '');
  // SSR 已取到 → 直接用 load 数据（invalidateAll 后随 data 刷新，关注态/
  // 统计即时反映）；load 不可用（直接客户端渲染/测试）时 onMount 兜底。
  let clientUser = $state<PublicProfile | null>(null);
  let user = $derived<PublicProfileWithStats | null>(
    (data.user as PublicProfileWithStats | null) ?? clientUser
  );
  // 仅取 SSR 初值（一次性判断是否需要客户端兜底拉取）。
  let loading = $state(untrack(() => data.user === null));
  let problem = $state<Problem | null>(null);

  // 本人视角（客户端识别：getMe 对比用户名）→ 显示「编辑资料」而非关注按钮。
  let isOwner = $state(false);

  // 登录态（load 投影 authed，SSR 即知；测试隔离渲染可缺省 → false）：
  // 关注是会话操作，匿名渲染登录引导而非关注表单。
  const authed = $derived((data as { authed?: boolean }).authed === true);

  /** 客户端兜底/重试拉取公开资料（data.user 缺失时使用；重试按钮复用）。 */
  async function loadProfile(): Promise<void> {
    if (!username) return;
    loading = true;
    problem = null;
    try {
      clientUser = await getUser(fetch, username);
    } catch (err: unknown) {
      problem = err as Problem;
    }
    loading = false;
  }

  onMount(async () => {
    if (data.user) {
      loading = false;
    } else if (username) {
      void loadProfile();
    } else {
      loading = false;
    }
    // 本人检测 best-effort：失败（未登录/网络错误）一律按非本人渲染。
    try {
      const me = await getMe(fetch);
      if (me && me.username === username) isOwner = true;
    } catch {
      isOwner = false;
    }
  });

  // 瞬态服务端错误（5xx/429）→ 全局 Toast 提示，页面只留「加载失败·重试」
  // 占位（产品约定：不整页展示错误态）；持续性错误（如 404 用户不存在）
  // 仍走 ProblemState。
  $effect(() => {
    void problem;
    announceTransientProblem(problem);
  });

  // ── 内容 tabs（GAP-FIX：?tab= URL 参数） ─────────────────────────────────

  /** 内容 / 回复 / 收藏 / 动态（默认内容）。 */
  let tab = $derived((page.url.searchParams?.get('tab') ?? 'posts').trim() || 'posts');

  const TABS = [
    { key: 'posts', label: '内容' },
    { key: 'replies', label: '回复' },
    { key: 'favorites', label: '收藏' },
    { key: 'activity', label: '动态' }
  ] as const;

  function tabHref(tabKey: string): string {
    return `/users/${encodeURIComponent(username)}?tab=${tabKey}`;
  }

  // ── 内容列表（客户端直连 GET /api/v1/posts?author_username=；load 返回
  //    形状保持 { user }（load.test.ts 契约），列表因此放客户端，无 JS 下
  //    内容 tab 显示引导文案。） ───────────────────────────────────────────

  let posts = $state<PostSummary[]>([]);
  let postsLoading = $state(false);
  /** 是否完成过至少一次客户端拉取（区分加载中与无 JS 的静态渲染）。 */
  let postsLoaded = $state(false);
  let postsError = $state<string | null>(null);

  async function fetchPosts(): Promise<void> {
    if (!username) return;
    postsLoading = true;
    postsError = null;
    try {
      const params = new URLSearchParams({ author_username: username, limit: '20' });
      const response = await fetch(`/api/v1/posts?${params.toString()}`, {
        headers: { Accept: 'application/json' },
        credentials: 'same-origin'
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const data = (await response.json()) as { items?: PostSummary[] };
      posts = Array.isArray(data.items) ? data.items : [];
    } catch {
      posts = [];
      postsError = '帖子加载失败，请稍后重试';
    }
    postsLoading = false;
    postsLoaded = true;
  }

  // tab / 用户变化 → 重取帖子（SSR 不执行 $effect，测试渲染安全）。
  $effect(() => {
    void tab;
    void user;
    if (tab === 'posts') void fetchPosts();
  });

  /** 后端时间戳为毫秒（M01-DB-08），formatRelative 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }

  /** 关注/取关 enhance 回调：成功 toast + 刷新（load 重取 is_following/统计）。 */
  function followEnhance(message: string): SubmitFunction {
    return () => {
      return async ({ result, update }) => {
        if (result.type === 'success') {
          show(message, 'success');
          await update();
          await invalidateAll();
        } else if (result.type === 'failure') {
          show(
            String((result.data as UserFollowActionData | undefined)?.message ?? '操作失败，请重试'),
            'danger'
          );
          await update();
        } else {
          // redirect（401 → /login）等交由框架应用。
          await update();
        }
      };
    };
  }

  /** 发私信成功后由 server action 跳入已创建/复用的会话；失败保留在主页。 */
  function messageEnhance(): SubmitFunction {
    return () => {
      return async ({ result, update }) => {
        if (result.type === 'failure') {
          show(
            String((result.data as UserMessageActionData | undefined)?.message ?? '发私信失败，请重试'),
            'danger'
          );
        }
        await update();
      };
    };
  }

  const actionMessage = $derived(form?.ok ? null : form?.message ?? null);
</script>

<Seo
  title={`${user?.display_name || user?.username || username} 的主页`}
  description={user?.signature || `查看 ${user?.username || username} 的公开资料`}
  og={{ type: 'profile' }}
  noindex={!user}
  jsonLd={
    user
      ? {
          '@context': 'https://schema.org',
          '@type': 'ProfilePage',
          mainEntity: {
            '@type': 'Person',
            name: user.display_name || user.username,
            identifier: user.username
          }
        }
      : null
  }
/>

<div class="container page-content" id="page-user">

  {#if loading}
    <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
  {:else if problem && isTransientProblem(problem)}
    <LoadFailureState onretry={() => void loadProfile()} />
  {:else if problem}
    <ProblemState {problem} desc="用户可能已注销或不存在" />
  {:else if user}
    <section class="app-profile">
      <ProfileCover attachmentId={user.cover_attachment_id} label="个人资料背景" />
      <CosmeticAvatar name={user.display_name || user.username} size="xl" presentation={user.presentation_tokens} avatarAttachmentId={user.avatar_attachment_id} seed={user.username ?? user.id} />
      <div class="app-profile__body">
        <h2>
          <CosmeticName name={user.display_name || user.username} presentation={user.presentation_tokens} />
          <span class="badge badge-level">TL{user.level}</span>
        </h2>
        <p class="profile-bio">@ {user.username}</p>
        {#if user.signature}
          <p class="profile-sig">{user.signature}</p>
        {/if}
        {#if typeof user.post_count === 'number'}
          <!-- 统计行 → 对应页面（与悬浮卡统计一致）：帖子 → 内容 tab；
               关注者/正在关注 → 关系列表页。加入时间为纯文本。 -->
          <div class="app-profile__meta">
            <a class="profile-stat-link" href={`/users/${encodeURIComponent(username)}?tab=posts`}>
              <b>{formatCount(user.post_count)}</b> 帖子
            </a>
            <a class="profile-stat-link" href={`/users/${encodeURIComponent(username)}/followers`}>
              <b>{formatCount(user.followers ?? null)}</b> 关注者
            </a>
            <a class="profile-stat-link" href={`/users/${encodeURIComponent(username)}/following`}>
              <b>{formatCount(user.following ?? null)}</b> 正在关注
            </a>
            {#if user.created_at}
              <span>加入于 {formatRelative(toSeconds(user.created_at))}</span>
            {/if}
          </div>
        {/if}
      </div>
      <div class="app-profile__actions">
        {#if isOwner}
          <!-- 本人页：4 入口按钮（对齐原型 IA：编辑资料/我的收藏/我的余额/我的装扮）。 -->
          <a class="btn secondary sm" href="/settings">编辑资料</a>
          <a class="btn secondary sm" href="/favorites">我的收藏</a>
          <a class="btn secondary sm" href="/me/balance">我的余额</a>
          <a class="btn secondary sm" href="/me/wardrobe">我的装扮</a>
        {:else if authed}
          <form method="POST" action="?/message" use:enhance={messageEnhance()}>
            <button type="submit" class="btn secondary sm">发私信</button>
          </form>
          {#if user.is_following}
            <form method="POST" action="?/unfollow" use:enhance={followEnhance('已取消关注')}>
              <button type="submit" class="btn ghost sm">已关注 · 取消</button>
            </form>
          {:else}
            <form method="POST" action="?/follow" use:enhance={followEnhance('已关注')}>
              <button type="submit" class="btn primary sm">+ 关注</button>
            </form>
          {/if}
        {:else}
          <!-- 匿名：关注是登录操作，不渲染表单，展示登录引导
               （?next= 登录后回跳本人页；后端 401 兜底不变）。 -->
          <a class="btn secondary sm" href="/login?next={encodeURIComponent(`/users/${username}`)}">登录后私信</a>
          <a class="btn primary sm" href="/login?next={encodeURIComponent(`/users/${username}`)}">登录后关注</a>
        {/if}
      </div>
      {#if actionMessage}
        <p class="input-hint is-error profile-action-message" role="alert">{actionMessage}</p>
      {/if}
    </section>

    <!-- 内容 tabs（?tab=，无 JS 下为普通链接导航）。 -->
    <nav class="tabs-nav" aria-label="内容分类" style="display:flex;align-items:center;gap:var(--space-1);margin-top:var(--space-5);border-bottom:var(--border-default);overflow-x:auto;scrollbar-width:none;flex-wrap:nowrap;">
      {#each TABS as item (item.key)}
        <a
          href={tabHref(item.key)}
          class="tab-link"
          aria-current={tab === item.key ? 'page' : undefined}
          style="padding:var(--space-2) var(--space-3);font-size:var(--text-sm);border-bottom:2px solid {tab === item.key ? 'var(--color-brand)' : 'transparent'};color:{tab === item.key ? 'var(--color-text-primary)' : 'var(--color-text-secondary)'};text-decoration:none;white-space:nowrap;flex-shrink:0;"
        >
          {item.label}
        </a>
      {/each}
      {#if tab === 'posts' && user.post_count !== undefined && user.post_count !== null}
        <span class="text-secondary" style="margin-left:auto;font-size:var(--text-sm);padding-right:var(--space-2);white-space:nowrap;">
          共 {formatCount(user.post_count)} 篇
        </span>
      {/if}
    </nav>

    <div class="user-post-content" style="margin-top:var(--space-4);">
      <div class="main-col">
        {#if tab === 'posts'}
          <div class="card">
            <div class="card-body" style="padding:0;">
              {#if postsLoading}
                <div class="empty-state"><div class="empty-state-title">帖子加载中…</div></div>
              {:else if postsError}
                <div style="padding:var(--space-4);">
                  <p class="input-hint is-error" role="alert">{postsError}</p>
                </div>
              {:else if !postsLoaded}
                <!-- 无 JS / 客户端拉取尚未发生：列表为客户端直连，静态渲染下
                     给出说明而非误导性的“没有帖子”。 -->
                <div style="padding:var(--space-6);">
                  <EmptyState
                    icon="file-text"
                    title="帖子列表"
                    desc="帖子列表需浏览器加载后显示；你也可以从首页信息流找到该用户的内容"
                  />
                </div>
              {:else if posts.length === 0}
                <div style="padding:var(--space-6);">
                  <EmptyState icon="file-text" title="还没有帖子" desc="该用户还没有发布过内容，或者你无权查看" />
                </div>
              {:else}
                <div class="app-post-list" role="feed" aria-label="用户发布的帖子">
                  {#each posts as post (post.id)}
                    {@const authorName = post.author?.display_name || post.author_display_name || post.author?.username || post.author_name || user.display_name || user.username}
                    <div class="app-post-row" data-post-id={post.id}>
                      <CosmeticAvatar name={authorName} size="md" presentation={user.presentation_tokens} avatarAttachmentId={user.avatar_attachment_id} seed={post.author?.username ?? post.author?.id ?? user.username ?? user.id} />
                      <div class="app-post-row__main">
                        <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
                          {#if post.pinned}
                            <Badge text="置顶" type="pinned" />
                          {/if}
                          {#if post.board_slug && post.board_name}
                            <span class="category-badge" style="--cat-color:{boardVisuals(post.board_slug).color};">
                              <span class="category-badge-square"></span>
                              <span>{post.board_name}</span>
                            </span>
                          {:else if post.board_name}
                            <span class="sbadge sb-gray">{post.board_name}</span>
                          {/if}
                        </div>
                        <h3 style="margin-top:var(--space-1);">
                          <a href="/posts/{encodeURIComponent(post.id)}">{post.title}</a>
                        </h3>
                        {#if post.summary}
                          <p>
                            {post.summary}
                          </p>
                        {/if}
                        <div class="app-post-row__meta">
                          <span>{formatCount(post.reply_count)} 回复</span>
                          <span>·</span>
                          <span>{formatCount(post.view_count)} 浏览</span>
                          {#if post.created_at}
                            <span>·</span>
                            <span>{formatRelative(toSeconds(post.created_at))}</span>
                          {/if}
                        </div>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {:else if tab === 'replies' || tab === 'favorites' || tab === 'activity'}
          <!-- 回复/收藏/动态：后端暂无对应的用户侧列表端点
               明确功能未开放与无数据边界（P3-01），避免误导用户。 -->
          <div class="card">
            <div class="card-body">
              <EmptyState
                icon="clock"
                title="功能尚未开放"
                desc="公开「{TABS.find((t) => t.key === tab)?.label ?? '内容'}」功能规划中，当前暂未开放，非无数据状态。"
              />
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
