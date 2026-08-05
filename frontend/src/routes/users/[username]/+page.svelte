<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { getUser, type PublicProfile } from '$lib/api/client';
  import { type Problem } from '$lib/errors';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import ProblemState from '$lib/components/ProblemState.svelte';

  let username = $derived(page.params.username);
  let user = $state<PublicProfile | null>(null);
  let loading = $state(true);
  let problem = $state<Problem | null>(null);

  onMount(async () => {
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
      <div class="profile-cover" role="img" aria-label="个人资料背景"></div>
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
            </dl>
          </div>
        </div>
      </div>
      <div class="side-col"></div>
    </div>
  {/if}
</div>
