<script lang="ts">
  import { onMount } from 'svelte';
  import { getMe, type User, type Problem } from '$lib/api/client';
  import Button from '$lib/components/ui/Button.svelte';
  import Card from '$lib/components/ui/Card.svelte';

  let user = $state<User | null>(null);
  let loading = $state(true);
  let displayName = $state('');
  let saved = $state(false);
  let error = $state('');
  let saving = $state(false);

  onMount(async () => {
    user = await getMe(fetch);
    displayName = user?.display_name || '';
    loading = false;
  });

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = '';
    saved = false;
    try {
      const response = await fetch('/api/v1/me', {
        method: 'PATCH',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ display_name: displayName.trim() || null })
      });
      if (!response.ok) {
        const problem = (await response.json().catch(() => null)) as Problem | null;
        throw new Error(problem?.detail || '保存失败');
      }
      user = await response.json();
      saved = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : '保存失败';
    }
    saving = false;
  }
</script>

<svelte:head>
  <title>账号设置 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">账号设置</span>
  </nav>

  <div class="settings-layout">
    <nav class="settings-nav" aria-label="设置导航">
      <a href="/settings" class="settings-nav-item is-active">基本资料</a>
      <a href="/me" class="settings-nav-item">我的主页</a>
      <a href="/notifications" class="settings-nav-item">通知</a>
    </nav>

    <div class="settings-content">
      {#if loading}
        <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
      {:else if user}
        <form class="card" onsubmit={handleSubmit}>
          <div class="card-header"><span class="card-title">基本资料</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
            <div class="input-wrapper">
              <label class="input-label" for="set-display-name">昵称</label>
              <input type="text" class="input-field" id="set-display-name" placeholder="显示昵称" bind:value={displayName} maxlength="32" />
              <p class="input-hint">用于帖子、回复和主页展示；留空则使用用户名。</p>
            </div>
            {#if error}<p class="input-hint is-error" role="alert">{error}</p>{/if}
            {#if saved}<p class="input-hint" role="status">已保存</p>{/if}
            <div>
              <Button text={saving ? '保存中…' : '保存修改'} variant="primary" size="sm" type="submit" disabled={saving} />
            </div>
          </div>
        </form>

        <div class="card">
          <div class="card-header"><span class="card-title">账号信息</span></div>
          <div class="card-body">
            <dl class="profile-about-list">
              <div class="profile-about-item"><dt>用户名</dt><dd>{user.username}</dd></div>
              <div class="profile-about-item"><dt>邮箱</dt><dd>{user.email}</dd></div>
              <div class="profile-about-item"><dt>状态</dt><dd>{user.status}</dd></div>
            </dl>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
