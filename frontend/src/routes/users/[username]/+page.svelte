<script lang="ts">
  // M03-UI-01：用户主页 SSR——公开资料安全投影
  //
  // - SSR 主路径：+page.server.ts load 服务端取 `getPublicUser` 公开投影
  //   （九字段 allowlist），页面直接渲染 data.user（无 JS 也可读）；
  // - 不存在/已注销/匿名化 → load 抛 error(404)（不泄漏存在性）；
  // - banned/pending_delete → 后端 200 安全降级投影（bio/signature/头像/
  //   Cover 置空），页面隐藏缺失字段，不输出任何状态字段；
  // - 资料隐私：页面只渲染 allowlist 公开字段；对抗性响应（混入邮箱/状态/
  //   凭据）也不会进入 DOM（客户端兜底路径同守卫，见 user-page-privacy.test）。
  import { onMount, untrack } from 'svelte';
  import { page } from '$app/state';
  import { getUser, type PublicProfile } from '$lib/api/client';
  import { type Problem } from '$lib/errors';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import ProfileCover from '$lib/components/ui/ProfileCover.svelte';
  import ProblemState from '$lib/components/ProblemState.svelte';
  import type { UserPageData } from './+page.server';

  let { data = { user: null } }: { data?: UserPageData | { user: null } } = $props();

  let username = $derived(page.params.username);
  // SSR 已取到 → 直接用；load 不可用（直接客户端导航/测试）时客户端兜底。
  // data 是每次导航重建页面时提供的一次性 SSR 初值，非响应式输入，因此用
  // untrack 显式声明“仅取初值”，避免 state_referenced_locally 噪音。
  let user = $state<PublicProfile | null>(untrack(() => data.user ?? null));
  let loading = $state(untrack(() => data.user === null));
  let problem = $state<Problem | null>(null);

  onMount(async () => {
    if (user) {
      loading = false;
      return;
    }
    if (!username) {
      loading = false;
      return;
    }
    try {
      user = await getUser(fetch, username);
    } catch (err: unknown) {
      problem = err as Problem;
    }
    loading = false;
  });
</script>

<svelte:head>
  <title>{username} — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">{username}</span>
  </nav>

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
        </div>
      </div>
    </div>

    <div class="content-grid" style="margin-top:var(--space-5);">
      <div class="main-col">
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
      <div class="side-col"></div>
    </div>
  {/if}
</div>
