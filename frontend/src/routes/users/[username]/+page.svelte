<script lang="ts">
  // M03-UI-01：用户主页 SSR——公开资料安全投影
  //
  // - SSR 主路径：+page.server.ts load 服务端取公开投影（九字段 allowlist +
  //   GAP-FIX 社交统计 post_count/followers/following/is_following），页面
  //   直接渲染 data.user（无 JS 也可读）；
  // - 不存在/已注销/匿名化 → load 抛 error(404)（不泄漏存在性）；
  // - banned/pending_delete → 后端 200 安全降级投影（bio/signature/头像/
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
  import { type Problem } from '$lib/errors';
  import { show } from '$lib/ui/toast';
  import { formatCount, formatRelative } from '$lib/utils';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import ProblemState from '$lib/components/ProblemState.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  // M14-SEO-01/02：作者页统一 SEO；banned/pending_delete 降级投影 → noindex。
  import Seo from '$lib/components/Seo.svelte';
  import type { UserFollowActionData, UserPageData } from './+page.server';

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
  }: { data?: UserPageData | { user: null }; form?: UserFollowActionData | null } = $props();

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

  onMount(async () => {
    if (data.user) {
      loading = false;
    } else if (username) {
      try {
        clientUser = await getUser(fetch, username);
      } catch (err: unknown) {
        problem = err as Problem;
      }
      loading = false;
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
</script>

<Seo
  title={`${user?.display_name || user?.username || username} 的主页`}
  description={user?.bio || `查看 ${user?.username || username} 在 BBLBB 的公开资料`}
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

<div class="container page-content">

  {#if loading}
    <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
  {:else if problem}
    <ProblemState {problem} desc="用户可能已注销或不存在" />
  {:else if user}
    <div class="card profile-page-card">
      <ProfileCover class="profile-cover" label="个人资料背景" />
      <div class="profile-header">
        <div class="profile-avatar">
          <Avatar name={user.display_name || user.username} size="xl" />
        </div>
        <div class="profile-info">
          <div class="profile-name">
            {user.display_name || user.username}
            <span class="badge badge-level">LV.{user.level}</span>
          </div>
          <p class="profile-bio">@ {user.username}</p>
          <!-- GAP-FIX 社交统计行：post_count/followers/following（BE-1 已随
               PublicProfile 返回；字段缺失（旧后端/降级投影）时整行跳过，
               不显示误导性的 0）。 -->
          {#if typeof user.post_count === 'number'}
            <p class="profile-stats text-secondary" style="margin:var(--space-1) 0 0;font-size:var(--text-sm);display:flex;gap:var(--space-3);flex-wrap:wrap;">
              <span>帖子 <strong style="font-variant-numeric:tabular-nums;">{formatCount(user.post_count)}</strong></span>
              <span>粉丝 <strong style="font-variant-numeric:tabular-nums;">{formatCount(user.followers ?? null)}</strong></span>
              <span>关注 <strong style="font-variant-numeric:tabular-nums;">{formatCount(user.following ?? null)}</strong></span>
              {#if user.created_at}
                <span>加入于 {formatRelative(toSeconds(user.created_at))}</span>
              {/if}
            </p>
          {/if}
        </div>
        <div class="profile-actions">
          {#if isOwner}
            <!-- 本人页：编辑资料入口（客户端 getMe 识别）。 -->
            <a class="btn btn-secondary btn-sm" href="/settings">编辑资料</a>
          {:else if authed}
            {#if user.is_following}
              <form method="POST" action="?/unfollow" use:enhance={followEnhance('已取消关注')}>
                <button type="submit" class="btn btn-ghost btn-sm">已关注 · 取消</button>
              </form>
            {:else}
              <form method="POST" action="?/follow" use:enhance={followEnhance('已关注')}>
                <button type="submit" class="btn btn-primary btn-sm">+ 关注</button>
              </form>
            {/if}
          {:else}
            <!-- 匿名：关注是登录操作，不渲染表单，展示登录引导
                 （?next= 登录后回跳本人页；后端 401 兜底不变）。 -->
            <a class="btn btn-primary btn-sm" href="/login?next={encodeURIComponent(`/users/${username}`)}">登录后关注</a>
          {/if}
        </div>
      </div>
    </div>

    <!-- 内容 tabs（?tab=，无 JS 下为普通链接导航）。 -->
    <nav class="tabs-nav" aria-label="内容分类" style="display:flex;gap:var(--space-1);margin-top:var(--space-5);border-bottom:var(--border-default);flex-wrap:wrap;">
      {#each TABS as item (item.key)}
        <a
          href={tabHref(item.key)}
          class="tab-link"
          aria-current={tab === item.key ? 'page' : undefined}
          style="padding:var(--space-2) var(--space-3);font-size:var(--text-sm);border-bottom:2px solid {tab === item.key ? 'var(--color-primary)' : 'transparent'};color:{tab === item.key ? 'var(--color-text)' : 'var(--color-text-secondary)'};text-decoration:none;"
        >
          {item.label}
        </a>
      {/each}
    </nav>

    <div class="content-grid" style="margin-top:var(--space-4);">
      <div class="main-col">
        {#if tab === 'posts'}
          <div class="card">
            <div class="card-header">
              <span class="card-title">内容</span>
              {#if user.post_count !== undefined && user.post_count !== null}
                <span class="text-secondary" style="font-size:var(--text-sm);">共 {formatCount(user.post_count)} 篇</span>
              {/if}
            </div>
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
                <div style="display:flex;flex-direction:column;">
                  {#each posts as post (post.id)}
                    <div class="post-row" style="padding:var(--space-3) var(--space-4);border-bottom:var(--border-default);">
                      <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
                        <span class="badge badge-neutral">内容</span>
                        {#if post.board_name}
                          <span class="text-secondary" style="font-size:var(--text-xs);">{post.board_name}</span>
                        {/if}
                      </div>
                      <div style="font-weight:var(--weight-medium);margin-top:var(--space-1);">
                        <a href="/posts/{encodeURIComponent(post.id)}">{post.title}</a>
                      </div>
                      <div class="text-secondary" style="font-size:var(--text-xs);margin-top:2px;display:flex;gap:var(--space-2);flex-wrap:wrap;">
                        <span>{formatCount(post.reply_count)} 回复</span>
                        <span>·</span>
                        <span>{formatCount(post.view_count)} 浏览</span>
                        <span>·</span>
                        <span>{formatRelative(toSeconds(post.created_at))}</span>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {:else if tab === 'replies' || tab === 'favorites' || tab === 'activity'}
          <!-- 回复/收藏/动态：后端暂无对应的用户侧列表端点
               （GAP-FIX-SPEC 四节预留：GET /users/{username}/comments 等），
               先以上线占位说明呈现，端点落地后替换为列表。 -->
          <div class="card">
            <div class="card-header"><span class="card-title">{TABS.find((t) => t.key === tab)?.label ?? '内容'}</span></div>
            <div class="card-body">
              <EmptyState
                icon="clock"
                title="即将上线"
                desc="该内容分类的接口尚未开放，功能上线后会在这里展示"
              />
            </div>
          </div>
        {/if}
      </div>
      <div class="side-col">
        <div class="card">
          <div class="card-header"><span class="card-title">个人资料</span></div>
          <div class="card-body">
            <dl class="profile-about-list">
              <div class="profile-about-item"><dt>昵称</dt><dd>{user.display_name || user.username}</dd></div>
              <div class="profile-about-item"><dt>用户名</dt><dd>{user.username}</dd></div>
              <div class="profile-about-item"><dt>等级</dt><dd>LV.{user.level}</dd></div>
              {#if user.bio}
                <div class="profile-about-item"><dt>简介</dt><dd>{user.bio}</dd></div>
              {/if}
              {#if user.signature}
                <div class="profile-about-item"><dt>签名</dt><dd>{user.signature}</dd></div>
              {/if}
            </dl>
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>
