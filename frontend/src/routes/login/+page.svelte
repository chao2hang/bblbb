<script lang="ts">
  import { goto } from '$app/navigation';
  import { login } from '$lib/api/client';
  import { problemText, type Problem } from '$lib/errors';
  import Button from '$lib/components/ui/Button.svelte';

  let identifier = $state('');
  let password = $state('');
  let error = $state('');
  let submitting = $state(false);

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!identifier.trim() || !password) return;
    submitting = true;
    error = '';
    try {
      await login(fetch, identifier.trim(), password);
      goto('/');
    } catch (err: unknown) {
      error = problemText(err as Problem);
    }
    submitting = false;
  }
</script>

<svelte:head>
  <title>登录 — BBLBB</title>
</svelte:head>

<div class="auth-wrapper">
  <div class="auth-card">
    <div class="auth-header">
      <div class="auth-logo">BBLBB</div>
      <div class="auth-title">欢迎回来</div>
      <div class="auth-subtitle">登录你的账号继续探索</div>
    </div>
    <div class="auth-body">
      <form class="auth-form" onsubmit={handleSubmit}>
        <div class="input-wrapper">
          <label class="input-label" for="login-identifier">用户名或邮箱</label>
          <input type="text" class="input-field" id="login-identifier" placeholder="用户名或邮箱" bind:value={identifier} autocomplete="username" />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="login-password">密码</label>
          <input type="password" class="input-field" id="login-password" placeholder="输入密码" bind:value={password} autocomplete="current-password" />
        </div>
        {#if error}<p class="input-hint is-error" role="alert">{error}</p>{/if}
        <Button text={submitting ? '登录中…' : '登录'} variant="primary" size="lg" type="submit" disabled={submitting} />
      </form>
    </div>
    <div class="auth-footer">
      还没有账号？<a href="/register">立即注册</a>
    </div>
  </div>
</div>
