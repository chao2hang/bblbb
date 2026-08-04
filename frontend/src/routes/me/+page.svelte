<script lang="ts">
  import { onMount } from 'svelte';
  import { getMe, type User } from '$lib/api/client';
  import { goto } from '$app/navigation';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Card from '$lib/components/ui/Card.svelte';

  let user = $state<User | null>(null);
  let loading = $state(true);

  onMount(async () => {
    user = await getMe(fetch);
    loading = false;
  });

  $effect(() => {
    if (!loading && !user) goto('/login');
  });
</script>

<svelte:head>
  <title>我的主页 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">我的主页</span>
  </nav>

  {#if loading}
    <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
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
          <p class="profile-bio">这是我的个人主页</p>
        </div>
        <div class="profile-actions">
          <Button text="编辑资料" variant="secondary" size="sm" icon="edit-3" href="/settings" />
        </div>
      </div>
    </div>

    <div class="content-grid" style="margin-top:var(--space-5);">
      <div class="main-col">
        <div class="card">
          <div class="card-header"><span class="card-title">账号信息</span></div>
          <div class="card-body">
            <dl class="profile-about-list">
              <div class="profile-about-item"><dt>用户名</dt><dd>{user.username}</dd></div>
              <div class="profile-about-item"><dt>邮箱</dt><dd>{user.email}</dd></div>
              <div class="profile-about-item"><dt>邮箱验证</dt><dd>{user.email_verified ? '已验证' : '未验证'}</dd></div>
              <div class="profile-about-item"><dt>账号状态</dt><dd>{user.status}</dd></div>
            </dl>
          </div>
        </div>
      </div>
      <div class="side-col">
        <div class="card">
          <div class="card-header"><span class="card-title">快捷操作</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
            <Button text="发布新帖" variant="primary" size="sm" icon="pen-line" href="/editor" />
            <Button text="账号设置" variant="secondary" size="sm" icon="settings" href="/settings" />
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>
